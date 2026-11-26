use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

const TOKEN_TTL_HOURS: i64 = 24;
const NONCE_BYTES: usize = 32;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid auth proof")]
    InvalidProof,
    #[error("invalid or expired nonce")]
    InvalidNonce,
    #[error("nickname must not be empty")]
    EmptyNickname,
    #[error("invalid token: {0}")]
    InvalidToken(String),
}

/// Random hex nonce for HMAC challenge–response.
pub fn generate_nonce() -> String {
    let mut buf = [0u8; NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// `HMAC-SHA256(access_key, "{nonce}:{nickname}")` as lowercase hex.
pub fn compute_proof(access_key: &str, nonce: &str, nickname: &str) -> Result<String, AuthError> {
    let mut mac = HmacSha256::new_from_slice(access_key.as_bytes())
        .map_err(|e| AuthError::InvalidToken(format!("hmac key: {e}")))?;
    mac.update(nonce.as_bytes());
    mac.update(b":");
    mac.update(nickname.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Constant-time verify of an HMAC proof against `ACCESS_KEY`.
pub fn verify_proof(
    access_key: &str,
    nonce: &str,
    nickname: &str,
    proof_hex: &str,
) -> Result<(), AuthError> {
    let Ok(provided) = hex::decode(proof_hex.trim()) else {
        return Err(AuthError::InvalidProof);
    };
    let mut mac = HmacSha256::new_from_slice(access_key.as_bytes())
        .map_err(|e| AuthError::InvalidToken(format!("hmac key: {e}")))?;
    mac.update(nonce.as_bytes());
    mac.update(b":");
    mac.update(nickname.as_bytes());
    mac.verify_slice(&provided)
        .map_err(|_| AuthError::InvalidProof)
}

pub fn normalize_nickname(nickname: &str) -> Result<String, AuthError> {
    let trimmed = nickname.trim();
    if trimmed.is_empty() {
        return Err(AuthError::EmptyNickname);
    }
    Ok(trimmed.to_string())
}

pub fn issue_token(nickname: &str, jwt_secret: &str) -> Result<String, AuthError> {
    let now = Utc::now();
    let claims = Claims {
        sub: nickname.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(TOKEN_TTL_HOURS)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AuthError::InvalidToken(e.to_string()))
}

pub fn verify_token(token: &str, jwt_secret: &str) -> Result<Claims, AuthError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| AuthError::InvalidToken(e.to_string()))?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_round_trip() {
        let key = "shared-secret";
        let nonce = "abc123";
        let nick = "alice";
        let proof = compute_proof(key, nonce, nick).expect("proof");
        assert!(verify_proof(key, nonce, nick, &proof).is_ok());
        assert!(matches!(
            verify_proof(key, nonce, "bob", &proof),
            Err(AuthError::InvalidProof)
        ));
        assert!(matches!(
            verify_proof("wrong", nonce, nick, &proof),
            Err(AuthError::InvalidProof)
        ));
    }

    #[test]
    fn proof_rejects_bad_hex() {
        assert!(matches!(
            verify_proof("k", "n", "alice", "not-hex!!"),
            Err(AuthError::InvalidProof)
        ));
    }

    #[test]
    fn nonce_is_hex_and_unique() {
        let a = generate_nonce();
        let b = generate_nonce();
        assert_eq!(a.len(), NONCE_BYTES * 2);
        assert_ne!(a, b);
        assert!(hex::decode(&a).is_ok());
    }

    #[test]
    fn token_round_trip() {
        let token = issue_token("alice", "jwt-secret").expect("issue");
        let claims = verify_token(&token, "jwt-secret").expect("verify");
        assert_eq!(claims.sub, "alice");
    }

    #[test]
    fn reject_empty_nickname() {
        assert!(matches!(
            normalize_nickname("   "),
            Err(AuthError::EmptyNickname)
        ));
    }
}
