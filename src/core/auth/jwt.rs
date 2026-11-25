//! Emissão e validação de access tokens (JWT HS256).
//!
//! O token carrega `role`, `token_version` e a família de refresh (`fid`).
//! Permissões são derivadas do papel no servidor; não vão no token.

use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{config::settings::JwtSettings, core::auth::role::Role};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: Role,
    pub token_version: i32,
    /// Família de refresh: identifica a sessão/dispositivo.
    pub fid: Uuid,
    pub jti: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub aud: String,
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("token inválido: {0}")]
    Invalid(#[from] jsonwebtoken::errors::Error),
}

#[derive(Clone)]
pub struct JwtService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    issuer: String,
    audience: String,
    access_ttl: Duration,
}

impl JwtService {
    pub fn new(settings: &JwtSettings) -> Result<Self, JwtError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[settings.issuer.as_str()]);
        validation.set_audience(&[settings.audience.as_str()]);
        validation.leeway = 5;

        Ok(Self {
            encoding: EncodingKey::from_secret(settings.secret.as_bytes())?,
            decoding: DecodingKey::from_secret(settings.secret.as_bytes())?,
            validation,
            issuer: settings.issuer.clone(),
            audience: settings.audience.clone(),
            access_ttl: settings.access_ttl(),
        })
    }

    pub fn access_ttl_secs(&self) -> i64 {
        i64::try_from(self.access_ttl.as_secs()).unwrap_or(900)
    }

    pub fn issue_access(
        &self,
        id: Uuid,
        email: &str,
        role: Role,
        token_version: i32,
        family_id: Uuid,
    ) -> Result<String, JwtError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: id.to_string(),
            email: email.to_owned(),
            role,
            token_version,
            fid: family_id,
            jti: Uuid::new_v4().simple().to_string(),
            iat: now,
            exp: now.saturating_add(self.access_ttl_secs()),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
        };

        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding).map_err(JwtError::from)
    }

    pub fn decode(&self, token: &str) -> Result<Claims, JwtError> {
        decode::<Claims>(token, &self.decoding, &self.validation)
            .map(|data| data.claims)
            .map_err(JwtError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> JwtSettings {
        JwtSettings {
            issuer: "teste".to_owned(),
            audience: "teste-front".to_owned(),
            access_ttl_min: 15,
            refresh_ttl_days: 7,
            secret: "x".repeat(48),
        }
    }

    #[test]
    fn emite_e_decodifica() {
        let service = JwtService::new(&settings()).expect("jwt");
        let id = Uuid::now_v7();
        let family_id = Uuid::now_v7();
        let token = service
            .issue_access(id, "a@b.com", Role::Admin, 3, family_id)
            .expect("token");
        let claims = service.decode(&token).expect("claims");

        assert_eq!(claims.sub, id.to_string());
        assert_eq!(claims.email, "a@b.com");
        assert_eq!(claims.role, Role::Admin);
        assert_eq!(claims.token_version, 3);
        assert_eq!(claims.fid, family_id);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn rejeita_token_adulterado() {
        let service = JwtService::new(&settings()).expect("jwt");
        let token = service
            .issue_access(Uuid::now_v7(), "a@b.com", Role::User, 0, Uuid::now_v7())
            .expect("token");
        let mut tampered = token.clone();
        tampered.push('x');

        assert!(service.decode(&tampered).is_err());
    }

    #[test]
    fn rejeita_token_de_outro_segredo() {
        let service = JwtService::new(&settings()).expect("jwt");
        let token = service
            .issue_access(Uuid::now_v7(), "a@b.com", Role::User, 0, Uuid::now_v7())
            .expect("token");

        let mut other = settings();
        other.secret = "y".repeat(48);
        let other_service = JwtService::new(&other).expect("jwt");

        assert!(other_service.decode(&token).is_err());
    }
}
