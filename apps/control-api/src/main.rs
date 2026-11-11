//! DHPanel control plane HTTP API (Axum).
//!
//! Startup: load and validate configuration (`dhpanel-config`), initialise
//! telemetry, serve HTTP, shut down gracefully on SIGINT/SIGTERM.

mod app;
mod openapi;

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

    // Lazy pool: the API starts even if PostgreSQL is down; /ready reports it.
    let db = match config
        .database_url()
        .map_err(|e| e.to_string())
        .and_then(|url| {
            dhpanel_database::connect_lazy(
                &url,
                &dhpanel_database::PoolSettings {
                    max_connections: config.database.max_connections,
                    application_name: SERVICE.to_owned(),
                },
            )
            .map_err(|e| e.to_string())
        }) {
        Ok(pool) => pool,
        Err(err) => {
            tracing::error!(error = %err, "database configuration invalid");
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    // Redis: URL parsed now, connection opened on first use; /ready reports outages.
    let redis = match config
        .redis_url()
        .map_err(|e| e.to_string())
        .and_then(|url| dhpanel_cache::RedisHandle::new(&url).map_err(|e| e.to_string()))
    {
        Ok(handle) => handle,
        Err(err) => {
            tracing::error!(error = %err, "redis configuration invalid");
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    let readiness = Readiness::default()
        .with_probe("database", {
            let db = db.clone();
            Arc::new(move || {
                let db = db.clone();
                Box::pin(
                    async move { dhpanel_database::ping(&db).await.map_err(|e| e.to_string()) },
                )
            })
        })
        .with_probe("redis", {
            let redis = redis.clone();
            Arc::new(move || {
                let redis = redis.clone();
                Box::pin(async move { redis.ping().await.map_err(|e| e.to_string()) })
            })
        });

    let hasher = match dhpanel_auth_core::password::PasswordHasher::new(
        dhpanel_auth_core::password::Argon2Params {
            memory_kib: config.auth.argon2_memory_kib,
            iterations: config.auth.argon2_iterations,
            parallelism: config.auth.argon2_parallelism,
        },
    ) {
        Ok(h) => h,
        Err(err) => {
            tracing::error!(error = %err, "password hashing configuration invalid");
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let sessions = dhpanel_session::SessionStore::new(
        redis.clone(),
        db.clone(),
        dhpanel_session::SessionConfig {
            idle_timeout: std::time::Duration::from_secs(
                u64::from(config.auth.session_idle_minutes) * 60,
            ),
            absolute_timeout: std::time::Duration::from_secs(
                u64::from(config.auth.session_absolute_hours) * 3600,
            ),
            sudo_window: std::time::Duration::from_secs(u64::from(config.auth.sudo_minutes) * 60),
            ..dhpanel_session::SessionConfig::default()
        },
    );
    let master_key = match config
        .master_key()
        .map_err(|e| e.to_string())
        .and_then(|key| {
            dhpanel_auth_core::secretbox::MasterKey::from_base64(
                secrecy::ExposeSecret::expose_secret(&key),
            )
            .map_err(|e| format!("secrets.master_key: {e}"))
        }) {
        Ok(key) => key,
        Err(err) => {
            tracing::error!(error = %err, "master key unavailable");
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let minutes = |m: u32| std::time::Duration::from_secs(u64::from(m) * 60);
    let lockout = |threshold| dhpanel_cache::lockout::LockoutPolicy {
        threshold,
        window: minutes(config.auth.lockout_window_minutes),
        lockout: minutes(config.auth.lockout_minutes),
    };
    let auth = dhpanel_mod_auth::AuthState {
        db: db.clone(),
        sessions,
        hasher,
        limits: dhpanel_mod_auth::LoginLimits::new(
            redis.clone(),
            lockout(config.auth.lockout_threshold),
            lockout(config.auth.ip_lockout_threshold),
        ),
        trusted_proxies: config.trusted_proxies.clone().into(),
        redis: redis.clone(),
        master_key,
        totp_issuer: config.auth.totp_issuer.trim().into(),
    };

    let handle = telemetry.prometheus_handle();
    let app = app::router(
        app::AppState {
            metrics: Arc::new(move || handle.render()),
            readiness,
        },
        Some(auth),
    );

    let addr = config.listen;
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            tracing::error!(%addr, error = %err, "failed to bind");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(%addr, version = dhpanel_common::VERSION, "{SERVICE} listening");
    // Connect info gives handlers the peer address (login events, sessions).
    let result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;

    db.close().await;
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
