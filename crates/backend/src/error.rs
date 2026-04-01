//! Structured API error types.
//!
//! Replaces the pattern `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)`
//! with typed errors that preserve context for diagnostics.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Unified error type for all API handlers.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found")]
    NotFound,

    #[error("Forbidden: insufficient permissions")]
    Forbidden,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid input: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Service unavailable")]
    ServiceUnavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            ApiError::Database(sqlx::Error::RowNotFound) => (
                StatusCode::NOT_FOUND,
                "not_found",
                "Resource not found".to_string(),
            ),
            ApiError::Database(sqlx::Error::Database(db_err)) => {
                // PostgreSQL unique violation = "23505", SQLite UNIQUE constraint = "2067"
                let is_unique_violation =
                    matches!(db_err.code().as_deref(), Some("23505") | Some("2067"));
                // Also check message for SQLite which may not always set code
                let is_unique_msg = db_err
                    .message()
                    .to_lowercase()
                    .contains("unique constraint");
                if is_unique_violation || is_unique_msg {
                    (
                        StatusCode::CONFLICT,
                        "conflict",
                        "Resource already exists".to_string(),
                    )
                } else {
                    tracing::error!(
                        db_code = ?db_err.code(),
                        "Database error: {}", db_err
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "database_error",
                        "Database error".to_string(),
                    )
                }
            }
            ApiError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "Database error".to_string(),
                )
            }
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "Resource not found".to_string(),
            ),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "Insufficient permissions".to_string(),
            ),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required".to_string(),
            ),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            ApiError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error".to_string(),
                )
            }
            ApiError::ServiceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "Service unavailable".to_string(),
            ),
        };

        (
            status,
            Json(json!({ "error": error_type, "message": message })),
        )
            .into_response()
    }
}

/// Convenience trait for converting `Option<T>` to `Result<T, ApiError>`.
pub trait OptionExt<T> {
    fn ok_or_not_found(self) -> Result<T, ApiError>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_not_found(self) -> Result<T, ApiError> {
        self.ok_or(ApiError::NotFound)
    }
}

/// Validate a string field length.
pub fn validate_length(field: &str, value: &str, min: usize, max: usize) -> Result<(), ApiError> {
    if value.len() < min {
        return Err(ApiError::Validation(format!(
            "{} must be at least {} characters",
            field, min
        )));
    }
    if value.len() > max {
        return Err(ApiError::Validation(format!(
            "{} must be at most {} characters",
            field, max
        )));
    }
    Ok(())
}

/// Validate an optional string field length.
pub fn validate_optional_length(
    field: &str,
    value: &Option<String>,
    max: usize,
) -> Result<(), ApiError> {
    if let Some(v) = value {
        if v.len() > max {
            return Err(ApiError::Validation(format!(
                "{} must be at most {} characters",
                field, max
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_length_ok() {
        assert!(validate_length("name", "hello", 1, 200).is_ok());
    }

    #[test]
    fn test_validate_length_too_short() {
        let err = validate_length("name", "", 1, 200).unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[test]
    fn test_validate_length_too_long() {
        let long = "x".repeat(201);
        let err = validate_length("name", &long, 1, 200).unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[test]
    fn test_validate_optional_length_none_ok() {
        assert!(validate_optional_length("desc", &None, 2000).is_ok());
    }

    #[test]
    fn test_validate_optional_length_some_too_long() {
        let long = Some("x".repeat(2001));
        let err = validate_optional_length("desc", &long, 2000).unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[test]
    fn test_option_ext_not_found() {
        let result: Result<i32, ApiError> = None::<i32>.ok_or_not_found();
        assert!(matches!(result, Err(ApiError::NotFound)));
    }
}
