use crate::auth::oidc::OidcConfig;
use crate::auth::saml::SamlConfig;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_issuer: String,
    /// OIDC configuration (optional — set OIDC_DISCOVERY_URL to enable)
    pub oidc: Option<OidcConfig>,
    /// SAML configuration (optional — set SAML_IDP_SSO_URL to enable)
    pub saml: Option<SamlConfig>,
    /// Application environment: "production", "staging", "development"
    pub app_env: String,
    /// Rate limiting: auth endpoints (per IP per minute)
    pub rate_limit_auth: u32,
    /// Rate limiting: operation endpoints (per user per minute)
    pub rate_limit_operations: u32,
    /// Rate limiting: read endpoints (per user per minute)
    pub rate_limit_reads: u32,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
        let is_production = app_env == "production";

        // JWT_SECRET: required in production, fallback in dev
        let jwt_secret = match std::env::var("JWT_SECRET") {
            Ok(secret) => {
                // Warn if secret looks insecure
                if secret.contains("dev") || secret.contains("change") || secret.len() < 32 {
                    tracing::warn!(
                        "JWT_SECRET appears insecure (contains 'dev'/'change' or < 32 chars). \
                         Use a strong random secret in production."
                    );
                    if is_production {
                        panic!(
                            "FATAL: JWT_SECRET is insecure and APP_ENV=production. \
                             Set a strong random JWT_SECRET (>= 32 chars)."
                        );
                    }
                }
                secret
            }
            Err(_) => {
                if is_production {
                    panic!(
                        "FATAL: JWT_SECRET must be set when APP_ENV=production. \
                         Generate one with: openssl rand -base64 48"
                    );
                }
                tracing::warn!("JWT_SECRET not set — using dev default. NOT SAFE FOR PRODUCTION.");
                "dev-secret-change-in-production".to_string()
            }
        };

        // DATABASE_URL: required in production, fallback in dev
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => {
                if is_production && url.contains("appcontrol:appcontrol@localhost") {
                    tracing::warn!("DATABASE_URL uses default credentials in production!");
                }
                url
            }
            Err(_) => {
                if is_production {
                    panic!("FATAL: DATABASE_URL must be set when APP_ENV=production.");
                }
                tracing::warn!(
                    "DATABASE_URL not set — using localhost default. NOT SAFE FOR PRODUCTION."
                );
                "postgresql://appcontrol:appcontrol@localhost:5432/appcontrol".to_string()
            }
        };

        Self {
            database_url,
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            jwt_secret,
            jwt_issuer: std::env::var("JWT_ISSUER").unwrap_or_else(|_| "appcontrol".to_string()),
            oidc: OidcConfig::from_env(),
            saml: SamlConfig::from_env(),
            app_env,
            rate_limit_auth: std::env::var("RATE_LIMIT_AUTH")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            rate_limit_operations: std::env::var("RATE_LIMIT_OPERATIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            rate_limit_reads: std::env::var("RATE_LIMIT_READS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200),
        }
    }
}
