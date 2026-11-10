// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The contract between the replication gateway and a region server (D-037):
//! everything they say to each other is one of the messages here, and
//! nothing else passes between them — no shared state, no handle on a
//! session. In one process the messages go through the channels of
//! [`in_process`]; across machines, the same messages go over a connection
//! ([`network`]), encoded by [`ToRegion::encode`] and [`ToGateway::encode`].
//!
//! The gateway owns sessions: it authenticates them, tells the region who
//! joined and left, and relays what they send. The region owns the world: it
//! answers with messages addressed to one session or to all its members, and
//! the gateway's [`Router`] carries them to the sessions' connections.
//!
//! Order is part of the contract. Each direction is one ordered stream. The
//! region announces a session with [`ToGateway::Joined`] before anything it
//! sends that session, and a [`ToGateway::Broadcast`] reaches exactly the
//! sessions the region had joined and not yet let go ([`ToGateway::Left`])
//! when it sent it: a session never gets a broadcast from before it joined.

pub mod network;

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use osage_proto::{
    components::Controller,
    v0::{ClientMessage, Role, SchemaBundle, ServerMessage, client_message},
};
use prost::Message as _;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// What kind of participant a session is, as the gateway authenticated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Human,
    Agent,
    Bridge,
}

/// Who a session is, as far as the region needs to know.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub kind: SessionKind,
    pub role: Role,
    pub controller: Controller,
    /// For a bridge: the per-world identifier of the devices' owner.
    pub owner: String,
}

/// What the gateway tells a region.
#[derive(Clone, Debug, PartialEq)]
pub enum ToRegion {
    /// A session was admitted and enters the region.
    Join { session: u64, member: Member },
    /// The session is gone, however it went.
    Leave { session: u64 },
    /// A message the session sent, for the region.
    Client { session: u64, message: client_message::Message },
}

/// What a region tells the gateway.
#[derive(Clone, Debug, PartialEq)]
pub enum ToGateway {
    /// The region is up: the schemas of its world and its tick rate, which
    /// the gateway gives every session in its `Welcome`. Sent once, first.
    Ready { schemas: SchemaBundle, tick_hz: u32 },
    /// The region has taken the session in: from here on it receives
    /// broadcasts.
    Joined { session: u64 },
    /// The region has let the session go: nothing more is sent to it.
    Left { session: u64 },
    /// A message for one session.
    Send { session: u64, message: ServerMessage },
    /// A message for every session the region has joined.
    Broadcast { message: ServerMessage },
}

/// The gateway's end of a link to a region.
pub struct GatewayEnd {
    pub to_region: crossbeam_channel::Sender<ToRegion>,
    pub from_region: UnboundedReceiver<ToGateway>,
}

/// A region's end of a link to the gateway. The region reads without
/// waiting, once a tick; the gateway, asynchronously.
pub struct RegionEnd {
    pub from_gateway: crossbeam_channel::Receiver<ToRegion>,
    pub to_gateway: UnboundedSender<ToGateway>,
}

/// A link inside one process: two channels.
pub fn in_process() -> (GatewayEnd, RegionEnd) {
    let (to_region, from_gateway) = crossbeam_channel::unbounded();
    let (to_gateway, from_region) = unbounded_channel();
    (GatewayEnd { to_region, from_region }, RegionEnd { from_gateway, to_gateway })
}

/// Carries a region's messages to the sessions' connections, as the contract
/// orders: [`Router::open`] a session's connection before telling the region
/// it joined, then [`Router::deliver`] what the region sends, in order.
#[derive(Default)]
pub struct Router {
    routes: HashMap<u64, Route>,
}

struct Route {
    out: UnboundedSender<ServerMessage>,
    joined: bool,
}

impl Router {
    /// Where the session's messages go. The route closes when the region
    /// lets the session go, dropping its sender.
    pub fn open(&mut self, session: u64, out: UnboundedSender<ServerMessage>) {
        self.routes.insert(session, Route { out, joined: false });
    }

    /// Carries one message from the region. `Ready` is not for sessions and
    /// is ignored here.
    pub fn deliver(&mut self, message: ToGateway) {
        match message {
            ToGateway::Ready { .. } => {}
            ToGateway::Joined { session } => {
                if let Some(route) = self.routes.get_mut(&session) {
                    route.joined = true;
                }
            }
            ToGateway::Left { session } => {
                self.routes.remove(&session);
            }
            ToGateway::Send { session, message } => {
                if let Some(route) = self.routes.get(&session) {
                    let _ = route.out.send(message);
                }
            }
            ToGateway::Broadcast { message } => {
                for route in self.routes.values().filter(|r| r.joined) {
                    let _ = route.out.send(message.clone());
                }
            }
        }
    }

