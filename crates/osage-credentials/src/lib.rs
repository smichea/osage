// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! osage v0 credentials (spec/README.md, "Credentials").
//!
//! - **Identity:** a W3C VC 2.0 secured as a compact JWS (ES256) by an issuer
//!   the world trusts; its subject is the per-world identifier.
//! - **Delegation:** a token the world issues to a human for one agent.
//!
//! v0 credentials are short-lived bearer tokens bound to one world; holder
//! binding comes with OpenID4VP.

mod jws;

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use jws::Header;
use p256::ecdsa::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Longest accepted lifetime of an identity credential.
pub const MAX_IDENTITY_LIFETIME: Duration = Duration::from_secs(3600);

/// Clock difference tolerated when checking expiry.
const LEEWAY_SECS: u64 = 60;

const VC_CONTEXT: &str = "https://www.w3.org/ns/credentials/v2";
const ASSURANCE_TYPE: &str = "OsageAssuranceCredential";

/// osage assurance levels a credential can attest (A0 is "no credential").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    A1,
    A2,
    A3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subject {
    /// The per-world identifier: a pairwise pseudonym, never a civil identity.
    pub id: String,
    #[serde(rename = "assuranceLevel")]
    pub assurance_level: Level,
}

/// Claims of an identity credential: a VC 2.0 with the JWT claims it is
/// secured with.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityClaims {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    #[serde(rename = "type")]
    pub types: Vec<String>,
    pub iss: String,
    /// The world this credential is for.
    pub aud: String,
    pub sub: String,
    pub iat: u64,
    pub exp: u64,
    #[serde(rename = "credentialSubject")]
    pub credential_subject: Subject,
}

/// An issuer a world trusts: its id and P-256 public key (JWK coordinates,
/// base64url).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedIssuer {
    pub issuer: String,
    pub kid: String,
    pub x: String,
    pub y: String,
}

impl TrustedIssuer {
    fn verifying_key(&self) -> Result<VerifyingKey> {
        let point = [vec![4], jws::decode(&self.x)?, jws::decode(&self.y)?].concat();
        VerifyingKey::from_sec1_bytes(&point).context("issuer key")
    }
}

/// What a world learns from a valid identity credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub per_world_id: String,
    pub level: Level,
}

/// Verifies an identity credential for `world` against the issuers it trusts.
pub fn verify_identity(token: &str, world: &str, trusted: &[TrustedIssuer]) -> Result<VerifiedIdentity> {
    let token = jws::parse(token).context("not a JWS")?;
    let kid = token.header.kid.as_deref().context("no key id")?;
    let issuer = trusted.iter().find(|i| i.kid == kid).context("untrusted issuer key")?;
    let claims: IdentityClaims = token.verify_es256(&issuer.verifying_key()?)?;

    ensure!(claims.iss == issuer.issuer, "issuer mismatch");
    ensure!(claims.aud == world, "credential for another world");
    ensure!(unix_now() <= claims.exp + LEEWAY_SECS, "expired");
    ensure!(claims.types.iter().any(|t| t == ASSURANCE_TYPE), "not an osage assurance credential");
    ensure!(claims.sub == claims.credential_subject.id, "subject mismatch");
    ensure!(claims.exp.saturating_sub(claims.iat) <= MAX_IDENTITY_LIFETIME.as_secs(), "lifetime too long");
    Ok(VerifiedIdentity {
        issuer: claims.iss,
        per_world_id: claims.sub,
        level: claims.credential_subject.assurance_level,
    })
}

/// Claims of a delegation the world issued to a human for one agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DelegationClaims {
    pub iss: String,
    pub aud: String,
    /// The holder's per-world identifier (the agent or the bridge).
    pub sub: String,
    /// The per-world identifier of the responsible human.
    pub responsible: String,
    /// The responsible human's assurance level when the delegation was issued.
    pub level: Level,
    /// What the delegation is for.
    pub role: DelegatedRole,
    pub iat: u64,
    pub exp: u64,
}

/// What a delegation lets its holder be in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DelegatedRole {
    Agent,
    Bridge,
}

/// Issues and verifies delegations with the world's secret (HS256).
pub struct DelegationKey {
    world: String,
    secret: Vec<u8>,
}

impl DelegationKey {
    pub fn new(world: &str, secret: &[u8]) -> Self {
        Self { world: world.to_owned(), secret: secret.to_vec() }
    }

    pub fn issue(
        &self,
        role: DelegatedRole,
        holder: &str,
        responsible: &str,
        level: Level,
        lifetime: Duration,
    ) -> Result<String> {
        let iat = unix_now();
        let claims = DelegationClaims {
            iss: self.world.clone(),
            aud: self.world.clone(),
            sub: holder.to_owned(),
            responsible: responsible.to_owned(),
            level,
            role,
            iat,
            exp: iat + lifetime.as_secs(),
        };
        let header = Header { typ: Some("JWT".to_owned()), ..Header::new("HS256") };
        jws::sign_hs256(&header, &claims, &self.secret)
    }

