//! Correlation (request) IDs.
//!
//! [`correlation_id_layer`] accepts an incoming `x-request-id` if it is a sane
//! token, otherwise generates a UUIDv7. The ID is
//! * stored in a task-local so [`current`] works anywhere inside the handler
//!   (used by [`crate::error::AppError`] responses),
//! * attached to a `request` tracing span, so every log line carries it,
//! * echoed back in the `x-request-id` response header.

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;

/// Header carrying the correlation ID in both directions.
pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Longest client-supplied ID we accept; longer values are replaced.
const MAX_INBOUND_LEN: usize = 128;

tokio::task_local! {
    static CORRELATION_ID: String;
}

/// The current request's correlation ID, if called inside the middleware scope.
#[must_use]
pub fn current() -> Option<String> {
    CORRELATION_ID.try_with(Clone::clone).ok()
}

fn accept_inbound(value: &HeaderValue) -> Option<String> {
    let s = value.to_str().ok()?;
    let ok = !s.is_empty()
        && s.len() <= MAX_INBOUND_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    ok.then(|| s.to_owned())
}

/// Middleware: assigns the correlation ID and runs the rest of the stack inside its scope.
///
/// Install outermost so every other layer (metrics, tracing, errors) sees the ID.
pub async fn correlation_id_layer(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(accept_inbound)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    if let Ok(v) = HeaderValue::from_str(&id) {
        request.headers_mut().insert(REQUEST_ID_HEADER.clone(), v);
    }

    let span = tracing::info_span!(
        "request",
        correlation_id = %id,
        method = %request.method(),
        path = %request.uri().path(),
    );

    let mut response = CORRELATION_ID
        .scope(id.clone(), next.run(request).instrument(span))
        .await;

    if let Ok(v) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER.clone(), v);
    }
    response
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use axum::{Router, body::Body, routing::get};
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        Router::new()
            .route("/", get(|| async { current().unwrap_or_default() }))
            .layer(axum::middleware::from_fn(correlation_id_layer))
    }

    async fn call(header: Option<&str>) -> (String, String) {
        let mut req = Request::builder().uri("/");
        if let Some(h) = header {
            req = req.header("x-request-id", h);
        }
        let res = app()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let echoed = res.headers()[&REQUEST_ID_HEADER]
            .to_str()
            .unwrap()
            .to_owned();
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        (echoed, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn generates_uuid_v7_when_missing() {
        let (echoed, seen) = call(None).await;
        assert_eq!(echoed, seen);
        let parsed = uuid::Uuid::parse_str(&echoed).unwrap();
        assert_eq!(parsed.get_version_num(), 7);
    }

    #[tokio::test]
    async fn keeps_a_sane_inbound_id() {
        let (echoed, seen) = call(Some("abc-123_x.y")).await;
        assert_eq!(echoed, "abc-123_x.y");
        assert_eq!(seen, "abc-123_x.y");
    }

    #[tokio::test]
    async fn replaces_unsafe_inbound_id() {
        let (echoed, _) = call(Some("bad id<script>")).await;
        assert_ne!(echoed, "bad id<script>");
        assert!(uuid::Uuid::parse_str(&echoed).is_ok());

        let long = "a".repeat(MAX_INBOUND_LEN + 1);
        let (echoed, _) = call(Some(&long)).await;
        assert_ne!(echoed, long);
    }

    #[test]
    fn current_is_none_outside_a_request() {
        assert!(current().is_none());
    }
}
