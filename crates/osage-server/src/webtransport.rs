// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! WebTransport transport (ADR-0002 §3). The client opens one bidirectional
//! stream that carries every reliable message, length-prefixed
//! ([`osage_proto::framing`]); `StateDatagram`s travel as datagrams, cut to
//! the datagram size, and the client's intents may too. Anything else
//! arriving in a datagram is dropped.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use osage_proto::{
    framing::{FrameReader, frame},
    v0::{ClientMessage, EntityComponent, ServerMessage, StateDatagram, client_message, server_message},
};
use prost::Message as _;
use tokio::{
    sync::mpsc::{self, Sender, UnboundedReceiver},
    time::timeout,
};
use wtransport::{
    Connection, Endpoint, Identity, RecvStream, SendStream, ServerConfig, VarInt,
    endpoint::{IncomingSession, endpoint_side::Server},
};

use crate::{
    WebTransportConfig,
    gateway::{Gateway, HELLO_TIMEOUT, INBOX},
};

/// A client silent this long has vanished (QUIC's default is 30 s); the
/// keep-alive makes a quiet but live client answer well before.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEP_ALIVE: Duration = Duration::from_secs(3);

/// What discovery tells clients about the endpoint.
#[derive(Clone, Debug)]
pub(crate) struct Advert {
    pub(crate) port: u16,
    /// Lowercase hexadecimal SHA-256 of a self-signed certificate; empty for
    /// a certificate the web PKI vouches for.
    pub(crate) certificate_hashes: Vec<String>,
}

pub(crate) async fn bind(config: &WebTransportConfig) -> Result<(Endpoint<Server>, Advert)> {
    let (identity, certificate_hashes) = match &config.certificate {
        Some((chain, key)) => (Identity::load_pemfiles(chain, key).await.context("loading the certificate")?, vec![]),
        None => {
            let mut names = vec!["localhost".to_owned(), "127.0.0.1".to_owned(), "::1".to_owned()];
            if !config.address.ip().is_unspecified() {
                names.push(config.address.ip().to_string());
            }
            let identity = Identity::self_signed(names)?;
            let hash = identity.certificate_chain().as_slice()[0].hash();
            let hash = hash.as_ref().iter().map(|b| format!("{b:02x}")).collect();
            (identity, vec![hash])
        }
    };
    let server = ServerConfig::builder()
        .with_bind_address(config.address)
        .with_identity(identity)
        .keep_alive_interval(Some(KEEP_ALIVE))
        .max_idle_timeout(Some(IDLE_TIMEOUT))?
        .build();
    let endpoint = Endpoint::server(server)?;
    let port = endpoint.local_addr()?.port();
    tracing::info!(
        address = %endpoint.local_addr()?,
        certificate = if certificate_hashes.is_empty() { "provided" } else { "self-signed, 14 days" },
        "gateway listening (WebTransport)"
    );
    Ok((endpoint, Advert { port, certificate_hashes }))
}

pub(crate) async fn serve(endpoint: Endpoint<Server>, gateway: Arc<Gateway>) {
    loop {
        let incoming = endpoint.accept().await;
        let gateway = gateway.clone();
        tokio::spawn(async move {
            if let Err(error) = session(incoming, &gateway).await {
                tracing::debug!(%error, "WebTransport session ended with an error");
            }
        });
    }
}

async fn session(incoming: IncomingSession, gateway: &Gateway) -> Result<()> {
    let connection = incoming.await?.accept().await?;
    let (send, recv) = timeout(HELLO_TIMEOUT, connection.accept_bi()).await??;
    let (to_session, incoming) = mpsc::channel(INBOX);
    let (out, outbox) = mpsc::unbounded_channel();
    let reader = tokio::spawn(read(connection.clone(), recv, to_session));
    let writer = tokio::spawn(write(connection.clone(), send, outbox));
    let result = gateway.run("webtransport", incoming, out, reader, writer).await;
    connection.close(VarInt::from_u32(0), b"");
    result
}

