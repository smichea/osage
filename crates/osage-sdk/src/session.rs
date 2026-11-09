// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use osage_proto::{
    PROTOCOL_VERSION,
    v0::{
        Anonymous, Capabilities, ClientInfo, ClientMessage, Component, Credentials, Hello, Intent, Role, ServerMessage,
        Tier, Welcome, client_message, credentials, server_message,
    },
};
use prost::Message as _;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A `Hello` for an anonymous client of the given role and tier.
pub fn hello(implementation: &str, version: &str, role: Role, tier: Tier) -> Hello {
    Hello {
        protocol_versions: vec![PROTOCOL_VERSION.to_owned()],
        client: Some(ClientInfo {
            implementation: implementation.to_owned(),
            version: version.to_owned(),
        }),
        role: role.into(),
        capabilities: Some(Capabilities {
            tier: tier.into(),
            max_datagram_size: 0,
            content_kinds: vec!["builtin".to_owned()],
        }),
        credentials: Some(Credentials {
            credential: Some(credentials::Credential::Anonymous(Anonymous {})),
        }),
        world: String::new(),
    }
}

/// An `Intent` carrying `value`, encoded as component type `type_id` of the
/// session (see [`crate::TypeTable::id`]).
pub fn intent(sequence: u64, entity: u64, type_id: u32, value: &impl prost::Message) -> client_message::Message {
    client_message::Message::Intent(Intent {
        sequence,
        entity,
        input: Some(Component {
            r#type: type_id,
            value: value.encode_to_vec(),
        }),
    })
}

/// An open session with an osage gateway.
pub struct Session {
    socket: Socket,
    welcome: Welcome,
}

/// Sending half of a [`Session`], see [`Session::split`].
pub struct SessionSender(SplitSink<Socket, Message>);

/// Receiving half of a [`Session`], see [`Session::split`].
pub struct SessionReceiver(SplitStream<Socket>);

/// Connects to `url` (e.g. `ws://127.0.0.1:7878`), sends `hello` and returns
/// once the gateway has answered with a `Welcome`.
pub async fn connect(url: &str, hello: Hello) -> Result<Session> {
    let (mut socket, _) = connect_async(url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let first = ClientMessage {
        message: Some(client_message::Message::Hello(hello)),
    };
    socket.send(Message::binary(first.encode_to_vec())).await?;

    loop {
        let frame = socket
            .next()
            .await
            .ok_or_else(|| anyhow!("connection closed before Welcome"))??;
        let Message::Binary(bytes) = frame else { continue };
        match ServerMessage::decode(bytes)?.message {
            Some(server_message::Message::Welcome(welcome)) => return Ok(Session { socket, welcome }),
            Some(server_message::Message::Goodbye(goodbye)) => {
                bail!("refused by the gateway: {:?} {}", goodbye.reason(), goodbye.detail)
            }
            _ => bail!("expected Welcome as the first server message"),
        }
    }
}

impl Session {
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// Next message from the gateway; `None` once the connection is closed.
    pub async fn next_message(&mut self) -> Option<Result<ServerMessage>> {
        next_server_message(&mut self.socket).await
    }

    pub async fn send(&mut self, message: client_message::Message) -> Result<()> {
        send_client_message(&mut self.socket, message).await
    }

    /// Splits the session so that sending and receiving can run concurrently.
    pub fn split(self) -> (Welcome, SessionSender, SessionReceiver) {
        let (sink, stream) = self.socket.split();
        (self.welcome, SessionSender(sink), SessionReceiver(stream))
    }
}

impl SessionSender {
    pub async fn send(&mut self, message: client_message::Message) -> Result<()> {
        send_client_message(&mut self.0, message).await
    }
}

impl SessionReceiver {
    /// Next message from the gateway; `None` once the connection is closed.
    pub async fn next_message(&mut self) -> Option<Result<ServerMessage>> {
        next_server_message(&mut self.0).await
    }
}

async fn send_client_message<S>(sink: &mut S, message: client_message::Message) -> Result<()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let frame = ClientMessage { message: Some(message) };
    sink.send(Message::binary(frame.encode_to_vec())).await?;
    Ok(())
}

async fn next_server_message<S>(stream: &mut S) -> Option<Result<ServerMessage>>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match stream.next().await? {
            Ok(Message::Binary(bytes)) => return Some(ServerMessage::decode(bytes).map_err(Into::into)),
            Ok(Message::Close(_)) => return None,
            Ok(_) => continue,
            Err(error) => return Some(Err(error.into())),
        }
    }
}
