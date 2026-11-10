//! Application errors rendered as RFC 9457 Problem Details.
//!
//! Handlers return `Result<T, AppError>`. Each variant maps to one HTTP status
//! and a stable machine-readable `code` the frontend can switch on. Internal
//! errors are logged with their cause but answered with a generic message.

use std::borrow::Cow;

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Media type for problem details responses.
pub const PROBLEM_JSON: &str = "application/problem+json";
/// Base URI for problem `type` values (documentation anchor, not fetched).
pub const PROBLEM_TYPE_BASE: &str = "https://docs.dhpanel.dev/problems/";

/// One invalid input field (used with 422).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    /// JSON pointer-ish field path, e.g. `email` or `limits.max_domains`.
    pub field: String,
    /// Stable machine code, e.g. `required`, `too_long`, `invalid_format`.
    pub code: String,
    /// Human-readable message (English; the UI translates by `code`).
    pub message: String,
}

impl FieldError {
    /// Convenience constructor.
    pub fn new(
        field: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Error type returned by handlers.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 400 — malformed request (bad JSON, bad query string).
    #[error("bad request: {detail}")]
    BadRequest {
        /// Stable code.
        code: Cow<'static, str>,
        /// Explanation safe to show to the client.
        detail: String,
    },
    /// 401 — not authenticated.
    #[error("unauthorized")]
    Unauthorized,
    /// 403 — authenticated but not allowed. `code` distinguishes e.g.
    /// `permission_denied` from `feature_disabled`.
    #[error("forbidden: {code}")]
    Forbidden {
        /// Stable code.
        code: Cow<'static, str>,
        /// Explanation safe to show to the client.
        detail: String,
    },
    /// 404 — resource does not exist (or the caller may not know it exists).
    #[error("not found: {resource}")]
    NotFound {
        /// Resource kind, e.g. `domain`.
        resource: Cow<'static, str>,
    },
    /// 409 — state conflict, e.g. duplicate name or `limit_reached`.
    #[error("conflict: {code}")]
    Conflict {
        /// Stable code.
        code: Cow<'static, str>,
        /// Explanation safe to show to the client.
        detail: String,
    },
    /// 422 — well-formed but invalid input.
    #[error("validation failed ({} field errors)", errors.len())]
    Validation {
        /// Per-field problems.
        errors: Vec<FieldError>,
    },
    /// 500 — unexpected failure. The source is logged, never sent.
    #[error("internal error: {0}")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AppError {
    /// 400 with a code and client-safe detail.
    pub fn bad_request(code: &'static str, detail: impl Into<String>) -> Self {
        Self::BadRequest {
            code: code.into(),
            detail: detail.into(),
        }
    }
    /// 403 with a code and client-safe detail.
    pub fn forbidden(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Forbidden {
            code: code.into(),
            detail: detail.into(),
        }
    }
    /// 404 for a resource kind.
    pub fn not_found(resource: &'static str) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }
    /// 409 with a code and client-safe detail.
    pub fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Conflict {
            code: code.into(),
            detail: detail.into(),
        }
    }
    /// 422 from field errors.
    #[must_use]
    pub fn validation(errors: Vec<FieldError>) -> Self {
        Self::Validation { errors }
    }
    /// 500 wrapping any error.
    pub fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self::Internal(err.into())
    }

    /// HTTP status for this error.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::BadRequest { code, .. }
            | Self::Forbidden { code, .. }
            | Self::Conflict { code, .. } => code,
            Self::Unauthorized => "unauthenticated",
            Self::NotFound { .. } => "not_found",
            Self::Validation { .. } => "validation_failed",
            Self::Internal(_) => "internal_error",
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::BadRequest { detail, .. }
            | Self::Forbidden { detail, .. }
            | Self::Conflict { detail, .. } => detail.clone(),
            Self::Unauthorized => "Authentication is required.".to_owned(),
            Self::NotFound { resource } => format!("The requested {resource} was not found."),
            Self::Validation { .. } => "One or more fields are invalid.".to_owned(),
            Self::Internal(_) => {
                "An unexpected error occurred. Quote the correlation_id when contacting support."
                    .to_owned()
            }
        }
    }
}

