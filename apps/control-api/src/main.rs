//! DHPanel control plane HTTP API (Axum).
//!
//! Startup: load and validate configuration (`dhpanel-config`), initialise
//! telemetry, serve HTTP, shut down gracefully on SIGINT/SIGTERM.

mod app;

use std::{process::ExitCode, sync::Arc};

use dhpanel_config::ControlConfig;
use dhpanel_http_kit::health::Readiness;
use dhpanel_observability::{LogFormat, TelemetryConfig};

const SERVICE: &str = "control-api";

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
    // Database and Redis probes are registered by T00.07 / T00.08.
    let app = app::router(app::AppState {
        metrics: Arc::new(move || handle.render()),
        readiness: Readiness::default(),
    });

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