/// Feeds the session until the stream ends or carries a malformed frame.
async fn read(connection: Connection, mut stream: RecvStream, to_session: Sender<ClientMessage>) {
    let mut frames = FrameReader::default();
    let mut chunk = vec![0; 16 * 1024];
    loop {
        tokio::select! {
            read = stream.read(&mut chunk) => {
                let Ok(Some(n)) = read else { return };
                frames.push(&chunk[..n]);
                loop {
                    let message = match frames.next_frame() {
                        Ok(Some(payload)) => ClientMessage::decode(payload.as_slice()),
                        Ok(None) => break,
                        Err(_) => return,
                    };
                    let Ok(message) = message else { return };
                    if to_session.send(message).await.is_err() {
                        return;
                    }
                }
            }
            datagram = connection.receive_datagram() => {
                let Ok(datagram) = datagram else { return };
                if let Ok(message) = ClientMessage::decode(datagram.payload())
                    && matches!(message.message, Some(client_message::Message::Intent(_)))
                {
                    // Intents are volatile: dropped rather than waited for when the session is busy.
                    let _ = to_session.try_send(message);
                }
            }
        }
    }
}

/// Carries the session's messages until every sender is gone.
async fn write(connection: Connection, mut stream: SendStream, mut outbox: UnboundedReceiver<ServerMessage>) {
    while let Some(mut message) = outbox.recv().await {
        if let Some(server_message::Message::State(state)) = &message.message
            && let Some(max) = connection.max_datagram_size()
        {
            let (datagrams, too_large) = split_state(state, max);
            let unsent: Vec<_> = datagrams.into_iter().filter(|d| connection.send_datagram(d).is_err()).collect();
            if too_large.is_empty() && unsent.is_empty() {
                continue;
            }
            // What could not leave as a datagram goes on the stream.
            let mut rest = StateDatagram { values: too_large, ..state.clone() };
            for datagram in unsent {
                if let Ok(ServerMessage { message: Some(server_message::Message::State(part)) }) =
                    ServerMessage::decode(datagram.as_slice())
                {
                    rest.values.extend(part.values);
                }
            }
            message = ServerMessage { message: Some(server_message::Message::State(rest)) };
        }
        if stream.write_all(&frame(&message)).await.is_err() {
            return;
        }
    }
    let _ = stream.finish().await;
}

/// Cuts `state` into encoded `ServerMessage`s of at most `max` bytes, each a
/// complete `StateDatagram` of the same tick; values too large for any
/// datagram are returned apart.
fn split_state(state: &StateDatagram, max: usize) -> (Vec<Vec<u8>>, Vec<EntityComponent>) {
    // A `StateDatagram` inside a `ServerMessage`: field tag, length, message.
    let wrapped_len =
        |s: &StateDatagram| 1 + prost::encoding::encoded_len_varint(s.encoded_len() as u64) + s.encoded_len();
    let encode = |s: StateDatagram| ServerMessage { message: Some(server_message::Message::State(s)) }.encode_to_vec();
    let empty = StateDatagram { values: Vec::new(), ..state.clone() };
    let (mut datagrams, mut too_large, mut part) = (Vec::new(), Vec::new(), empty.clone());
    for value in &state.values {
        part.values.push(value.clone());
        if wrapped_len(&part) <= max {
            continue;
        }
        part.values.pop();
        if !part.values.is_empty() {
            datagrams.push(encode(std::mem::replace(&mut part, empty.clone())));
            part.values.push(value.clone());
            if wrapped_len(&part) <= max {
                continue;
            }
            part.values.pop();
        }
        too_large.push(value.clone());
    }
    if !part.values.is_empty() {
        datagrams.push(encode(part));
    }
    (datagrams, too_large)
}

#[cfg(test)]
mod tests {
    use osage_proto::v0::Component;

    use super::*;

    fn value(entity: u64, size: usize) -> EntityComponent {
        EntityComponent { entity, component: Some(Component { r#type: 1, value: vec![7; size] }) }
    }

    #[test]
    fn a_state_is_cut_into_datagrams_of_the_same_tick() {
        let state = StateDatagram {
            tick: 42,
            server_time_us: 1,
            values: (0..100).map(|e| value(e, 40)).chain([value(999, 5000)]).collect(),
        };
        let (datagrams, too_large) = split_state(&state, 1200);
        assert!(datagrams.len() > 1);
        let mut seen = Vec::new();
        for datagram in &datagrams {
            assert!(datagram.len() <= 1200, "{} bytes", datagram.len());
            let Some(server_message::Message::State(part)) =
                ServerMessage::decode(datagram.as_slice()).unwrap().message
            else {
                panic!("not a state");
            };
            assert_eq!((part.tick, part.server_time_us), (42, 1));
            seen.extend(part.values);
        }
        assert_eq!(seen, state.values[..100]);
        assert_eq!(too_large, [value(999, 5000)]);
    }
}
