use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub org: String, // organization_id
    pub email: String,
    pub role: String,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
}

pub fn create_token(
    user_id: Uuid,
    org_id: Uuid,
    email: &str,
    role: &str,
    secret: &str,
    issuer: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        org: org_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        exp: now + 86400, // 24 hours
        iat: now,
        iss: issuer.to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn validate_token(
    token: &str,
    secret: &str,
    issuer: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    validation.set_issuer(&[issuer]);
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_validate_token() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = create_token(
            user_id,
            org_id,
            "test@example.com",
            "admin",
            "secret",
            "appcontrol",
        )
        .unwrap();
        let claims = validate_token(&token, "secret", "appcontrol").unwrap();
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.org, org_id.to_string());
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_invalid_secret_fails() {
        let token = create_token(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "t@t.com",
            "viewer",
            "secret1",
            "appcontrol",
        )
        .unwrap();
        let result = validate_token(&token, "wrong-secret", "appcontrol");
        assert!(result.is_err());
    }
}
