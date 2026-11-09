// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP adapter for osage: an AI agent's harness launches it over stdio; it
//! joins a world with the role `agent` and the delegation of the agent's
//! responsible human (design/01 §3), and exposes the world as MCP tools.
//!
//! Components are decoded generically with the schemas the world sends in
//! its Welcome (D-019): the adapter knows no component type in advance.

mod mcp;

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use osage_sdk::{
    DELEGATE_AGENT, SessionSender, World, components, connect, hello, intent, types,
    v0::{Role, Tier, Welcome},
    with_delegation,
};
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};
use serde_json::{Value, json};
use tokio::sync::Mutex;

pub use mcp::{handle as mcp_handle, serve_stdio};

/// Longest single `move`, seconds.
const MAX_MOVE_SECONDS: f64 = 5.0;
const INTENT_PERIOD: Duration = Duration::from_millis(50);

/// A live agent session and the tools it offers.
pub struct Agent {
    welcome: Welcome,
    world: Arc<Mutex<World>>,
    sender: Mutex<SessionSender>,
    messages: HashMap<String, MessageDescriptor>,
    sequence: std::sync::atomic::AtomicU64,
}

impl Agent {
    /// Joins `url` as an agent with `delegation`.
    pub async fn join(url: &str, delegation: &str) -> Result<Self> {
        let hello = with_delegation(
            hello("osage-agent-mcp", env!("CARGO_PKG_VERSION"), Role::Agent, Tier::T0Headless),
            delegation,
        );
        let (welcome, sender, mut receiver) = connect(url, hello).await?.split();

        let schemas = welcome.schemas.clone().context("the world sent no schemas")?;
        let pool = DescriptorPool::from_file_descriptor_set(
            schemas.descriptors.context("schemas by URL are not supported yet")?,
        )?;
        let messages = schemas
            .types
            .iter()
            .filter_map(|t| Some((t.name.clone(), pool.get_message_by_name(&t.message_type)?)))
            .collect();

        let world = Arc::new(Mutex::new(World::new(&welcome)));
        let replica = world.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = receiver.next_message().await {
                replica.lock().await.apply(&message);
            }
        });
        Ok(Self { welcome, world, sender: Mutex::new(sender), messages, sequence: Default::default() })
    }

    fn me(&self) -> &str {
        self.welcome.identity.as_ref().map_or("", |i| i.per_world_id.as_str())
    }

    /// `world_info`: the world and who this agent is.
    pub fn world_info(&self) -> Value {
        let world = self.welcome.world.clone().unwrap_or_default();
        let identity = self.welcome.identity.clone().unwrap_or_default();
        json!({
            "world": world.name,
            "tick_rate_hz": world.tick_rate_hz,
            "source": world.source_offer.map(|s| json!({ "url": s.url, "version": s.version })),
            "me": {
                "per_world_id": identity.per_world_id,
                "level": identity.level().as_str_name(),
                "privileges": identity.privileges,
            },
            "component_types": self.messages.keys().collect::<Vec<_>>(),
        })
    }

    /// `observe`: every entity in scope, its components decoded as JSON.
    pub async fn observe(&self) -> Value {
        let world = self.world.lock().await;
        let mine = world.controlled_by(self.me());
        let entities: Vec<Value> = world
            .entities()
            .map(|entity| {
                let components: serde_json::Map<String, Value> =
                    world.components(entity).map(|(name, bytes)| (name.to_owned(), self.decode(name, bytes))).collect();
                json!({ "entity": entity, "mine": Some(entity) == mine, "components": components })
            })
            .collect();
        json!({ "tick": world.tick(), "entities": entities })
    }

    /// `move`: walks this agent's avatar in direction (x, z) for `seconds`.
    pub async fn walk(&self, x: f32, z: f32, seconds: f64) -> Result<Value> {
        let (avatar, move_type) = {
            let world = self.world.lock().await;
            let avatar = world.controlled_by(self.me()).context("this agent has no avatar in scope yet")?;
            (avatar, world.types().id(types::MOVE_INTENT).context("this world has no movement intents")?)
        };
        let seconds = seconds.clamp(0.0, MAX_MOVE_SECONDS);
        let steps = (seconds / INTENT_PERIOD.as_secs_f64()).ceil() as u32;
        let direction = |x, z| components::MoveIntent { direction: Some(osage_sdk::math::Vec3 { x, y: 0.0, z }) };
        for step in 0..=steps {
            let (dx, dz) = if step == steps { (0.0, 0.0) } else { (x, z) };
            let sequence = self.sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            self.sender.lock().await.send(intent(sequence, avatar, move_type, &direction(dx, dz))).await?;
            tokio::time::sleep(INTENT_PERIOD).await;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let world = self.world.lock().await;
        let position = world.get::<components::Transform>(avatar, types::TRANSFORM).and_then(|t| t.translation);
        Ok(json!({ "entity": avatar, "position": position.map(|p| json!({ "x": p.x, "y": p.y, "z": p.z })) }))
    }

    fn decode(&self, type_name: &str, bytes: &[u8]) -> Value {
        let Some(descriptor) = self.messages.get(type_name) else { return json!({ "undecoded_bytes": bytes.len() }) };
        match DynamicMessage::decode(descriptor.clone(), bytes) {
            Ok(message) => serde_json::to_value(&message).unwrap_or(Value::Null),
            Err(error) => json!({ "decode_error": error.to_string() }),
        }
    }
}

/// Connects as the human holding `credential` and asks the world for an agent
/// delegation (the `osage.delegate_agent` action).
pub async fn request_delegation(url: &str, credential: &str) -> Result<String> {
    osage_sdk::request_delegation(url, credential, DELEGATE_AGENT).await
}
