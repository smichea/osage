// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The World Module Interface ([`osage_world_module_api`], permissive), and a
//! harness that runs a world module in a region, for its tests.

use bevy::prelude::*;
use osage_proto::{components, v0::Role};
pub use osage_world_module_api::*;

/// Runs a world in a region without network or real time — every update is
/// one tick of simulated time — to test its rules. It stands where the
/// gateway does, on the other end of the region's link.
pub struct Harness {
    pub app: App,
    link: osage_region_link::GatewayEnd,
    router: osage_region_link::Router,
    schema: Schema,
    sessions: u64,
}

impl Harness {
    pub fn new(world: impl WorldModule, tick_hz: u32) -> Self {
        let (link, region) = osage_region_link::in_process();
        let schema = Schema::new(&world);
        let mut app = crate::region::app(region, tick_hz, schema.clone(), world);
        let tick = std::time::Duration::from_secs_f64(1.0 / f64::from(tick_hz.max(1)));
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(tick));
        Self { app, link, router: osage_region_link::Router::default(), schema, sessions: 0 }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// A Welcome carrying this world's schema bundle, for client-side code.
    pub fn welcome(&self) -> osage_proto::v0::Welcome {
        osage_proto::v0::Welcome { schemas: Some(self.schema.bundle()), ..default() }
    }

    /// A session joins; returns it and what the region sends it, which
    /// arrives as ticks run.
    pub fn join(
        &mut self,
        kind: SessionKind,
        role: Role,
        per_world_id: &str,
    ) -> (u64, tokio::sync::mpsc::UnboundedReceiver<osage_proto::v0::ServerMessage>) {
        self.sessions += 1;
        let (out, messages) = tokio::sync::mpsc::unbounded_channel();
        let (controller_kind, kind) = match kind {
            SessionKind::Human => (components::ControllerKind::Human, osage_region_link::SessionKind::Human),
            SessionKind::Agent => (components::ControllerKind::Agent, osage_region_link::SessionKind::Agent),
            SessionKind::Bridge => (components::ControllerKind::Human, osage_region_link::SessionKind::Bridge),
        };
        let controller =
            components::Controller { kind: controller_kind.into(), per_world_id: per_world_id.to_owned(), ..default() };
        let member = osage_region_link::Member { kind, role, controller, owner: String::new() };
        self.router.open(self.sessions, out);
        let _ = self.link.to_region.send(osage_region_link::ToRegion::Join { session: self.sessions, member });
        (self.sessions, messages)
    }

    pub fn leave(&mut self, session: u64) {
        let _ = self.link.to_region.send(osage_region_link::ToRegion::Leave { session });
    }

    pub fn send(&mut self, session: u64, message: osage_proto::v0::client_message::Message) {
        let _ = self.link.to_region.send(osage_region_link::ToRegion::Client { session, message });
    }

    /// Runs `ticks` ticks.
    pub fn run(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.app.update();
            while let Ok(message) = self.link.from_region.try_recv() {
                self.router.deliver(message);
            }
        }
    }
}
