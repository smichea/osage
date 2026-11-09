// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Replication gateway: authenticates clients, issues agent delegations and
//! relays messages between sessions and the region (ADR-0001 §2). The
//! transports — [`crate::websocket`] and [`crate::webtransport`] — hand it
//! decoded client messages and carry its server messages.

use std::{
    net::TcpListener as StdTcpListener,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use crossbeam_channel::Sender;
use osage_credentials::{DelegatedRole, DelegationKey};
use osage_proto::{
    PROTOCOL_VERSION,
    actions::{DELEGATE_AGENT, DELEGATE_BRIDGE},
    v0::{
        ActionRequest, ActionResult, ActionStatus, AssuranceLevel, ClientMessage, Goodbye, GoodbyeReason,
        ServerMessage, SessionIdentity, SourceOffer, Welcome, WorldDescriptor, client_message, server_message,
    },
};
use tokio::{
    net::TcpListener,
    sync::mpsc::{Receiver, UnboundedSender},
    task::JoinHandle,
    time::timeout,
};

use crate::{
    Config, RegionInput,
    identity::{Identity, Refusal, Registry, SessionKind, credential_level, privileges},
    now_us,
    region::Member,
    schema, websocket, webtransport,
};

pub(crate) const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Client messages a session buffers before its transport waits.
pub(crate) const INBOX: usize = 256;
/// How long the last server messages (e.g. a refusal) may take to leave.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);
const DELEGATION_LIFETIME: Duration = Duration::from_secs(3600);

pub(crate) struct Gateway {
    pub(crate) config: Config,
    region: Sender<RegionInput>,
    next_session: AtomicU64,
    next_delegation: AtomicU64,
    pub(crate) delegations: DelegationKey,
    pub(crate) registry: Registry,
    /// What the WebSocket host announces of the WebTransport endpoint.
    pub(crate) webtransport: Option<webtransport::Advert>,
}

pub async fn serve(listener: StdTcpListener, config: Config, region: Sender<RegionInput>) -> Result<()> {
    let listener = TcpListener::from_std(listener)?;
    let endpoint = match &config.webtransport {
        Some(webtransport) => Some(webtransport::bind(webtransport).await?),
        None => None,
    };
    let gateway = Arc::new(Gateway {
        delegations: DelegationKey::new(&config.world_name, &config.delegation_secret),
        config,
        region,
        next_session: AtomicU64::new(1),
        next_delegation: AtomicU64::new(1),
        registry: Registry::default(),
        webtransport: endpoint.as_ref().map(|(_, advert)| advert.clone()),
    });
    if let Some((endpoint, _)) = endpoint {
        tokio::spawn(webtransport::serve(endpoint, gateway.clone()));
    }
    tracing::info!(address = %listener.local_addr()?, "gateway listening (WebSocket)");
    websocket::serve(listener, gateway).await
}

impl Gateway {
    /// Runs a session with a transport's two pumps: `reader` feeds `incoming`
    /// until the connection ends; `writer` drains `out`'s receiver.
    pub(crate) async fn run(
        &self,
        transport: &'static str,
        incoming: Receiver<ClientMessage>,
        out: UnboundedSender<ServerMessage>,
        reader: JoinHandle<()>,
        mut writer: JoinHandle<()>,
    ) -> Result<()> {
        let result = self.session(transport, incoming, out).await;
        reader.abort();
        if timeout(FLUSH_TIMEOUT, &mut writer).await.is_err() {
            writer.abort();
        }
        result
    }

    /// One session, whatever its transport: `incoming` ends when the
    /// connection does (or sends a malformed frame); `out` is carried to the
    /// client until every sender — this session's and the region's — is gone.
    async fn session(
        &self,
        transport: &'static str,
        mut incoming: Receiver<ClientMessage>,
        out: UnboundedSender<ServerMessage>,
    ) -> Result<()> {
        let hello = match timeout(HELLO_TIMEOUT, incoming.recv()).await? {
            Some(ClientMessage { message: Some(client_message::Message::Hello(hello)) }) => hello,
            Some(_) => return Err(anyhow!("first client message must be Hello")),
            None => return Err(anyhow!("connection closed before Hello")),
        };

        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        let admitted = if hello.protocol_versions.iter().any(|v| v == PROTOCOL_VERSION) {
            self.authenticate(session, &hello).and_then(|identity| self.registry.admit(&self.config, identity))
        } else {
            Err(Refusal(GoodbyeReason::Version, format!("this gateway speaks {PROTOCOL_VERSION}")))
        };
        let admission = match admitted {
            Ok(admission) => admission,
            Err(Refusal(reason, detail)) => {
                tracing::info!(session, ?reason, %detail, "session refused");
                out.send(goodbye(reason, detail))?;
                return Ok(());
            }
        };
        let identity = admission.identity().clone();

        out.send(self.welcome(session, &identity))?;
        let member = Member {
            kind: identity.kind,
            role: hello.role(),
            controller: identity.controller(),
            owner: identity.responsible.clone().unwrap_or_default(),
        };
        self.region.send(RegionInput::Join { session, member, out: out.clone() })?;
        tracing::info!(session, who = %identity.per_world_id, client = ?hello.client, transport, "session joined");

        // Whatever ends the connection — Goodbye, close, network error or a
        // malformed frame — the region is told the session left.
        while let Some(message) = incoming.recv().await {
            match message.message {
                Some(client_message::Message::Goodbye(_)) => break,
                Some(client_message::Message::ActionRequest(request))
                    if [DELEGATE_AGENT, DELEGATE_BRIDGE].contains(&request.action.as_str()) =>
                {
                    self.delegate(&identity, &request, &out);
                }
                Some(message) => self.region.send(RegionInput::Message { session, message })?,
                None => {}
            }
        }

        self.region.send(RegionInput::Leave { session })?;
        drop(admission);
        tracing::info!(session, "session left");
        Ok(())
    }

    /// Issues a delegation to a human: for one agent (needs `run_agents`, A2
    /// by default) or for one device bridge (needs an account, A1).
    fn delegate(&self, identity: &Identity, request: &ActionRequest, out: &UnboundedSender<ServerMessage>) {
        let mut result = ActionResult { request_id: request.request_id, ..Default::default() };
        let (role, prefix, allowed, requirement) = if request.action == DELEGATE_AGENT {
            let allowed = privileges(identity.level).iter().any(|p| p == "run_agents");
            (DelegatedRole::Agent, "agent", allowed, "running agents requires the run_agents privilege (A2 by default)")
        } else {
            (
                DelegatedRole::Bridge,
                "bridge",
                identity.level >= AssuranceLevel::A1,
                "bridging devices requires an account (A1)",
            )
        };
        match credential_level(identity.level) {
            Some(level) if allowed && identity.kind == SessionKind::Human => {
                let holder = format!(
                    "{prefix}-{}.{}",
                    self.next_delegation.fetch_add(1, Ordering::Relaxed),
                    identity.per_world_id
                );
                match self.delegations.issue(role, &holder, &identity.per_world_id, level, DELEGATION_LIFETIME) {
                    Ok(token) => {
                        result.status = ActionStatus::Ok.into();
                        result.result = token.into_bytes();
                    }
                    Err(error) => {
                        result.status = ActionStatus::Failed.into();
                        result.reason = error.to_string();
                    }
                }
            }
            _ => {
                result.status = ActionStatus::Rejected.into();
                result.reason = requirement.to_owned();
            }
        }
        let _ = out.send(ServerMessage { message: Some(server_message::Message::ActionResult(result)) });
    }

    fn welcome(&self, session: u64, identity: &Identity) -> ServerMessage {
        let world = WorldDescriptor {
            name: self.config.world_name.clone(),
            tick_rate_hz: self.config.tick_hz,
            content_base_url: String::new(),
            source_offer: Some(SourceOffer {
                url: self.config.source_url.clone(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            }),
            earth_frame: None,
        };
        ServerMessage {
            message: Some(server_message::Message::Welcome(Welcome {
                protocol_version: PROTOCOL_VERSION.to_owned(),
                session_id: session,
                identity: Some(SessionIdentity {
                    per_world_id: identity.per_world_id.clone(),
                    level: identity.level.into(),
                    issuer: identity.issuer.clone(),
                    privileges: privileges(identity.level),
                }),
                server_time_us: now_us(),
                world: Some(world),
                schemas: Some(schema::bundle()),
            })),
        }
    }
}

fn goodbye(reason: GoodbyeReason, detail: String) -> ServerMessage {
    ServerMessage { message: Some(server_message::Message::Goodbye(Goodbye { reason: reason.into(), detail })) }
}
