// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! An AI agent races through the generic MCP adapter: it learns from
//! `world_info` that the world takes `kart.autopilot/1` intents and what
//! they mean, and sends one with `send_intent` — nothing kart-specific in
//! the adapter.

use std::{net::TcpListener, thread, time::Duration};

use osage_agent::{Agent, mcp_handle};
use osage_credentials::{Level, TestIssuer};
use osage_sdk::{DELEGATE_AGENT, request_delegation};
use osage_world_kart::{KartWorld, TICK_HZ};
use serde_json::{Value, json};

async fn tool(agent: &Agent, name: &str, arguments: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    let response = mcp_handle(agent, &request).await.unwrap();
    assert!(response["result"]["isError"].is_null(), "{response}");
    response["result"]["structuredContent"].clone()
}

fn my_kart(seen: &Value) -> Value {
    seen["entities"].as_array().unwrap().iter().find(|e| e["mine"] == true).expect("the agent's kart").clone()
}

#[tokio::test]
async fn an_ai_agent_races_through_mcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let config = osage_server::Config {
        world_name: "osage-kart".to_owned(),
        tick_hz: TICK_HZ,
        trusted_issuers: vec![TestIssuer::insecure_dev().trusted()],
        ..Default::default()
    };
    thread::spawn(move || osage_server::serve(listener, config, KartWorld { grid: 2, laps: 3 }));

    let human =
        TestIssuer::insecure_dev().issue("osage-kart", "alice@demo", Level::A2, Duration::from_secs(600)).unwrap();
    let delegation = request_delegation(&url, &human, DELEGATE_AGENT).await.unwrap();
    let agent = Agent::join(&url, &delegation).await.unwrap();

    let info = tool(&agent, "world_info", json!({})).await;
    let types = info["component_types"].as_array().unwrap();
    let autopilot = types.iter().find(|t| t["name"] == "kart.autopilot/1").expect("described by the world");
    assert!(autopilot["description"].as_str().unwrap().contains("pace"), "{autopilot}");

    tokio::time::sleep(Duration::from_secs(4)).await; // the lights
    let waiting = my_kart(&tool(&agent, "observe", json!({})).await);
    assert_eq!(waiting["components"]["kart.state/1"]["speed"].as_f64().unwrap_or(0.0), 0.0, "it waits for the agent");
    assert_eq!(waiting["components"]["kart.racer/1"]["name"], info["me"]["per_world_id"]);

    let sent =
        tool(&agent, "send_intent", json!({ "type": "kart.autopilot/1", "value": { "pace": 0.9, "line": 0.2 } })).await;
    assert_eq!(sent["entity"], waiting["entity"]);
    tokio::time::sleep(Duration::from_secs(4)).await;
    let racing = my_kart(&tool(&agent, "observe", json!({})).await);
    assert!(racing["components"]["kart.state/1"]["speed"].as_f64().unwrap() > 15.0, "{racing}");
    assert_eq!(racing["components"]["kart.racer/1"]["lap"], 1, "past the start line");
}
