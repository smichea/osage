// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Who a session is: authentication of identity credentials and agent
//! delegations (D-013, design/01), privileges by level (D-006), and the
//! registry that refuses duplicate sessions and enforces agent quotas.

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use osage_credentials::{DelegatedRole, Level, verify_identity};
use osage_proto::{
    components::{Controller, ControllerKind},
    v0::{AssuranceLevel, GoodbyeReason, Hello, Role, credentials::Credential},
};

use crate::{Config, gateway::Gateway};

/// Why a session is refused, sent in its Goodbye.
pub struct Refusal(pub GoodbyeReason, pub String);

/// What kind of participant a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Human,
    Agent,
    Bridge,
}

#[derive(Clone, Debug)]
pub struct Identity {
    pub kind: SessionKind,
    pub per_world_id: String,
    pub level: AssuranceLevel,
    pub issuer: String,
    /// For an agent: its responsible human; for a bridge: the devices' owner.
    pub responsible: Option<String>,
}

impl Identity {
    pub fn is_agent(&self) -> bool {
        self.kind == SessionKind::Agent
    }

    /// Anonymous guests get generated identifiers and are never deduplicated.
    fn is_guest(&self) -> bool {
        self.level == AssuranceLevel::A0
    }

    pub fn controller(&self) -> Controller {
        Controller {
            kind: if self.is_agent() { ControllerKind::Agent } else { ControllerKind::Human }.into(),
            per_world_id: self.per_world_id.clone(),
            responsible_per_world_id: self.responsible.clone().unwrap_or_default(),
        }
    }
}

/// Default privileges by level (D-006); worlds will be able to change them.
pub fn privileges(level: AssuranceLevel) -> Vec<String> {
    let mut granted = vec!["visit"];
    if level >= AssuranceLevel::A1 {
        granted.push("persist");
    }
    if level >= AssuranceLevel::A2 {
        granted.extend(["trade", "own", "ranked", "run_agents"]);
    }
    if level >= AssuranceLevel::A3 {
        granted.extend(["withdraw", "moderate", "vouch"]);
    }
    granted.into_iter().map(str::to_owned).collect()
}

fn assurance(level: Level) -> AssuranceLevel {
    match level {
        Level::A1 => AssuranceLevel::A1,
        Level::A2 => AssuranceLevel::A2,
        Level::A3 => AssuranceLevel::A3,
    }
}

pub fn credential_level(level: AssuranceLevel) -> Option<Level> {
    match level {
        AssuranceLevel::A1 => Some(Level::A1),
        AssuranceLevel::A2 => Some(Level::A2),
        AssuranceLevel::A3 => Some(Level::A3),
        _ => None,
    }
}

impl Gateway {
    pub(crate) fn authenticate(&self, session: u64, hello: &Hello) -> Result<Identity, Refusal> {
        let unauthenticated = |detail: &str| Refusal(GoodbyeReason::Unauthenticated, detail.to_owned());
        let credential = hello.credentials.as_ref().and_then(|c| c.credential.as_ref());
        match (hello.role(), credential) {
            (Role::Agent, Some(Credential::Delegation(token))) => self.delegated(token, DelegatedRole::Agent),
            (Role::Agent, _) => Err(unauthenticated("agents join with a delegation from their responsible human")),
            (Role::Bridge, Some(Credential::Bridge(token))) => self.delegated(token, DelegatedRole::Bridge),
            (Role::Bridge, _) => Err(unauthenticated("bridges join with a delegation from the devices' owner")),
            (_, None | Some(Credential::Anonymous(_))) => Ok(Identity {
                kind: SessionKind::Human,
                per_world_id: format!("guest-{session}"),
                level: AssuranceLevel::A0,
                issuer: String::new(),
                responsible: None,
            }),
            (_, Some(Credential::VerifiablePresentation(token))) => {
                let token = std::str::from_utf8(token).map_err(|_| unauthenticated("credential is not UTF-8"))?;
                let verified = verify_identity(token, &self.config.world_name, &self.config.trusted_issuers)
                    .map_err(|error| unauthenticated(&format!("credential refused: {error}")))?;
                Ok(Identity {
                    kind: SessionKind::Human,
                    per_world_id: verified.per_world_id,
                    level: assurance(verified.level),
                    issuer: verified.issuer,
                    responsible: None,
                })
            }
            _ => Err(unauthenticated("this credential does not fit this role")),
        }
    }

    fn delegated(&self, token: &[u8], role: DelegatedRole) -> Result<Identity, Refusal> {
        let unauthenticated = |detail: String| Refusal(GoodbyeReason::Unauthenticated, detail);
        let token = std::str::from_utf8(token).map_err(|_| unauthenticated("delegation is not UTF-8".to_owned()))?;
        let claims = self.delegations.verify(token).map_err(|e| unauthenticated(format!("delegation refused: {e}")))?;
        if claims.role != role {
            return Err(unauthenticated(format!("this delegation is for a {:?}", claims.role)));
        }
        Ok(Identity {
            kind: if role == DelegatedRole::Agent { SessionKind::Agent } else { SessionKind::Bridge },
            per_world_id: claims.sub,
            level: assurance(claims.level),
            issuer: self.config.world_name.clone(),
            responsible: Some(claims.responsible),
        })
    }
}

/// Identities currently connected, and agents per responsible human.
#[derive(Default)]
pub struct Registry(Mutex<Connected>);

#[derive(Default)]
struct Connected {
    identities: HashSet<String>,
    agents_by_human: HashMap<String, usize>,
}

/// An admitted session; leaving the registry happens when it is dropped.
pub struct Admission<'a> {
    registry: &'a Registry,
    identity: Identity,
}

impl Admission<'_> {
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

impl Registry {
    pub(crate) fn admit<'a>(&'a self, config: &Config, identity: Identity) -> Result<Admission<'a>, Refusal> {
        let sanctioned = |id: &String| config.sanctioned.contains(id);
        if sanctioned(&identity.per_world_id) || identity.responsible.as_ref().is_some_and(sanctioned) {
            return Err(Refusal(GoodbyeReason::Sanctioned, "sanctioned in this world".to_owned()));
        }
        let mut connected = self.0.lock().expect("registry lock");
        if !identity.is_guest() && connected.identities.contains(&identity.per_world_id) {
            return Err(Refusal(GoodbyeReason::DuplicateSession, "this identity is already connected".to_owned()));
        }
        if let (true, Some(human)) = (identity.is_agent(), &identity.responsible) {
            let running = connected.agents_by_human.get(human).copied().unwrap_or(0);
            if running >= config.max_agents_per_human {
                return Err(Refusal(
                    GoodbyeReason::Quota,
                    format!("{human} already runs {running} agents in this world"),
                ));
            }
            *connected.agents_by_human.entry(human.clone()).or_default() += 1;
        }
        if !identity.is_guest() {
            connected.identities.insert(identity.per_world_id.clone());
        }
        Ok(Admission { registry: self, identity })
    }
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut connected = self.registry.0.lock().expect("registry lock");
        connected.identities.remove(&self.identity.per_world_id);
        if let (true, Some(human)) = (self.identity.is_agent(), &self.identity.responsible)
            && let Some(count) = connected.agents_by_human.get_mut(human)
        {
            *count = count.saturating_sub(1);
        }
    }
}
