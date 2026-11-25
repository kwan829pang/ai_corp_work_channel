use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOKEN_TTL_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid access key")]
    InvalidAccessKey,
    #[error("nickname must not be empty")]
    EmptyNickname,
    #[error("invalid token: {0}")]
    InvalidToken(String),
}

pub fn validate_access_key(expected: &str, provided: &str) -> Result<(), AuthError> {
    if expected == provided {
        Ok(())
    } else {
        Err(AuthError::InvalidAccessKey)
    }
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
    fn access_key_check() {
        assert!(validate_access_key("secret", "secret").is_ok());
        assert!(matches!(
            validate_access_key("secret", "wrong"),
            Err(AuthError::InvalidAccessKey)
        ));
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