    pub fn verify(&self, token: &str) -> Result<DelegationClaims> {
        let claims: DelegationClaims = jws::parse(token).context("not a JWS")?.verify_hs256(&self.secret)?;
        ensure!(claims.iss == self.world && claims.aud == self.world, "delegation for another world");
        ensure!(unix_now() <= claims.exp + LEEWAY_SECS, "expired");
        Ok(claims)
    }
}

/// A local issuer for development and tests. **Its key is public — never
/// trust it in a real world.**
pub struct TestIssuer {
    issuer: String,
    kid: String,
    key: SigningKey,
    trusted: TrustedIssuer,
}

impl TestIssuer {
    /// The insecure development issuer: a fixed, published key.
    pub fn insecure_dev() -> Self {
        Self::from_seed("urn:osage:insecure-dev-issuer", "insecure-dev-1", [7; 32]).expect("valid dev key")
    }

    pub fn from_seed(issuer: &str, kid: &str, seed: [u8; 32]) -> Result<Self> {
        use p256::elliptic_curve::sec1::ToSec1Point;

        let key = SigningKey::from_slice(&seed).context("seed is not a P-256 scalar")?;
        let point = key.verifying_key().as_affine().to_sec1_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else { bail!("compressed point") };
        let trusted =
            TrustedIssuer { issuer: issuer.to_owned(), kid: kid.to_owned(), x: jws::encode(x), y: jws::encode(y) };
        Ok(Self { issuer: issuer.to_owned(), kid: kid.to_owned(), key, trusted })
    }

    pub fn trusted(&self) -> TrustedIssuer {
        self.trusted.clone()
    }

    /// Issues an identity credential for `world`.
    pub fn issue(&self, world: &str, per_world_id: &str, level: Level, lifetime: Duration) -> Result<String> {
        let iat = unix_now();
        let claims = IdentityClaims {
            context: vec![VC_CONTEXT.to_owned()],
            types: vec!["VerifiableCredential".to_owned(), ASSURANCE_TYPE.to_owned()],
            iss: self.issuer.clone(),
            aud: world.to_owned(),
            sub: per_world_id.to_owned(),
            iat,
            exp: iat + lifetime.as_secs(),
            credential_subject: Subject { id: per_world_id.to_owned(), assurance_level: level },
        };
        let header = Header { typ: Some("vc+jwt".to_owned()), kid: Some(self.kid.clone()), ..Header::new("ES256") };
        jws::sign_es256(&header, &claims, &self.key)
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD: &str = "osage-demo";
    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn a_dev_credential_verifies_for_its_world_only() {
        let issuer = TestIssuer::insecure_dev();
        let token = issuer.issue(WORLD, "alice@demo", Level::A2, HOUR).unwrap();
        let identity = verify_identity(&token, WORLD, &[issuer.trusted()]).unwrap();
        assert_eq!(identity.per_world_id, "alice@demo");
        assert_eq!(identity.level, Level::A2);
        assert!(verify_identity(&token, "another-world", &[issuer.trusted()]).is_err());
    }

    #[test]
    fn untrusted_issuers_and_long_lifetimes_are_refused() {
        let dev = TestIssuer::insecure_dev();
        let other = TestIssuer::from_seed("urn:example:other", "other-1", [9; 32]).unwrap();
        let token = other.issue(WORLD, "bob", Level::A3, HOUR).unwrap();
        assert!(verify_identity(&token, WORLD, &[dev.trusted()]).is_err(), "unknown key");

        let forged = TrustedIssuer { kid: "other-1".to_owned(), ..dev.trusted() };
        assert!(verify_identity(&token, WORLD, &[forged]).is_err(), "kid with another key");

        let long = dev.issue(WORLD, "carol", Level::A1, HOUR * 2).unwrap();
        assert!(verify_identity(&long, WORLD, &[dev.trusted()]).is_err());
    }

    #[test]
    fn delegations_are_bound_to_their_world() {
        let key = DelegationKey::new(WORLD, b"secret of the demo world");
        let token = key.issue(DelegatedRole::Agent, "agent-1", "alice@demo", Level::A2, HOUR).unwrap();
        let claims = key.verify(&token).unwrap();
        assert_eq!((claims.sub.as_str(), claims.responsible.as_str()), ("agent-1", "alice@demo"));
        assert_eq!(claims.role, DelegatedRole::Agent);
        assert!(DelegationKey::new("another-world", b"secret of the demo world").verify(&token).is_err());
        assert!(DelegationKey::new(WORLD, b"another secret").verify(&token).is_err());
    }

    #[test]
    fn expired_credentials_are_refused() {
        let issuer = TestIssuer::insecure_dev();
        let mut claims = jws::parse(&issuer.issue(WORLD, "dave", Level::A1, HOUR).unwrap())
            .unwrap()
            .verify_es256::<IdentityClaims>(&issuer.trusted().verifying_key().unwrap())
            .unwrap();
        claims.iat -= 2 * HOUR.as_secs();
        claims.exp -= 2 * HOUR.as_secs();
        let header = Header { kid: Some("insecure-dev-1".to_owned()), ..Header::new("ES256") };
        let expired = jws::sign_es256(&header, &claims, &issuer.key).unwrap();
        assert!(verify_identity(&expired, WORLD, &[issuer.trusted()]).unwrap_err().to_string().contains("expired"));
    }
}
