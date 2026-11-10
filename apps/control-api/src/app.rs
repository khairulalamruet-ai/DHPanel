//! HTTP application: shared state, routes and middleware stack.

use std::{any::Any, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{FromRef, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use dhpanel_http_kit::{
    AppError,
    health::{self, Readiness},
};
use tower_http::{
    catch_panic::CatchPanicLayer, compression::CompressionLayer, limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer,
};

/// Renders the current metrics in Prometheus text format. A closure keeps the
/// router independent of the exporter type and easy to fake in tests.
pub type MetricsHandle = Arc<dyn Fn() -> String + Send + Sync>;

/// Whole-request timeout. Long-running work goes through the operations queue
/// (202 + operation_id), never a long HTTP request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Default maximum request body. Upload endpoints will set their own limits.
pub const BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;

/// State shared by all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Metrics renderer for `/metrics`.
    pub metrics: MetricsHandle,
    /// Readiness probes for `/api/v1/public/ready`.
    pub readiness: Readiness,
}

impl FromRef<AppState> for Readiness {
    fn from_ref(state: &AppState) -> Self {
        state.readiness.clone()
    }
}

impl FromRef<AppState> for MetricsHandle {
    fn from_ref(state: &AppState) -> Self {
        Arc::clone(&state.metrics)
    }
}

/// Builds the HTTP router.
///
/// Layer order, outermost first:
/// correlation ID -> panic catcher -> metrics -> timeout -> body limit -> compression -> routes.
/// Unknown routes answer with a Problem Details 404.
pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness));

    Router::new()
        .nest("/api/v1/public", public)
        .route("/metrics", get(render_metrics))
        .fallback(|| async { AppError::not_found("route") })
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(axum::middleware::from_fn(
            dhpanel_http_kit::metrics::track_http_metrics,
        ))
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(axum::middleware::from_fn(
            dhpanel_http_kit::request_id::correlation_id_layer,
        ))
        .with_state(state)
}

async fn render_metrics(State(metrics): State<MetricsHandle>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics(),
    )
}

/// Turns a handler panic into a Problem Details 500 instead of dropping the connection.
fn panic_response(payload: Box<dyn Any + Send + 'static>) -> Response {
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_owned());
    AppError::internal(std::io::Error::other(format!(
        "handler panicked: {message}"
    )))
    .into_response()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use axum::{body::Body, http::Request};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    fn state(readiness: Readiness) -> AppState {
        AppState {
            metrics: Arc::new(|| "http_requests_total 1\n".to_owned()),
            readiness,
        }
    }

    async fn get_json(app: Router, uri: &str) -> (StatusCode, Value) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn health_is_ok_with_version() {
        let (status, body) =
            get_json(router(state(Readiness::default())), "/api/v1/public/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn ready_is_200_when_all_probes_pass() {
        let readiness =
            Readiness::default().with_probe("database", Arc::new(|| Box::pin(async { Ok(()) })));
        let (status, body) = get_json(router(state(readiness)), "/api/v1/public/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
        assert_eq!(body["checks"][0]["name"], "database");
        assert_eq!(body["checks"][0]["status"], "ok");
    }

    #[tokio::test]
    async fn ready_is_503_when_a_probe_fails() {
        let readiness = Readiness::default().with_probe(
            "database",
            Arc::new(|| Box::pin(async { Err("down".to_owned()) })),
        );
        let (status, body) = get_json(router(state(readiness)), "/api/v1/public/ready").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["checks"][0]["status"], "failed");
    }

    #[tokio::test]
    async fn metrics_endpoint_returns_prometheus_text() {
        let res = router(state(Readiness::default()))
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/plain"), "{ct}");
        assert!(res.headers().contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn unknown_route_is_problem_json_with_correlation_id() {
        let res = router(state(Readiness::default()))
            .oneshot(
                Request::builder()
                    .uri("/no/such/route")
                    .header("x-request-id", "corr-abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            res.headers()[header::CONTENT_TYPE],
            dhpanel_http_kit::error::PROBLEM_JSON
        );
        assert_eq!(res.headers()["x-request-id"], "corr-abc");
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "not_found");
        assert_eq!(body["correlation_id"], "corr-abc");
    }

    #[tokio::test]
    async fn panic_becomes_problem_500_and_keeps_correlation_id() {
        async fn boom() -> &'static str {
            panic!("secret internal detail")
        }
        // Same panic + correlation layers as the real router, plus a route that panics.
        let app = Router::new()
            .route("/boom", get(boom))
            .layer(CatchPanicLayer::custom(panic_response))
            .layer(axum::middleware::from_fn(
                dhpanel_http_kit::request_id::correlation_id_layer,
            ));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/boom")
                    .header("x-request-id", "corr-panic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(res.headers()["x-request-id"], "corr-panic");
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(r#""code":"internal_error""#), "{text}");
        assert!(!text.contains("secret internal detail"), "{text}");
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let app = Router::new()
            .route(
                "/echo",
                axum::routing::post(|body: String| async move { body }),
            )
            .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES));
        let big = "x".repeat(BODY_LIMIT_BYTES + 1);
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .body(Body::from(big))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
