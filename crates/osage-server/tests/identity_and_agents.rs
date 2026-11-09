// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Identity credentials (D-013), refusals, and agents joining with the
//! delegation of their responsible human (design/01 §3).

use std::{net::TcpListener, thread, time::Duration};

use osage_credentials::{Level, TestIssuer};
use osage_sdk::{
    World, components, connect, hello, types,
    v0::{ActionRequest, ActionStatus, AssuranceLevel, Role, Tier, client_message, server_message},
    with_delegation, with_identity,
};
use osage_server::{Config, DELEGATE_AGENT};
use tokio::time::timeout;

const WORLD: &str = "osage-demo";
const HOUR: Duration = Duration::from_secs(3600);

fn start(config: Config) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    thread::spawn(move || osage_server::run(listener, config));
    url
}

fn trusting_dev_issuer() -> Config {
    Config { trusted_issuers: vec![TestIssuer::insecure_dev().trusted()], ..Config::default() }
}

fn credential(who: &str, level: Level) -> String {
    TestIssuer::insecure_dev().issue(WORLD, who, level, HOUR).unwrap()
}

fn player() -> osage_sdk::v0::Hello {
    hello("test", "0", Role::Player, Tier::T2Standard)
}

#[tokio::test]
async fn a_verified_identity_is_welcomed_with_its_level_and_privileges() {
    let url = start(trusting_dev_issuer());
    let session = connect(&url, with_identity(player(), &credential("alice@demo", Level::A2))).await.unwrap();
    let identity = session.welcome().identity.clone().unwrap();
    assert_eq!(identity.per_world_id, "alice@demo");
    assert_eq!(identity.level(), AssuranceLevel::A2);
    assert_eq!(identity.issuer, "urn:osage:insecure-dev-issuer");
    assert!(identity.privileges.contains(&"run_agents".to_owned()));
    assert!(!identity.privileges.contains(&"moderate".to_owned()), "moderation needs A3");
}

#[tokio::test]
async fn bad_credentials_duplicates_and_sanctions_are_refused() {
    let mut config = trusting_dev_issuer();
    config.sanctioned.insert("mallory@demo".to_owned());
    let url = start(config);

    let other_world = TestIssuer::insecure_dev().issue("another-world", "bob@demo", Level::A2, HOUR).unwrap();
    let error = connect(&url, with_identity(player(), &other_world)).await.err().unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "{error}");

    let stranger = TestIssuer::from_seed("urn:example:stranger", "s-1", [9; 32]).unwrap();
    let untrusted = stranger.issue(WORLD, "bob@demo", Level::A3, HOUR).unwrap();
    let error = connect(&url, with_identity(player(), &untrusted)).await.err().unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "{error}");

    let _bob = connect(&url, with_identity(player(), &credential("bob@demo", Level::A1))).await.unwrap();
    let error = connect(&url, with_identity(player(), &credential("bob@demo", Level::A1))).await.err().unwrap();
    assert!(error.to_string().contains("DuplicateSession"), "{error}");

    let error = connect(&url, with_identity(player(), &credential("mallory@demo", Level::A3))).await.err().unwrap();
    assert!(error.to_string().contains("Sanctioned"), "{error}");
}

/// Asks the world for a delegation and returns the action's status and token.
async fn ask_delegation(session: &mut osage_sdk::Session) -> (ActionStatus, String) {
    let request = ActionRequest { request_id: 1, action: DELEGATE_AGENT.to_owned(), ..Default::default() };
    session.send(client_message::Message::ActionRequest(request)).await.unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            let message = session.next_message().await.unwrap().unwrap();
            if let Some(server_message::Message::ActionResult(result)) = message.message {
                return (result.status(), String::from_utf8(result.result).unwrap());
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn an_agent_joins_with_its_humans_delegation_within_the_quota() {
    let url = start(Config { max_agents_per_human: 1, ..trusting_dev_issuer() });

    let mut guest = connect(&url, player()).await.unwrap();
    assert_eq!(ask_delegation(&mut guest).await.0, ActionStatus::Rejected, "guests cannot run agents");

    let mut alice = connect(&url, with_identity(player(), &credential("alice@demo", Level::A2))).await.unwrap();
    let (status, delegation) = ask_delegation(&mut alice).await;
    assert_eq!(status, ActionStatus::Ok);

    let agent =
        connect(&url, with_delegation(hello("agent", "0", Role::Player, Tier::T0Headless), &delegation)).await.unwrap();
    let identity = agent.welcome().identity.clone().unwrap();
    assert!(identity.per_world_id.ends_with(".alice@demo"), "{}", identity.per_world_id);
    assert_eq!(identity.level(), AssuranceLevel::A2);

    let (welcome, _sender, mut receiver) = agent.split();
    let mut world = World::new(&welcome);
    let _ = timeout(Duration::from_millis(300), async {
        while let Some(Ok(message)) = receiver.next_message().await {
            world.apply(&message);
        }
    })
    .await;
    let avatar = world.controlled_by(&identity.per_world_id).expect("the agent has an avatar");
    let controller: components::Controller = world.get(avatar, types::CONTROLLER).unwrap();
    assert_eq!(controller.kind(), components::ControllerKind::Agent);
    assert_eq!(controller.responsible_per_world_id, "alice@demo");

    let (_, second) = ask_delegation(&mut alice).await;
    let error = connect(&url, with_delegation(hello("agent", "0", Role::Agent, Tier::T0Headless), &second))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("Quota"), "one agent per human in this world: {error}");

    let error = connect(&url, hello("agent", "0", Role::Agent, Tier::T0Headless)).await.err().unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "agents need a delegation: {error}");
}
