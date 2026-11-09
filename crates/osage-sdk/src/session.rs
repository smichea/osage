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
        ActionRequest, ActionStatus, Anonymous, Capabilities, ClientInfo, ClientMessage, Component, Credentials,
        DeviceState, Hello, Intent, Role, ServerMessage, Tier, Welcome, client_message, credentials, server_message,
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
        client: Some(ClientInfo { implementation: implementation.to_owned(), version: version.to_owned() }),
        role: role.into(),
        capabilities: Some(Capabilities {
            tier: tier.into(),
            max_datagram_size: 0,
            content_kinds: vec!["builtin".to_owned()],
        }),
        credentials: Some(Credentials { credential: Some(credentials::Credential::Anonymous(Anonymous {})) }),
        world: String::new(),
    }
}

/// `hello` presenting an identity credential (D-013) instead of being anonymous.
pub fn with_identity(mut hello: Hello, credential: &str) -> Hello {
    let credential = credentials::Credential::VerifiablePresentation(credential.as_bytes().to_vec());
    hello.credentials = Some(Credentials { credential: Some(credential) });
    hello
}

/// `hello` for an agent presenting the delegation of its responsible human.
pub fn with_delegation(mut hello: Hello, delegation: &str) -> Hello {
    hello.role = Role::Agent.into();
    hello.credentials =
        Some(Credentials { credential: Some(credentials::Credential::Delegation(delegation.as_bytes().to_vec())) });
    hello
}

/// `hello` for a device bridge presenting the delegation of the devices' owner.
pub fn with_bridge(mut hello: Hello, delegation: &str) -> Hello {
    hello.role = Role::Bridge.into();
    hello.credentials =
        Some(Credentials { credential: Some(credentials::Credential::Bridge(delegation.as_bytes().to_vec())) });
    hello
}

/// Connects as the human holding the identity `credential`, sends `action`
/// (`DELEGATE_AGENT` or `DELEGATE_BRIDGE`) and returns the delegation token.
pub async fn request_delegation(url: &str, credential: &str, action: &str) -> Result<String> {
    let hello =
        with_identity(hello("osage-sdk", env!("CARGO_PKG_VERSION"), Role::Observer, Tier::T0Headless), credential);
    let mut session = connect(url, hello).await?;
    let request = ActionRequest { request_id: 1, action: action.to_owned(), ..Default::default() };
    session.send(client_message::Message::ActionRequest(request)).await?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(message) = session.next_message().await {
            if let Some(server_message::Message::ActionResult(result)) = message?.message {
                return Ok(result);
            }
        }
        bail!("connection closed before the delegation")
    })
    .await
    .context("no answer from the world")??;
    if result.status() != ActionStatus::Ok {
        bail!("delegation refused: {}", result.reason);
    }
    let _ = session.send(client_message::Message::Goodbye(Default::default())).await;
    Ok(String::from_utf8(result.result)?)
}

/// A `DeviceState` publishing `components` (type id, encoded value) of the
/// bridge's device `local_id`.
pub fn device_state(local_id: &str, components: Vec<(u32, Vec<u8>)>) -> client_message::Message {
    client_message::Message::DeviceState(DeviceState {
        local_id: local_id.to_owned(),
        components: components.into_iter().map(|(r#type, value)| Component { r#type, value }).collect(),
        removed: false,
    })
}

/// An `ActionRequest` setting `function` of device entity `target` to `value`
/// (the `osage.device.set` action).
pub fn device_set(request_id: u64, target: u64, function: &str, value: f64) -> client_message::Message {
    let arguments = osage_proto::components::DeviceSet { function: function.to_owned(), value };
    client_message::Message::ActionRequest(ActionRequest {
        request_id,
        target,
        action: osage_proto::actions::DEVICE_SET.to_owned(),
        arguments: arguments.encode_to_vec(),
    })
}

/// A bridge's answer to a `DeviceCommand`.
pub fn device_command_result(command_id: u64, status: ActionStatus, reason: &str) -> client_message::Message {
    client_message::Message::DeviceCommandResult(osage_proto::v0::DeviceCommandResult {
        command_id,
        status: status.into(),
        reason: reason.to_owned(),
    })
}

/// An `Intent` carrying `value`, encoded as component type `type_id` of the
/// session (see [`crate::TypeTable::id`]).
pub fn intent(sequence: u64, entity: u64, type_id: u32, value: &impl prost::Message) -> client_message::Message {
    client_message::Message::Intent(Intent {
        sequence,
        entity,
        input: Some(Component { r#type: type_id, value: value.encode_to_vec() }),
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
    let (mut socket, _) = connect_async(url).await.with_context(|| format!("connecting to {url}"))?;
    let first = ClientMessage { message: Some(client_message::Message::Hello(hello)) };
    socket.send(Message::binary(first.encode_to_vec())).await?;

    loop {
        let frame = socket.next().await.ok_or_else(|| anyhow!("connection closed before Welcome"))??;
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
