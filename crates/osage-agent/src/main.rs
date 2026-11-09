// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `osage-agent-mcp [--world ws://127.0.0.1:7878]` — MCP server over stdio; the
//! delegation is read from `OSAGE_DELEGATION`.
//! `osage-agent-mcp delegate --credential TOKEN [--world URL]` — prints a
//! delegation for the human holding the identity credential TOKEN.

use anyhow::{Context, Result, bail};
use osage_agent::{Agent, request_delegation, serve_stdio};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    let delegate = args.next_if_eq("delegate").is_some();
    let (mut world, mut credential) = ("ws://127.0.0.1:7878".to_owned(), None);
    while let Some(flag) = args.next() {
        let value = args.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--world" => world = value,
            "--credential" => credential = Some(value),
            _ => bail!("unknown argument {flag}"),
        }
    }
    if delegate {
        println!("{}", request_delegation(&world, &credential.context("delegate needs --credential")?).await?);
        return Ok(());
    }
    let delegation =
        std::env::var("OSAGE_DELEGATION").context("set OSAGE_DELEGATION (see `osage-agent-mcp delegate`)")?;
    let agent = Agent::join(&world, &delegation).await?;
    eprintln!("osage-agent-mcp: joined {world}");
    serve_stdio(agent).await
}