    /// Sessions with an open route, joined or not.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl ToRegion {
    pub fn encode(&self) -> Vec<u8> {
        use wire::to_region::Kind;
        let kind = match self.clone() {
            Self::Join { session, member } => Kind::Join(wire::Join {
                session,
                kind: member.kind as u32,
                role: member.role.into(),
                controller: Some(member.controller),
                owner: member.owner,
            }),
            Self::Leave { session } => Kind::Leave(session),
            Self::Client { session, message } => {
                Kind::Client(wire::Client { session, message: Some(ClientMessage { message: Some(message) }) })
            }
        };
        wire::ToRegion { kind: Some(kind) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        use wire::to_region::Kind;
        Ok(match wire::ToRegion::decode(bytes)?.kind.ok_or_else(|| anyhow!("empty message to a region"))? {
            Kind::Join(join) => Self::Join {
                session: join.session,
                member: Member {
                    kind: match join.kind {
                        0 => SessionKind::Human,
                        1 => SessionKind::Agent,
                        2 => SessionKind::Bridge,
                        other => return Err(anyhow!("unknown session kind {other}")),
                    },
                    role: Role::try_from(join.role).map_err(|_| anyhow!("unknown role {}", join.role))?,
                    controller: join.controller.unwrap_or_default(),
                    owner: join.owner,
                },
            },
            Kind::Leave(session) => Self::Leave { session },
            Kind::Client(client) => Self::Client {
                session: client.session,
                message: client.message.and_then(|m| m.message).ok_or_else(|| anyhow!("empty client message"))?,
            },
        })
    }
}

impl ToGateway {
    pub fn encode(&self) -> Vec<u8> {
        use wire::to_gateway::Kind;
        let kind = match self.clone() {
            Self::Ready { schemas, tick_hz } => Kind::Ready(wire::Ready { schemas: Some(schemas), tick_hz }),
            Self::Joined { session } => Kind::Joined(session),
            Self::Left { session } => Kind::Left(session),
            Self::Send { session, message } => Kind::Send(wire::Send { session, message: Some(message) }),
            Self::Broadcast { message } => Kind::Broadcast(message),
        };
        wire::ToGateway { kind: Some(kind) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        use wire::to_gateway::Kind;
        Ok(match wire::ToGateway::decode(bytes)?.kind.ok_or_else(|| anyhow!("empty message to the gateway"))? {
            Kind::Ready(ready) => Self::Ready { schemas: ready.schemas.unwrap_or_default(), tick_hz: ready.tick_hz },
            Kind::Joined(session) => Self::Joined { session },
            Kind::Left(session) => Self::Left { session },
            Kind::Send(send) => Self::Send {
                session: send.session,
                message: send.message.ok_or_else(|| anyhow!("empty message for a session"))?,
            },
            Kind::Broadcast(message) => Self::Broadcast { message },
        })
    }
}

/// The messages on the wire: Protocol Buffers, like the client protocol, so
/// that a session's messages are carried as they are.
mod wire {
    use osage_proto::{
        components::Controller,
        v0::{ClientMessage, SchemaBundle, ServerMessage},
    };

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ToRegion {
        #[prost(oneof = "to_region::Kind", tags = "1, 2, 3")]
        pub kind: Option<to_region::Kind>,
    }

