// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use osage_proto::{
    PROTOCOL_VERSION, WEBTRANSPORT_DISCOVERY_PATH,
    framing::{FrameReader, frame},
    v0::{
        ActionRequest, ActionStatus, Anonymous, Capabilities, ClientInfo, ClientMessage, Component, Credentials,
        DeviceState, GoodbyeReason, Hello, Intent, Role, ServerMessage, Tier, Welcome, client_message, credentials,
        server_message,
    },
};
use prost::Message as _;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use wtransport::{
    ClientConfig, Connection, Endpoint, RecvStream, SendStream, endpoint::endpoint_side::Client, tls::Sha256Digest,
};

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

/// How a session reaches its gateway (ADR-0002 §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    WebSocket,
    WebTransport,
}

/// A gateway's WebTransport endpoint, as announced by [`discover`].
#[derive(Clone, Debug)]
pub struct WebTransportEndpoint {
    pub url: String,
    /// SHA-256 hashes of the gateway's self-signed certificate (development);
    /// empty: the certificate must be valid for the web PKI.
    pub certificate_hashes: Vec<[u8; 32]>,
}

/// The gateway refused the session: its `Goodbye`.
#[derive(Debug)]
pub struct Refused {
    pub reason: GoodbyeReason,
    pub detail: String,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "refused by the gateway: {:?} {}", self.reason, self.detail)
    }
}

impl std::error::Error for Refused {}

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);
const WEBTRANSPORT_TIMEOUT: Duration = Duration::from_secs(3);
/// A gateway silent this long has vanished; the keep-alive keeps a quiet link open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEP_ALIVE: Duration = Duration::from_secs(3);

/// An open session with an osage gateway.
pub struct Session {
    welcome: Welcome,
    sender: SessionSender,
    receiver: SessionReceiver,
}

/// Sending half of a [`Session`], see [`Session::split`].
pub struct SessionSender(Sender);

/// Receiving half of a [`Session`], see [`Session::split`].
pub struct SessionReceiver(Receiver);

enum Sender {
    WebSocket(SplitSink<Socket, Message>),
    WebTransport { stream: SendStream, connection: Connection, _endpoint: Arc<Endpoint<Client>> },
}

enum Receiver {
    WebSocket(SplitStream<Socket>),
    /// Fed by a task reading the stream and the datagrams; aborted on drop.
    WebTransport {
        messages: mpsc::Receiver<Result<ServerMessage>>,
        task: JoinHandle<()>,
        datagrams: Arc<AtomicU64>,
    },
}

impl Drop for Receiver {
    fn drop(&mut self) {
        if let Receiver::WebTransport { task, .. } = self {
            task.abort();
        }
    }
}

/// Connects to the world at `url`, sends `hello` and returns once the gateway
/// has answered with a `Welcome`. For a `ws://` URL, uses the gateway's
/// WebTransport endpoint if it announces one and it answers, WebSocket
/// otherwise; `https://` is WebTransport only.
pub async fn connect(url: &str, hello: Hello) -> Result<Session> {
    if url.starts_with("https://") {
        let endpoint = WebTransportEndpoint { url: url.to_owned(), certificate_hashes: Vec::new() };
        return connect_webtransport(&endpoint, hello).await;
    }
    if let Ok(Ok(Some(endpoint))) = timeout(DISCOVERY_TIMEOUT, discover(url)).await {
        match timeout(WEBTRANSPORT_TIMEOUT, connect_webtransport(&endpoint, hello.clone())).await {
            Ok(Ok(session)) => return Ok(session),
            Ok(Err(error)) if error.is::<Refused>() => return Err(error),
            _ => {} // UDP blocked, endpoint down…: WebSocket.
        }
    }
    connect_websocket(url, hello).await
}

