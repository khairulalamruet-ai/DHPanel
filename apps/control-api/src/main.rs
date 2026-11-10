//! DHPanel control plane HTTP API (Axum).
//!
//! T00.03 scope: telemetry initialisation and the Prometheus `/metrics` endpoint.
//! Configuration (T00.04), error format (T00.05) and health/readiness (T00.06)
//! build on this entry point.

use std::{net::SocketAddr, process::ExitCode, sync::Arc};

use axum::{Router, extract::State, http::header, response::IntoResponse, routing::get};
use dhpanel_observability::{LogFormat, TelemetryConfig};

/// Renders the current metrics in Prometheus text format. A closure keeps the
/// router independent of the exporter type and easy to fake in tests.
type MetricsHandle = Arc<dyn Fn() -> String + Send + Sync>;

const SERVICE: &str = "control-api";
/// Temporary until T00.04 introduces the config loader.
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";

/// Builds the HTTP router. Kept separate from `main` so tests can drive it.
fn router(metrics: MetricsHandle) -> Router {
    Router::new()
        .route("/metrics", get(render_metrics))
        .layer(axum::middleware::from_fn(
            dhpanel_http_kit::metrics::track_http_metrics,
        ))
        .with_state(metrics)
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

#[tokio::main]
async fn main() -> ExitCode {
    let mut telemetry_config = TelemetryConfig::new(SERVICE);
    if std::env::var("DHPANEL_LOG_FORMAT").as_deref() == Ok("pretty") {
        telemetry_config.log_format = LogFormat::Pretty;
    }
    if let Ok(filter) = std::env::var("RUST_LOG") {
        telemetry_config.log_filter = filter;
    }
    telemetry_config.otlp_endpoint = std::env::var("DHPANEL_OTLP_ENDPOINT").ok();

    let telemetry = match dhpanel_observability::init(&telemetry_config) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("{SERVICE}: failed to initialise telemetry: {err}");
            return ExitCode::FAILURE;
        }
    };

    let handle = telemetry.prometheus_handle();
    let app = router(Arc::new(move || handle.render()));

    let listen = std::env::var("DHPANEL_LISTEN").unwrap_or_else(|_| DEFAULT_LISTEN.to_owned());
    let addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(err) => {
            tracing::error!(%listen, error = %err, "invalid listen address");
            return ExitCode::FAILURE;
        }
    };
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            tracing::error!(%addr, error = %err, "failed to bind");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(%addr, version = dhpanel_common::VERSION, "{SERVICE} listening");
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    telemetry.shutdown();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{SERVICE}: server error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn metrics_endpoint_returns_prometheus_text() {
        let app = router(Arc::new(|| "http_requests_total 1\n".to_owned()));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap_or_default(),
            )
            .await
            .unwrap_or_default();
        assert_eq!(res.status(), 200);
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(ct.starts_with("text/plain"), "{ct}");
    }
}