    pub mod to_region {
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum Kind {
            #[prost(message, tag = "1")]
            Join(super::Join),
            #[prost(uint64, tag = "2")]
            Leave(u64),
            #[prost(message, tag = "3")]
            Client(super::Client),
        }
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Join {
        #[prost(uint64, tag = "1")]
        pub session: u64,
        #[prost(uint32, tag = "2")]
        pub kind: u32,
        #[prost(int32, tag = "3")]
        pub role: i32,
        #[prost(message, optional, tag = "4")]
        pub controller: Option<Controller>,
        #[prost(string, tag = "5")]
        pub owner: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Client {
        #[prost(uint64, tag = "1")]
        pub session: u64,
        #[prost(message, optional, tag = "2")]
        pub message: Option<ClientMessage>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ToGateway {
        #[prost(oneof = "to_gateway::Kind", tags = "1, 2, 3, 4, 5")]
        pub kind: Option<to_gateway::Kind>,
    }

    pub mod to_gateway {
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum Kind {
            #[prost(message, tag = "1")]
            Ready(super::Ready),
            #[prost(uint64, tag = "2")]
            Joined(u64),
            #[prost(uint64, tag = "3")]
            Left(u64),
            #[prost(message, tag = "4")]
            Send(super::Send),
            #[prost(message, tag = "5")]
            Broadcast(osage_proto::v0::ServerMessage),
        }
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Ready {
        #[prost(message, optional, tag = "1")]
        pub schemas: Option<SchemaBundle>,
        #[prost(uint32, tag = "2")]
        pub tick_hz: u32,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Send {
        #[prost(uint64, tag = "1")]
        pub session: u64,
        #[prost(message, optional, tag = "2")]
        pub message: Option<ServerMessage>,
    }
}

#[cfg(test)]
mod tests {
    use osage_proto::v0::{Goodbye, Intent, TimeSync, server_message};

    use super::*;

    fn member(kind: SessionKind) -> Member {
        let controller = Controller { per_world_id: "guest-7".to_owned(), ..Default::default() };
        Member { kind, role: Role::Player, controller, owner: "owner-1".to_owned() }
    }

    fn server(detail: &str) -> ServerMessage {
        let goodbye = Goodbye { detail: detail.to_owned(), ..Default::default() };
        ServerMessage { message: Some(server_message::Message::Goodbye(goodbye)) }
    }

    #[test]
    fn every_message_crosses_the_wire_unchanged() {
        let to_region = [
            ToRegion::Join { session: 1, member: member(SessionKind::Human) },
            ToRegion::Join { session: 2, member: member(SessionKind::Agent) },
            ToRegion::Join { session: 3, member: member(SessionKind::Bridge) },
            ToRegion::Leave { session: 1 },
            ToRegion::Client {
                session: 2,
                message: client_message::Message::Intent(Intent { entity: 9, sequence: 4, ..Default::default() }),
            },
            ToRegion::Client {
                session: 2,
                message: client_message::Message::TimeSync(TimeSync { client_time_us: 5, ..Default::default() }),
            },
        ];
        for message in to_region {
            assert_eq!(ToRegion::decode(&message.encode()).unwrap(), message);
        }
        let to_gateway = [
            ToGateway::Ready { schemas: SchemaBundle { url: "u".to_owned(), ..Default::default() }, tick_hz: 30 },
            ToGateway::Joined { session: 1 },
            ToGateway::Left { session: 1 },
            ToGateway::Send { session: 4, message: server("one") },
            ToGateway::Broadcast { message: server("all") },
        ];
        for message in to_gateway {
            assert_eq!(ToGateway::decode(&message.encode()).unwrap(), message);
        }
    }

    #[test]
    fn what_is_not_a_message_is_refused() {
        assert!(ToRegion::decode(&[]).is_err(), "an empty message");
        assert!(ToGateway::decode(&[0xff, 0xff, 0xff]).is_err(), "garbage");
        let unknown_kind = wire::ToRegion {
            kind: Some(wire::to_region::Kind::Join(wire::Join { session: 1, kind: 9, ..Default::default() })),
        };
        assert!(ToRegion::decode(&unknown_kind.encode_to_vec()).is_err(), "an unknown session kind");
    }

    fn drain(messages: &mut UnboundedReceiver<ServerMessage>) -> Vec<String> {
        std::iter::from_fn(|| messages.try_recv().ok())
            .map(|m| match m.message {
                Some(server_message::Message::Goodbye(goodbye)) => goodbye.detail,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_broadcast_reaches_the_sessions_joined_when_it_was_sent() {
        let mut router = Router::default();
        let (alice, mut to_alice) = unbounded_channel();
        let (bob, mut to_bob) = unbounded_channel();
        router.open(1, alice);
        router.deliver(ToGateway::Joined { session: 1 });
        // Bob's connection is open, but the region has not taken him in yet.
        router.open(2, bob);
        router.deliver(ToGateway::Broadcast { message: server("before bob") });
        router.deliver(ToGateway::Send { session: 2, message: server("bob's snapshot") });
        router.deliver(ToGateway::Joined { session: 2 });
        router.deliver(ToGateway::Broadcast { message: server("after bob") });
        router.deliver(ToGateway::Left { session: 1 });
        router.deliver(ToGateway::Broadcast { message: server("after alice") });
        router.deliver(ToGateway::Send { session: 1, message: server("too late") });

        assert_eq!(drain(&mut to_alice), ["before bob", "after bob"]);
        assert_eq!(drain(&mut to_bob), ["bob's snapshot", "after bob", "after alice"]);
        assert!(to_alice.is_closed() || to_alice.try_recv().is_err(), "alice's route is closed");
        assert_eq!(router.len(), 1);
    }
}
