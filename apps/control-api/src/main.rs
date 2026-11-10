//! DHPanel control plane HTTP API (Axum).
//!
//! Startup: load and validate configuration (`dhpanel-config`), initialise
//! telemetry, serve HTTP, shut down gracefully on SIGINT/SIGTERM.

use std::{process::ExitCode, sync::Arc};

use axum::{Router, extract::State, http::header, response::IntoResponse, routing::get};
use dhpanel_config::ControlConfig;
use dhpanel_observability::{LogFormat, TelemetryConfig};

/// Renders the current metrics in Prometheus text format. A closure keeps the
/// router independent of the exporter type and easy to fake in tests.
type MetricsHandle = Arc<dyn Fn() -> String + Send + Sync>;

const SERVICE: &str = "control-api";

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

/// `EX_CONFIG` from sysexits.h: systemd and operators can tell config errors apart.
const EXIT_CONFIG: u8 = 78;

fn telemetry_config(config: &ControlConfig) -> TelemetryConfig {
    let mut t = TelemetryConfig::new(SERVICE);
    t.log_format = match config.log.format {
        dhpanel_config::LogFormat::Json => LogFormat::Json,
        dhpanel_config::LogFormat::Pretty => LogFormat::Pretty,
    };
    t.log_filter.clone_from(&config.log.filter);
    t.otlp_endpoint.clone_from(&config.log.otlp_endpoint);
    t
}

#[tokio::main]
async fn main() -> ExitCode {
    // Config comes first; telemetry is not up yet, so errors go to stderr.
    let config = match ControlConfig::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("{SERVICE}: {err}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    let telemetry = match dhpanel_observability::init(&telemetry_config(&config)) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("{SERVICE}: failed to initialise telemetry: {err}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    let handle = telemetry.prometheus_handle();
    let app = router(Arc::new(move || handle.render()));

    let addr = config.listen;
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
