// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Replication gateway: terminates client connections, runs the handshake and
//! relays messages between sessions and the region (ADR-0001 §2).

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
use osage_proto::{
    PROTOCOL_VERSION,
    v0::{
        AssuranceLevel, ClientMessage, Goodbye, GoodbyeReason, Hello, ServerMessage, SessionIdentity, SourceOffer,
        Welcome, WorldDescriptor, client_message, server_message,
    },
};
use prost::Message as _;
use tokio::{net::TcpListener, sync::mpsc, time::timeout};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{Config, RegionInput, now_us, schema};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

struct Gateway {
    config: Config,
    region: Sender<RegionInput>,
    next_session: AtomicU64,
}

pub async fn serve(listener: StdTcpListener, config: Config, region: Sender<RegionInput>) -> Result<()> {
    let listener = TcpListener::from_std(listener)?;
    tracing::info!(address = %listener.local_addr()?, "gateway listening (WebSocket)");
    let gateway = Arc::new(Gateway {
        config,
        region,
        next_session: AtomicU64::new(1),
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

        if !hello.protocol_versions.iter().any(|v| v == PROTOCOL_VERSION) {
            let goodbye = goodbye(
                GoodbyeReason::Version,
                format!("this gateway speaks {PROTOCOL_VERSION}"),
            );
            sink.send(Message::binary(goodbye.encode_to_vec())).await?;
            return Ok(());
        }

        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        let (out, mut outbox) = mpsc::unbounded_channel();
        out.send(self.welcome(session, &hello))?;
        self.region.send(RegionInput::Join {
            session,
            out: out.clone(),
        })?;
        tracing::info!(session, client = ?hello.client, "session joined");

        let writer = tokio::spawn(async move {
            while let Some(message) = outbox.recv().await {
                if sink.send(Message::binary(message.encode_to_vec())).await.is_err() {
                    break;
                }
            }
        });

        while let Some(frame) = source.next().await {
            let Message::Binary(bytes) = frame? else { continue };
            match ClientMessage::decode(bytes)?.message {
                Some(client_message::Message::Goodbye(_)) => break,
                Some(message) => self.region.send(RegionInput::Message { session, message })?,
                None => {}
            }
        }

        self.region.send(RegionInput::Leave { session })?;
        writer.abort();
        tracing::info!(session, "session left");
        Ok(())
    }

    fn welcome(&self, session: u64, _hello: &Hello) -> ServerMessage {
        // Phase 1 skeleton: every session is an anonymous guest (A0); the
        // verifier of D-013 comes next.
        let identity = SessionIdentity {
            per_world_id: format!("guest-{session}"),
            level: AssuranceLevel::A0.into(),
            issuer: String::new(),
            privileges: vec!["visit".to_owned()],
        };
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
                identity: Some(identity),
                server_time_us: now_us(),
                world: Some(world),
                schemas: Some(schema::bundle()),
            })),
        }
    }
}

fn goodbye(reason: GoodbyeReason, detail: String) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::Goodbye(Goodbye {
            reason: reason.into(),
            detail,
        })),
    }
}
