// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compact JWS (RFC 7515) for the two algorithms osage credentials use:
//! ES256 for identity credentials, HS256 for the world's delegations. The
//! caller chooses the algorithm; a token never does — `none` or any other
//! algorithm is refused, and so are critical header extensions.

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64ct::{Base64UrlUnpadded, Encoding};
use hmac::{Hmac, KeyInit, Mac};
use p256::ecdsa::{
    Signature, SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Header {
    pub(crate) alg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) typ: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kid: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) crit: Option<serde_json::Value>,
}

impl Header {
    pub(crate) fn new(alg: &str) -> Self {
        Self { alg: alg.to_owned(), ..Self::default() }
    }
}

/// A compact JWS split into its parts; nothing is verified yet.
pub(crate) struct Unverified<'a> {
    pub(crate) header: Header,
    signing_input: &'a str,
    payload: &'a str,
    signature: Vec<u8>,
}

pub(crate) fn parse(token: &str) -> Result<Unverified<'_>> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        bail!("not a compact JWS");
    };
    let signing_input = &token[..header.len() + 1 + payload.len()];
    let header: Header = serde_json::from_slice(&decode(header)?).context("malformed JWS header")?;
    ensure!(header.crit.is_none(), "critical JWS extensions are not supported");
    Ok(Unverified { header, signing_input, payload, signature: decode(signature)? })
}

impl Unverified<'_> {
    /// The claims, once the token's ES256 signature checks with `key`.
    pub(crate) fn verify_es256<C: DeserializeOwned>(&self, key: &VerifyingKey) -> Result<C> {
        ensure!(self.header.alg == "ES256", "expected an ES256 JWS, not {}", self.header.alg);
        let signature = Signature::from_slice(&self.signature).context("malformed ES256 signature")?;
        key.verify(self.signing_input.as_bytes(), &signature).map_err(|_| anyhow!("invalid signature"))?;
        self.claims()
    }

    /// The claims, once the token's HS256 signature checks with `secret`.
    pub(crate) fn verify_hs256<C: DeserializeOwned>(&self, secret: &[u8]) -> Result<C> {
        ensure!(self.header.alg == "HS256", "expected an HS256 JWS, not {}", self.header.alg);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
        mac.update(self.signing_input.as_bytes());
        mac.verify_slice(&self.signature).map_err(|_| anyhow!("invalid signature"))?;
        self.claims()
    }

    fn claims<C: DeserializeOwned>(&self) -> Result<C> {
        serde_json::from_slice(&decode(self.payload)?).context("malformed claims")
    }
}

pub(crate) fn sign_es256(header: &Header, claims: &impl Serialize, key: &SigningKey) -> Result<String> {
    let input = signing_input(header, claims)?;
    let signature: Signature = key.sign(input.as_bytes());
    Ok(format!("{input}.{}", encode(&signature.to_bytes())))
}

pub(crate) fn sign_hs256(header: &Header, claims: &impl Serialize, secret: &[u8]) -> Result<String> {
    let input = signing_input(header, claims)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
    mac.update(input.as_bytes());
    Ok(format!("{input}.{}", encode(&mac.finalize().into_bytes())))
}

fn signing_input(header: &Header, claims: &impl Serialize) -> Result<String> {
    Ok(format!("{}.{}", encode(&serde_json::to_vec(header)?), encode(&serde_json::to_vec(claims)?)))
}

pub(crate) fn encode(bytes: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(bytes)
}

pub(crate) fn decode(text: &str) -> Result<Vec<u8>> {
    Base64UrlUnpadded::decode_vec(text).map_err(|_| anyhow!("malformed base64url"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7515 appendix A.3: an ES256 JWS made by another implementation.
    #[test]
    fn verifies_the_rfc_7515_es256_example() {
        let token = "eyJhbGciOiJFUzI1NiJ9\
            .eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ\
            .DtEhU3ljbEg8L38VWAfUAqOyKAM6-Xx-F4GawxaepmXFCgfTjDxw5djxLa8ISlSApmWQxfKTUJqPP3-Kg6NU1Q";
        let point = [
            vec![4],
            decode("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU").unwrap(),
            decode("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0").unwrap(),
        ]
        .concat();
        let key = VerifyingKey::from_sec1_bytes(&point).unwrap();
        let claims: serde_json::Value = parse(token).unwrap().verify_es256(&key).unwrap();
        assert_eq!(claims["iss"], "joe");

        let tampered = token.replace(".eyJpc3MiOiJqb2Ui", ".eyJpc3MiOiJqb2Ii");
        assert!(parse(&tampered).unwrap().verify_es256::<serde_json::Value>(&key).is_err());
    }

    /// RFC 7515 appendix A.1: an HS256 JWS made by another implementation.
    #[test]
    fn verifies_the_rfc_7515_hs256_example() {
        let token = "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9\
            .eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ\
            .dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let secret =
            decode("AyM1SysPpbyDfgZld3umj1qzKObwVMkoqQ-EstJQLr_T-1qS0gZH75aKtMN3Yj0iPS4hcgUuTwjAzZr1Z9CAow").unwrap();
        let claims: serde_json::Value = parse(token).unwrap().verify_hs256(&secret).unwrap();
        assert_eq!(claims["iss"], "joe");
        assert!(parse(token).unwrap().verify_hs256::<serde_json::Value>(b"another secret").is_err());
    }

    #[test]
    fn the_token_never_chooses_the_algorithm() {
        let secret = b"a world secret";
        let hs256 = sign_hs256(&Header::new("HS256"), &serde_json::json!({"sub": "x"}), secret).unwrap();
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        assert!(parse(&hs256).unwrap().verify_es256::<serde_json::Value>(key.verifying_key()).is_err());

        let none = format!("{}.{}.", encode(br#"{"alg":"none"}"#), encode(br#"{"sub":"x"}"#));
        assert!(parse(&none).unwrap().verify_hs256::<serde_json::Value>(secret).is_err());

        let critical = format!("{}.{}.", encode(br#"{"alg":"HS256","crit":["b64"],"b64":false}"#), encode(b"{}"));
        assert!(parse(&critical).is_err());
        assert!(parse("a.b").is_err() && parse("a.b.c.d").is_err());
    }
}
