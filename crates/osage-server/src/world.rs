// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The World Module Interface ([`osage_world_module_api`], permissive), and a
//! harness that runs a world module in a region, for its tests.

use bevy::prelude::*;
use osage_proto::{components, v0::Role};
pub use osage_world_module_api::*;

/// Runs a world in a region without network or real time — every update is
/// one tick of simulated time — to test its rules.
pub struct Harness {
    pub app: App,
    inbox: crossbeam_channel::Sender<crate::RegionInput>,
    schema: Schema,
    sessions: u64,
}

impl Harness {
    pub fn new(world: impl WorldModule, tick_hz: u32) -> Self {
        let (inbox, rx) = crossbeam_channel::unbounded();
        let schema = Schema::new(&world);
        let mut app = crate::region::app(rx, tick_hz, schema.clone(), world);
        let tick = std::time::Duration::from_secs_f64(1.0 / f64::from(tick_hz.max(1)));
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(tick));
        Self { app, inbox, schema, sessions: 0 }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// A Welcome carrying this world's schema bundle, for client-side code.
    pub fn welcome(&self) -> osage_proto::v0::Welcome {
        osage_proto::v0::Welcome { schemas: Some(self.schema.bundle()), ..default() }
    }

    /// A session joins; returns it and what the region sends it.
    pub fn join(
        &mut self,
        kind: SessionKind,
        role: Role,
        per_world_id: &str,
    ) -> (u64, tokio::sync::mpsc::UnboundedReceiver<osage_proto::v0::ServerMessage>) {
        self.sessions += 1;
        let (out, messages) = tokio::sync::mpsc::unbounded_channel();
        let controller_kind = match kind {
            SessionKind::Agent => components::ControllerKind::Agent,
            _ => components::ControllerKind::Human,
        };
        let controller =
            components::Controller { kind: controller_kind.into(), per_world_id: per_world_id.to_owned(), ..default() };
        let member = crate::region::Member { kind, role, controller, owner: String::new() };
        let _ = self.inbox.send(crate::RegionInput::Join { session: self.sessions, member, out });
        (self.sessions, messages)
    }

    pub fn leave(&mut self, session: u64) {
        let _ = self.inbox.send(crate::RegionInput::Leave { session });
    }

    pub fn send(&mut self, session: u64, message: osage_proto::v0::client_message::Message) {
        let _ = self.inbox.send(crate::RegionInput::Message { session, message });
    }

    /// Runs `ticks` ticks.
    pub fn run(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.app.update();
        }
    }
}
