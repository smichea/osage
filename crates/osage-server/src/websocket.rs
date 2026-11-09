// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! WebSocket transport (ADR-0002 §3): one binary message per protocol
//! message, everything reliable. The same port answers
//! `GET /.well-known/osage/webtransport` with the WebTransport endpoint.

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use osage_proto::{
    WEBTRANSPORT_DISCOVERY_PATH,
    v0::{ClientMessage, ServerMessage},
};
use prost::Message as _;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::{sleep, timeout},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::gateway::{Gateway, HELLO_TIMEOUT, INBOX};

pub(crate) async fn serve(listener: TcpListener, gateway: Arc<Gateway>) -> Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let gateway = gateway.clone();
        tokio::spawn(async move {
            if let Err(error) = connection(stream, &gateway).await {
                tracing::debug!(%peer, %error, "connection ended with an error");
            }
        });
    }
}

async fn connection(mut stream: TcpStream, gateway: &Gateway) -> Result<()> {
    if timeout(HELLO_TIMEOUT, asks_for_discovery(&stream)).await?? {
        return answer_discovery(&mut stream, gateway).await;
    }
    let (mut sink, mut source) = accept_async(stream).await?.split();
    let (to_session, incoming) = mpsc::channel(INBOX);
    let (out, mut outbox) = mpsc::unbounded_channel::<ServerMessage>();
    let reader = tokio::spawn(async move {
        while let Some(Ok(frame)) = source.next().await {
            let Message::Binary(bytes) = frame else { continue };
            let Ok(message) = ClientMessage::decode(bytes) else { break };
            if to_session.send(message).await.is_err() {
                break;
            }
        }
    });
    let writer = tokio::spawn(async move {
        while let Some(message) = outbox.recv().await {
            if sink.send(Message::binary(message.encode_to_vec())).await.is_err() {
                return;
            }
        }
        let _ = sink.close().await;
    });
    gateway.run("websocket", incoming, out, reader, writer).await
}

/// Whether the request on `stream` is the discovery request, read without
/// consuming it so that a WebSocket handshake stays intact.
async fn asks_for_discovery(stream: &TcpStream) -> Result<bool> {
    let wanted = format!("GET {WEBTRANSPORT_DISCOVERY_PATH} ");
    let mut head = [0; 64];
    loop {
        let n = stream.peek(&mut head).await?;
        let seen = &head[..n];
        if n == 0 || !wanted.as_bytes().starts_with(&seen[..n.min(wanted.len())]) {
            return Ok(false);
        }
        if n >= wanted.len() {
            return Ok(true);
        }
        sleep(Duration::from_millis(5)).await; // the request line arrives in pieces
    }
}

/// Answers the discovery request: the WebTransport URL on the host the client
/// used, and the certificate hashes to accept (development certificates).
async fn answer_discovery(stream: &mut TcpStream, gateway: &Gateway) -> Result<()> {
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 8192 {
        let n = timeout(HELLO_TIMEOUT, stream.read(&mut chunk)).await??;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..n]);
    }
    let host = String::from_utf8_lossy(&request)
        .lines()
        .find_map(|line| line.split_once(':').filter(|(name, _)| name.eq_ignore_ascii_case("host")).map(|(_, v)| v))
        .map(|value| hostname(value.trim()).to_owned())
        .unwrap_or_else(|| stream.local_addr().map(|a| a.ip().to_string()).unwrap_or_default());
    let response = match &gateway.webtransport {
        Some(advert) => {
            let body = serde_json::json!({
                "url": format!("https://{host}:{}/", advert.port),
                "certificateHashes": advert.certificate_hashes,
            })
            .to_string();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }
        None => {
            "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned()
        }
    };
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

/// The host of a `Host` header, without its port (`[::1]:7878` → `[::1]`).
fn hostname(host: &str) -> &str {
    match host.strip_prefix('[') {
        Some(rest) => rest.find(']').map_or(host, |end| &host[..end + 2]),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hostname_drops_the_port() {
        assert_eq!(super::hostname("127.0.0.1:7878"), "127.0.0.1");
        assert_eq!(super::hostname("example.org"), "example.org");
        assert_eq!(super::hostname("[::1]:7878"), "[::1]");
        assert_eq!(super::hostname("[::1]"), "[::1]");
    }
}
