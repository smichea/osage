// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! An agent joins a real world through the MCP adapter and uses its tools.

use std::{net::TcpListener, thread, time::Duration};

use osage_agent::{Agent, mcp_handle, request_delegation};
use osage_credentials::{Level, TestIssuer};
use serde_json::{Value, json};

fn start_world() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let config = osage_server::Config {
        trusted_issuers: vec![TestIssuer::insecure_dev().trusted()],
        ..osage_server::Config::default()
    };
    thread::spawn(move || osage_server::run(listener, config));
    url
}

async fn call(agent: &Agent, id: u64, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    mcp_handle(agent, &request).await.expect("a request gets an answer")["result"].clone()
}

#[tokio::test]
async fn an_agent_observes_the_world_and_walks_through_mcp() {
    let url = start_world();
    let credential =
        TestIssuer::insecure_dev().issue("osage-demo", "alice@demo", Level::A2, Duration::from_secs(600)).unwrap();
    let delegation = request_delegation(&url, &credential).await.unwrap();
    let agent = Agent::join(&url, &delegation).await.unwrap();

    let init = call(&agent, 1, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {} })).await;
    assert_eq!(init["protocolVersion"], "2025-06-18");
    assert!(mcp_handle(&agent, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());

    let tools = call(&agent, 2, "tools/list", json!({})).await;
    let names: Vec<_> = tools["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["world_info", "observe", "set_device", "send_intent", "move"]);

    let info = call(&agent, 3, "tools/call", json!({ "name": "world_info", "arguments": {} })).await;
    let me = &info["structuredContent"]["me"];
    assert!(me["per_world_id"].as_str().unwrap().ends_with(".alice@demo"));
    assert_eq!(me["level"], "ASSURANCE_LEVEL_A2");
    let types = info["structuredContent"]["component_types"].as_array().unwrap();
    let walking = types.iter().find(|t| t["name"] == "osage.move_intent/1").expect("the intent is described");
    assert!(walking["description"].as_str().unwrap().starts_with("Intent"), "{walking}");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let seen = call(&agent, 4, "tools/call", json!({ "name": "observe", "arguments": {} })).await;
    let entities = seen["structuredContent"]["entities"].as_array().unwrap().clone();
    let mine = entities.iter().find(|e| e["mine"] == true).expect("the agent sees its avatar");
    // Decoded generically from the world's schema bundle (D-019), in canonical JSON.
    assert_eq!(mine["components"]["osage.controller/1"]["kind"], "CONTROLLER_KIND_AGENT");
    assert_eq!(mine["components"]["osage.controller/1"]["responsiblePerWorldId"], "alice@demo");
    assert!(entities.iter().any(|e| e["components"]["osage.content_ref/1"]["uri"] == "osage:builtin/cube"));
    let x_before = mine["components"]["osage.transform/1"]["translation"]["x"].as_f64().unwrap_or(0.0);

    let walked =
        call(&agent, 5, "tools/call", json!({ "name": "move", "arguments": { "x": 1, "z": 0, "seconds": 1 } })).await;
    assert!(walked.get("isError").is_none(), "{walked}");
    let x_after = walked["structuredContent"]["position"]["x"].as_f64().unwrap();
    assert!(x_after > x_before + 2.0, "walked east: {x_before} → {x_after}");

    let unknown = call(&agent, 6, "tools/call", json!({ "name": "fly", "arguments": {} })).await;
    assert_eq!(unknown["isError"], true);
}
