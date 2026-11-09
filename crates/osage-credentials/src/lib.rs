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

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode};
use serde::{Deserialize, Serialize};

/// Longest accepted lifetime of an identity credential.
pub const MAX_IDENTITY_LIFETIME: Duration = Duration::from_secs(3600);

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

/// What a world learns from a valid identity credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub per_world_id: String,
    pub level: Level,
}

/// Verifies an identity credential for `world` against the issuers it trusts.
pub fn verify_identity(token: &str, world: &str, trusted: &[TrustedIssuer]) -> Result<VerifiedIdentity> {
    let header = decode_header(token).context("not a JWS")?;
    ensure!(header.alg == Algorithm::ES256, "identity credentials are ES256");
    let kid = header.kid.context("no key id")?;
    let issuer = trusted.iter().find(|i| i.kid == kid).context("untrusted issuer key")?;
    let key = DecodingKey::from_ec_components(&issuer.x, &issuer.y).context("issuer key")?;

    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_audience(&[world]);
    validation.set_issuer(&[&issuer.issuer]);
    validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);
    let claims = decode::<IdentityClaims>(token, &key, &validation)?.claims;

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
    /// The agent's per-world identifier.
    pub sub: String,
    /// The per-world identifier of the responsible human.
    pub responsible: String,
    /// The responsible human's assurance level when the delegation was issued.
    pub level: Level,
    pub iat: u64,
    pub exp: u64,
}

/// Issues and verifies delegations with the world's secret (HS256).
pub struct DelegationKey {
    world: String,
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl DelegationKey {
    pub fn new(world: &str, secret: &[u8]) -> Self {
        Self {
            world: world.to_owned(),
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    pub fn issue(&self, agent: &str, responsible: &str, level: Level, lifetime: Duration) -> Result<String> {
        let iat = unix_now();
        let claims = DelegationClaims {
            iss: self.world.clone(),
            aud: self.world.clone(),
            sub: agent.to_owned(),
            responsible: responsible.to_owned(),
            level,
            iat,
            exp: iat + lifetime.as_secs(),
        };
        Ok(encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)?)
    }

    pub fn verify(&self, token: &str) -> Result<DelegationClaims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[&self.world]);
        validation.set_issuer(&[&self.world]);
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);
        Ok(decode::<DelegationClaims>(token, &self.decoding, &validation)?.claims)
    }
}

/// A local issuer for development and tests. **Its key is public — never
/// trust it in a real world.**
pub struct TestIssuer {
    issuer: String,
    kid: String,
    encoding: EncodingKey,
    trusted: TrustedIssuer,
}

impl TestIssuer {
    /// The insecure development issuer: a fixed, published key.
    pub fn insecure_dev() -> Self {
        Self::from_seed("urn:osage:insecure-dev-issuer", "insecure-dev-1", [7; 32]).expect("valid dev key")
    }

    pub fn from_seed(issuer: &str, kid: &str, seed: [u8; 32]) -> Result<Self> {
        use p256::{elliptic_curve::sec1::ToSec1Point, pkcs8::EncodePrivateKey};

        let secret = p256::SecretKey::from_slice(&seed).context("seed is not a P-256 scalar")?;
        let der = secret.to_pkcs8_der().context("PKCS#8")?;
        let point = secret.public_key().to_sec1_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else { bail!("compressed point") };
        let trusted =
            TrustedIssuer { issuer: issuer.to_owned(), kid: kid.to_owned(), x: base64url(x), y: base64url(y) };
        Ok(Self {
            issuer: issuer.to_owned(),
            kid: kid.to_owned(),
            encoding: EncodingKey::from_ec_der(der.as_bytes()),
            trusted,
        })
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
        let mut header = Header::new(Algorithm::ES256);
        header.typ = Some("vc+jwt".to_owned());
        header.kid = Some(self.kid.clone());
        Ok(encode(&header, &claims, &self.encoding)?)
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
        }
    }
    out
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
        let token = key.issue("agent-1", "alice@demo", Level::A2, HOUR).unwrap();
        let claims = key.verify(&token).unwrap();
        assert_eq!((claims.sub.as_str(), claims.responsible.as_str()), ("agent-1", "alice@demo"));
        assert!(DelegationKey::new("another-world", b"secret of the demo world").verify(&token).is_err());
        assert!(DelegationKey::new(WORLD, b"another secret").verify(&token).is_err());
    }

    #[test]
    fn base64url_matches_rfc4648_vectors() {
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }
}
