//! DHPanel control plane HTTP API (Axum).
//!
//! Startup: load and validate configuration (`dhpanel-config`), initialise
//! telemetry, serve HTTP, shut down gracefully on SIGINT/SIGTERM.

mod app;
mod modules;
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

    // Module manifests are part of the build; a broken one is a build mistake
    // that must not reach users half-working.
    let registry = match modules::registry() {
        Ok(registry) => {
            tracing::info!(
                modules = %registry.names().collect::<Vec<_>>().join(", "),
                "module registry built"
            );
            registry
        }
        Err(messages) => {
            for message in messages {
                tracing::error!("{message}");
                eprintln!("{SERVICE}: {message}");
            }
            telemetry.shutdown();
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

    // Tell the database which modules this build has (retried until it works).
    tokio::spawn(modules::sync_until_done(db.clone(), registry.clone()));

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
    // No CA yet is fine (enrollment then answers 503); a CA that exists but is
    // unusable (expired, wrong key, key readable by others) must not be ignored.
    let ca = match dhpanel_pki::load_ca_dir(&config.pki.ca_dir, time::OffsetDateTime::now_utc()) {
        Ok(ca) => {
            tracing::info!(fingerprint = ca.fingerprint_sha256(), "node CA loaded");
            Some(Arc::new(ca))
        }
        Err(dhpanel_pki::PkiError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            tracing::warn!(
                dir = %config.pki.ca_dir.display(),
                "no node CA found: node enrollment is switched off; create one with `dhctl ca init`"
            );
            None
        }
        Err(err) => {
            tracing::error!(error = %err, "node CA unusable");
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    // The agent port: mutual TLS, only with a CA and a server certificate.
    let gateway = match (config.agent.stream_listen, &ca) {
        (None, _) => None,
        (Some(_), None) => {
            tracing::error!(
                "agent.stream_listen is set but there is no node CA: run `dhctl ca init`"
            );
            telemetry.shutdown();
            return ExitCode::from(EXIT_CONFIG);
        }
        (Some(addr), Some(ca)) => {
            let tls = match dhpanel_node_gateway::tls::load_server_config(
                ca.certificate_pem(),
                &config.pki.ca_dir,
            ) {
                Ok(tls) => tls,
                Err(err) => {
                    tracing::error!(error = %err, "agent port TLS unusable: create the server certificate with `dhctl ca issue-server`");
                    telemetry.shutdown();
                    return ExitCode::from(EXIT_CONFIG);
                }
            };
            let listener = match dhpanel_node_gateway::bind(addr).await {
                Ok(l) => l,
                Err(err) => {
                    tracing::error!(%addr, error = %err, "failed to bind the agent port");
                    return ExitCode::FAILURE;
                }
            };
            tracing::info!(%addr, "agent port listening (mutual TLS)");
            Some((
                dhpanel_node_gateway::Gateway::new(
                    db.clone(),
                    tls,
                    dhpanel_node_gateway::Settings::default(),
                ),
                listener,
            ))
        }
    };
    let operations = dhpanel_operations::Operations::new(db.clone());
    let nodes = dhpanel_mod_nodes::NodesState {
        db: db.clone(),
        sessions: sessions.clone(),
        public_url: config.agent.public_url.as_deref().map(Into::into),
        stream_url: config.agent.stream_url.as_deref().map(Into::into),
        token_ttl: std::time::Duration::from_secs(
            u64::from(config.agent.enrollment_token_minutes) * 60,
        ),
        ca,
        limiter: dhpanel_mod_nodes::enrollment_limiter(redis.clone()),
        gateway: gateway.as_ref().map(|(gateway, _)| gateway.clone()),
        operations: operations.clone(),
        audit: dhpanel_audit::AuditWriter::new(db.clone()),
    };
    let admin = dhpanel_mod_admin::AdminState {
        db: db.clone(),
        sessions: sessions.clone(),
        hasher: hasher.clone(),
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
        audit: dhpanel_audit::AuditWriter::new(db.clone()),
    };

    let handle = telemetry.prometheus_handle();
    let app = app::router(
        app::AppState {
            metrics: Arc::new(move || handle.render()),
            readiness,
            trusted_proxies: config.trusted_proxies.clone().into(),
        },
        Some(auth),
        Some(admin),
        Some(nodes),
    );

    // One signal stops the API and the agent port.
    let (stop, stopped) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = stop.send(true);
    });
    // The dispatcher lives next to the agent port: it needs the live sessions.
    let dispatcher_task = gateway.as_ref().and_then(|(gateway, _)| {
        let results = gateway.take_results()?;
        let dispatcher = dhpanel_dispatcher::Dispatcher::new(
            operations.clone(),
            gateway.clone(),
            Some(redis.clone()),
            dhpanel_dispatcher::Settings::default(),
        );
        Some(tokio::spawn(dispatcher.run(results, stopped.clone())))
    });
    let gateway_task =
        gateway.map(|(gateway, listener)| tokio::spawn(gateway.serve(listener, stopped.clone())));

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
    .with_graceful_shutdown({
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.wait_for(|stop| *stop).await;
        }
    })
    .await;
    if let Some(task) = gateway_task {
        let _ = task.await;
    }
    if let Some(task) = dispatcher_task {
        let _ = task.await;
    }

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