/// Wire format (RFC 9457 + DHPanel extensions `code`, `correlation_id`, `errors`).
#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    /// Problem type URI.
    #[serde(rename = "type")]
    pub type_uri: String,
    /// Short summary of the status.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Human-readable explanation.
    pub detail: String,
    /// Stable machine-readable code.
    pub code: String,
    /// Request correlation ID, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Field errors (422 only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FieldError>,
}

impl From<&AppError> for ProblemDetails {
    fn from(err: &AppError) -> Self {
        let status = err.status();
        let code = err.code().to_owned();
        Self {
            type_uri: format!("{PROBLEM_TYPE_BASE}{code}"),
            title: status.canonical_reason().unwrap_or("Error").to_owned(),
            status: status.as_u16(),
            detail: err.detail(),
            code,
            correlation_id: crate::request_id::current(),
            errors: match err {
                AppError::Validation { errors } => errors.clone(),
                _ => Vec::new(),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(source) = &self {
            tracing::error!(error = %source, error.debug = ?source, "internal error");
        } else {
            tracing::debug!(
                status = self.status().as_u16(),
                code = self.code(),
                "request rejected"
            );
        }
        let body = ProblemDetails::from(&self);
        let mut response = (self.status(), Json(body)).into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        response
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use axum::{
        Router,
        body::Body,
        extract::{Path, Request},
        routing::get,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    async fn fail(Path(kind): Path<String>) -> Result<&'static str, AppError> {
        Err(match kind.as_str() {
            "400" => AppError::bad_request("malformed_json", "Body is not valid JSON."),
            "401" => AppError::Unauthorized,
            "403" => AppError::forbidden("feature_disabled", "Your plan does not include DNS."),
            "404" => AppError::not_found("domain"),
            "409" => AppError::conflict("limit_reached", "Domain limit (10) reached."),
            "422" => AppError::validation(vec![FieldError::new(
                "email",
                "invalid_format",
                "Not an email address.",
            )]),
            _ => AppError::internal(std::io::Error::other("db password=hunter2 refused")),
        })
    }

    async fn call(kind: &str) -> (StatusCode, String, Value) {
        let app = Router::new()
            .route("/fail/{kind}", get(fail))
            .layer(axum::middleware::from_fn(
                crate::request_id::correlation_id_layer,
            ));
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/fail/{kind}"))
                    .header("x-request-id", "test-corr-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let ct = res.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, ct, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn every_status_renders_problem_json() {
        for (kind, status, code) in [
            ("400", 400, "malformed_json"),
            ("401", 401, "unauthenticated"),
            ("403", 403, "feature_disabled"),
            ("404", 404, "not_found"),
            ("409", 409, "limit_reached"),
            ("422", 422, "validation_failed"),
            ("500", 500, "internal_error"),
        ] {
            let (got, ct, body) = call(kind).await;
            assert_eq!(got.as_u16(), status, "{kind}");
            assert_eq!(ct, PROBLEM_JSON, "{kind}");
            assert_eq!(body["status"], status, "{kind}");
            assert_eq!(body["code"], code, "{kind}");
            assert_eq!(body["type"], format!("{PROBLEM_TYPE_BASE}{code}"), "{kind}");
            assert_eq!(body["correlation_id"], "test-corr-1", "{kind}");
            assert!(
                body["title"].is_string() && body["detail"].is_string(),
                "{kind}"
            );
        }
    }

    #[tokio::test]
    async fn validation_lists_field_errors() {
        let (_, _, body) = call("422").await;
        assert_eq!(body["errors"][0]["field"], "email");
        assert_eq!(body["errors"][0]["code"], "invalid_format");
    }

    #[tokio::test]
    async fn non_validation_errors_omit_errors_array() {
        let (_, _, body) = call("409").await;
        assert!(body.get("errors").is_none());
    }

    #[tokio::test]
    async fn internal_error_does_not_leak_its_cause() {
        let (_, _, body) = call("500").await;
        let text = body.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("db password"), "{text}");
    }
}
