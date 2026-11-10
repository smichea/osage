// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A minimal MCP server over stdio (newline-delimited JSON-RPC 2.0):
//! `initialize`, `ping`, `tools/list`, `tools/call`.

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::Agent;

/// MCP protocol revisions this server speaks, newest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

pub async fn serve_stdio(agent: Agent) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle(&agent, &request).await,
            Err(error) => Some(failure(Value::Null, -32700, &format!("parse error: {error}"))),
        };
        if let Some(response) = response {
            stdout.write_all(format!("{response}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// Answers one JSON-RPC message; notifications get no answer.
pub async fn handle(agent: &Agent, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let result = match request.get("method").and_then(Value::as_str).unwrap_or_default() {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "osage-agent-mcp", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "You are an agent inside an osage world, acting for a responsible human whose \
                    identifier is in your per_world_id. Call world_info first: it describes the world's component \
                    types, including the intents you can send. Then observe; `mine` marks what you control. \
                    send_intent acts through any intent the world describes; move walks an avatar.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => call(agent, &params).await,
        method => return Some(failure(id, -32601, &format!("unknown method {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn tools() -> Value {
    json!([
        {
            "name": "world_info",
            "description": "The world you are in (name, tick rate, where its source is) and who you are (per-world id, level, privileges).",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "observe",
            "description": "Every entity you can perceive, with its components as JSON (transform, content, controller…). `mine` marks your avatar.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "set_device",
            "description": "Set a function of a real device (e.g. brightness), only if its owner opened it to the world. The world checks the function's risk class and the device's bridge enforces its own safety limits; the answer says OK, REJECTED (with the reason), PENDING_APPROVAL or UNKNOWN.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": { "type": "integer", "description": "the device entity, from observe" },
                    "function": { "type": "string", "description": "a function listed in the device's osage.device/1" },
                    "value": { "type": "number" }
                },
                "required": ["entity", "function", "value"]
            }
        },
        {
            "name": "send_intent",
            "description": "Act in the world: send an intent of a type world_info describes (its description says what it does), with its value as JSON in the type's protobuf JSON form. It goes to the entity you control unless you name another one you own.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "description": "an intent type from world_info, e.g. kart.autopilot/1" },
                    "value": { "type": "object", "description": "the intent's fields, e.g. {\"pace\": 0.8, \"line\": 0}" },
                    "entity": { "type": "integer", "description": "optional: an entity you own; default: the one you control" }
                },
                "required": ["type", "value"]
            }
        },
        {
            "name": "move",
            "description": "Walk your avatar in a horizontal direction for some seconds (at most 5), then stop. X is east, Z is south, metres.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": { "type": "number", "description": "east component of the direction" },
                    "z": { "type": "number", "description": "south component of the direction" },
                    "seconds": { "type": "number", "description": "how long to walk, 0 to 5" }
                },
                "required": ["x", "z", "seconds"]
            }
        }
    ])
}

async fn call(agent: &Agent, params: &Value) -> Value {
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let number = |name: &str| arguments.get(name).and_then(Value::as_f64);
    let outcome = match params.get("name").and_then(Value::as_str).unwrap_or_default() {
        "world_info" => Ok(agent.world_info()),
        "observe" => Ok(agent.observe().await),
        "set_device" => match (
            arguments.get("entity").and_then(Value::as_u64),
            arguments.get("function").and_then(Value::as_str),
            number("value"),
        ) {
            (Some(entity), Some(function), Some(value)) => agent.set_device(entity, function, value).await,
            _ => Err(anyhow::anyhow!("set_device needs entity (integer), function (string) and value (number)")),
        },
        "send_intent" => match (arguments.get("type").and_then(Value::as_str), arguments.get("value")) {
            (Some(type_name), Some(value)) => {
                agent.send_intent(arguments.get("entity").and_then(Value::as_u64), type_name, value.clone()).await
            }
            _ => Err(anyhow::anyhow!("send_intent needs type (string) and value (object)")),
        },
        "move" => match (number("x"), number("z"), number("seconds")) {
            (Some(x), Some(z), Some(seconds)) => agent.walk(x as f32, z as f32, seconds).await,
            _ => Err(anyhow::anyhow!("move needs numbers x, z and seconds")),
        },
        other => Err(anyhow::anyhow!("unknown tool {other}")),
    };
    match outcome {
        Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }], "structuredContent": value }),
        Err(error) => json!({ "content": [{ "type": "text", "text": error.to_string() }], "isError": true }),
    }
}

fn failure(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
