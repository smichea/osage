// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use osage_proto::{
    PROTOCOL_VERSION,
    v0::{
        Anonymous, Capabilities, ClientInfo, ClientMessage, Credentials, Hello, Role, ServerMessage, Tier, Welcome,
        client_message, credentials, server_message,
    },
};
use prost::Message as _;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

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

/// An open session with an osage gateway.
pub struct Session {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    welcome: Welcome,
}

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
        loop {
            match self.socket.next().await? {
                Ok(Message::Binary(bytes)) => return Some(ServerMessage::decode(bytes).map_err(Into::into)),
                Ok(Message::Close(_)) => return None,
                Ok(_) => continue,
                Err(error) => return Some(Err(error.into())),
            }
        }
    }

    pub async fn send(&mut self, message: client_message::Message) -> Result<()> {
        let frame = ClientMessage { message: Some(message) };
        self.socket.send(Message::binary(frame.encode_to_vec())).await?;
        Ok(())
    }
}