/// Asks the gateway at `url` (`ws://`) for its WebTransport endpoint;
/// `None` if it has none.
pub async fn discover(url: &str) -> Result<Option<WebTransportEndpoint>> {
    let Some(rest) = url.strip_prefix("ws://") else { return Ok(None) };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let mut stream = TcpStream::connect(authority).await.with_context(|| format!("connecting to {authority}"))?;
    let request =
        format!("GET {WEBTRANSPORT_DISCOVERY_PATH} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    stream.take(64 * 1024).read_to_end(&mut response).await?;
    let response = String::from_utf8_lossy(&response);
    let (head, body) = response.split_once("\r\n\r\n").context("malformed discovery answer")?;
    if head.split(' ').nth(1) != Some("200") {
        return Ok(None);
    }
    let advert: serde_json::Value = serde_json::from_str(body).context("malformed discovery answer")?;
    let url = advert["url"].as_str().context("discovery answer without url")?.to_owned();
    let certificate_hashes = advert["certificateHashes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|hash| hash.as_str().and_then(from_hex).context("malformed certificate hash"))
        .collect::<Result<_>>()?;
    Ok(Some(WebTransportEndpoint { url, certificate_hashes }))
}

/// Connects over WebSocket (`ws://…`) only.
pub async fn connect_websocket(url: &str, hello: Hello) -> Result<Session> {
    let (socket, _) = connect_async(url).await.with_context(|| format!("connecting to {url}"))?;
    let (sink, stream) = socket.split();
    let (sender, receiver) = (SessionSender(Sender::WebSocket(sink)), SessionReceiver(Receiver::WebSocket(stream)));
    handshake(sender, receiver, hello).await
}

/// Connects over WebTransport only.
pub async fn connect_webtransport(endpoint: &WebTransportEndpoint, hello: Hello) -> Result<Session> {
    let config = ClientConfig::builder().with_bind_default();
    let config = if endpoint.certificate_hashes.is_empty() {
        config.with_native_certs()
    } else {
        config.with_server_certificate_hashes(endpoint.certificate_hashes.iter().map(|h| Sha256Digest::new(*h)))
    };
    let config = config.keep_alive_interval(Some(KEEP_ALIVE)).max_idle_timeout(Some(IDLE_TIMEOUT))?.build();
    let client = Arc::new(Endpoint::client(config)?);
    let connection = client.connect(&endpoint.url).await.with_context(|| format!("connecting to {}", endpoint.url))?;
    let (stream, recv) = connection.open_bi().await?.await?;
    let (to_receiver, messages) = mpsc::channel(1024);
    let datagrams = Arc::new(AtomicU64::new(0));
    let task = tokio::spawn(receive_webtransport(connection.clone(), recv, to_receiver, datagrams.clone()));
    let sender = SessionSender(Sender::WebTransport { stream, connection, _endpoint: client });
    handshake(sender, SessionReceiver(Receiver::WebTransport { messages, task, datagrams }), hello).await
}

async fn handshake(mut sender: SessionSender, mut receiver: SessionReceiver, hello: Hello) -> Result<Session> {
    sender.send(client_message::Message::Hello(hello)).await?;
    loop {
        let first = receiver.next_message().await.ok_or_else(|| anyhow!("connection closed before Welcome"))??;
        return match first.message {
            Some(server_message::Message::Welcome(welcome)) => Ok(Session { welcome, sender, receiver }),
            Some(server_message::Message::Goodbye(goodbye)) => {
                Err(Refused { reason: goodbye.reason(), detail: goodbye.detail }.into())
            }
            // A datagram may overtake the Welcome on the stream; its state is of no use yet.
            Some(server_message::Message::State(_)) => continue,
            _ => bail!("expected Welcome as the first server message"),
        };
    }
}

/// Reads the stream's frames and the datagrams into `to_receiver` until the
/// stream ends; a datagram is dropped rather than waited for.
async fn receive_webtransport(
    connection: Connection,
    mut stream: RecvStream,
    to_receiver: mpsc::Sender<Result<ServerMessage>>,
    datagrams: Arc<AtomicU64>,
) {
    let mut frames = FrameReader::default();
    let mut chunk = vec![0; 64 * 1024];
    loop {
        tokio::select! {
            read = stream.read(&mut chunk) => {
                match read {
                    Ok(Some(n)) => frames.push(&chunk[..n]),
                    Ok(None) => return,
                    Err(error) => return drop(to_receiver.send(Err(error.into())).await),
                }
                loop {
                    let message = match frames.next_frame() {
                        Ok(Some(payload)) => ServerMessage::decode(payload.as_slice()).map_err(Into::into),
                        Ok(None) => break,
                        Err(error) => Err(error.into()),
                    };
                    let failed = message.is_err();
                    if to_receiver.send(message).await.is_err() || failed {
                        return;
                    }
                }
            }
            datagram = connection.receive_datagram() => {
                let Ok(datagram) = datagram else { continue };
                if let Ok(message) = ServerMessage::decode(datagram.payload()) {
                    datagrams.fetch_add(1, Ordering::Relaxed);
                    let _ = to_receiver.try_send(Ok(message));
                }
            }
        }
    }
}

impl Session {
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    pub fn transport(&self) -> Transport {
        self.sender.transport()
    }

    /// Next message from the gateway; `None` once the connection is closed.
    pub async fn next_message(&mut self) -> Option<Result<ServerMessage>> {
        self.receiver.next_message().await
    }

    pub async fn send(&mut self, message: client_message::Message) -> Result<()> {
        self.sender.send(message).await
    }

    /// Splits the session so that sending and receiving can run concurrently.
    pub fn split(self) -> (Welcome, SessionSender, SessionReceiver) {
        (self.welcome, self.sender, self.receiver)
    }
}

impl SessionSender {
    pub fn transport(&self) -> Transport {
        match self.0 {
            Sender::WebSocket(_) => Transport::WebSocket,
            Sender::WebTransport { .. } => Transport::WebTransport,
        }
    }

    pub async fn send(&mut self, message: client_message::Message) -> Result<()> {
        let message = ClientMessage { message: Some(message) };
        match &mut self.0 {
            Sender::WebSocket(sink) => sink.send(Message::binary(message.encode_to_vec())).await?,
            Sender::WebTransport { stream, connection, .. } => {
                // Intents are volatile: a datagram when it fits; the server keeps the latest sequence.
                if matches!(message.message, Some(client_message::Message::Intent(_)))
                    && connection.max_datagram_size().is_some_and(|max| message.encoded_len() <= max)
                    && connection.send_datagram(message.encode_to_vec()).is_ok()
                {
                    return Ok(());
                }
                stream.write_all(&frame(&message)).await?;
            }
        }
        Ok(())
    }
}

impl SessionReceiver {
    /// Next message from the gateway; `None` once the connection is closed.
    pub async fn next_message(&mut self) -> Option<Result<ServerMessage>> {
        match &mut self.0 {
            Receiver::WebSocket(stream) => loop {
                match stream.next().await? {
                    Ok(Message::Binary(bytes)) => return Some(ServerMessage::decode(bytes).map_err(Into::into)),
                    Ok(Message::Close(_)) => return None,
                    Ok(_) => continue,
                    Err(error) => return Some(Err(error.into())),
                }
            },
            Receiver::WebTransport { messages, .. } => messages.recv().await,
        }
    }

    /// Server messages received as datagrams so far (WebTransport only).
    pub fn datagrams(&self) -> u64 {
        match &self.0 {
            Receiver::WebSocket(_) => 0,
            Receiver::WebTransport { datagrams, .. } => datagrams.load(Ordering::Relaxed),
        }
    }
}

fn from_hex(hex: &str) -> Option<[u8; 32]> {
    let mut bytes = [0; 32];
    if hex.len() != 64 {
        return None;
    }
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(bytes)
}
