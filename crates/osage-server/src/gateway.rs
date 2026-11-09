// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Replication gateway: terminates client connections, authenticates them,
//! issues agent delegations and relays messages between sessions and the
//! region (ADR-0001 §2).

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
use futures_util::{SinkExt, StreamExt};
use osage_credentials::{DelegatedRole, DelegationKey};
use osage_proto::{
    PROTOCOL_VERSION,
    actions::{DELEGATE_AGENT, DELEGATE_BRIDGE},
    v0::{
        ActionRequest, ActionResult, ActionStatus, AssuranceLevel, ClientMessage, Goodbye, GoodbyeReason,
        ServerMessage, SessionIdentity, SourceOffer, Welcome, WorldDescriptor, client_message, server_message,
    },
};
use prost::Message as _;
use tokio::{
    net::TcpListener,
    sync::mpsc::{self, UnboundedSender},
    time::timeout,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{
    Config, RegionInput,
    identity::{Identity, Refusal, Registry, SessionKind, credential_level, privileges},
    now_us,
    region::Member,
    schema,
};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const DELEGATION_LIFETIME: Duration = Duration::from_secs(3600);

pub(crate) struct Gateway {
    pub(crate) config: Config,
    region: Sender<RegionInput>,
    next_session: AtomicU64,
    next_delegation: AtomicU64,
    pub(crate) delegations: DelegationKey,
    pub(crate) registry: Registry,
}

pub async fn serve(listener: StdTcpListener, config: Config, region: Sender<RegionInput>) -> Result<()> {
    let listener = TcpListener::from_std(listener)?;
    tracing::info!(address = %listener.local_addr()?, "gateway listening (WebSocket)");
    let gateway = Arc::new(Gateway {
        delegations: DelegationKey::new(&config.world_name, &config.delegation_secret),
        config,
        region,
        next_session: AtomicU64::new(1),
        next_delegation: AtomicU64::new(1),
        registry: Registry::default(),
    });
    loop {
        let (stream, peer) = listener.accept().await?;
        let gateway = gateway.clone();
        tokio::spawn(async move {
            if let Err(error) = gateway.session(stream).await {
                tracing::debug!(%peer, %error, "session ended with an error");
            }
        });
    }
}

impl Gateway {
    async fn session(&self, stream: tokio::net::TcpStream) -> Result<()> {
        let (mut sink, mut source) = accept_async(stream).await?.split();

        let hello = timeout(HELLO_TIMEOUT, async {
            while let Some(frame) = source.next().await {
                if let Message::Binary(bytes) = frame? {
                    return match ClientMessage::decode(bytes)?.message {
                        Some(client_message::Message::Hello(hello)) => Ok(hello),
                        _ => Err(anyhow!("first client message must be Hello")),
                    };
                }
            }
            Err(anyhow!("connection closed before Hello"))
        })
        .await??;

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
                sink.send(Message::binary(goodbye(reason, detail).encode_to_vec())).await?;
                return Ok(());
            }
        };
        let identity = admission.identity().clone();

        let (out, mut outbox) = mpsc::unbounded_channel();
        out.send(self.welcome(session, &identity))?;
        let member = Member {
            kind: identity.kind,
            role: hello.role(),
            controller: identity.controller(),
            owner: identity.responsible.clone().unwrap_or_default(),
        };
        self.region.send(RegionInput::Join { session, member, out: out.clone() })?;
        tracing::info!(session, who = %identity.per_world_id, client = ?hello.client, "session joined");

        let writer = tokio::spawn(async move {
            while let Some(message) = outbox.recv().await {
                if sink.send(Message::binary(message.encode_to_vec())).await.is_err() {
                    break;
                }
            }
        });

        // Whatever ends the connection — Goodbye, close, network error or a
        // malformed frame — the region is told the session left.
        while let Some(Ok(frame)) = source.next().await {
            let Message::Binary(bytes) = frame else { continue };
            let Ok(message) = ClientMessage::decode(bytes) else { break };
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
        writer.abort();
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
