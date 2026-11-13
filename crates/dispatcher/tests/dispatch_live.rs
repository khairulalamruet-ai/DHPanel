//! Operations from the ledger to real agents and back: real TLS, the real
//! gateway, the real agent connection code, real PostgreSQL.
//! Ignored by default; run with DATABASE_URL (superuser) and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use dhpanel_database::nodes::{self, NewNode, NewNodeCertificate};
use dhpanel_dispatcher::{Dispatcher, Settings};
use dhpanel_node_agent::{
    ops::{Executor, Registry, cache::ResultCache},
    stream::{self, Params},
};
use dhpanel_node_gateway::{Gateway, NodeResult, Settings as GatewaySettings, tls};
use dhpanel_node_protocol::{
    AgentError, ErrorCode, Operation, OperationResult, Output, ResultStatus,
};
use dhpanel_operations::{Operations, Request, Row, Status};
use dhpanel_pki::CertificateAuthority;
use rcgen::{CertificateParams, KeyPair};
use secrecy::{ExposeSecret as _, SecretString};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::sync::watch;
use uuid::Uuid;

struct Plane {
    ca: Arc<CertificateAuthority>,
    gateway: Gateway,
    addr: SocketAddr,
    ops: Operations,
    stop: watch::Sender<bool>,
}

fn dispatch_settings() -> Settings {
    Settings {
        poll: Duration::from_millis(50),
        batch: 50,
        operation_timeout: Duration::from_secs(1),
        retry_base: Duration::from_secs(1),
        retry_cap: Duration::from_secs(2),
        capabilities_check: Duration::from_millis(200),
        capabilities_max_age: Duration::from_secs(3600),
        purge_check: Duration::from_secs(3600),
    }
}

async fn plane(pool: &PgPool) -> Plane {
    let ca = Arc::new(
        CertificateAuthority::create(
            "Test CA",
            OffsetDateTime::now_utc(),
            time::Duration::days(3650),
        )
        .unwrap()
        .0,
    );
    let (server, key) = ca
        .issue_server_certificate(
            &["127.0.0.1".to_owned()],
            OffsetDateTime::now_utc(),
            time::Duration::days(30),
        )
        .unwrap();
    let config = tls::server_config(
        ca.certificate_pem(),
        server.cert_pem.as_bytes(),
        key.expose_secret().as_bytes(),
    )
    .unwrap();
    let gateway = Gateway::new(
        pool.clone(),
        config,
        GatewaySettings {
            ping_interval: Duration::from_millis(200),
            liveness_timeout: Duration::from_secs(30),
            handshake_timeout: Duration::from_secs(5),
            max_connections: 64,
        },
    );
    let listener = dhpanel_node_gateway::bind("127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(gateway.clone().serve(listener, stopped));
    Plane {
        ca,
        gateway,
        addr,
        ops: Operations::new(pool.clone()),
        stop,
    }
}

impl Plane {
    /// Runs the dispatcher the way control-api does.
    fn start_dispatcher(&self) -> Dispatcher {
        let d = Dispatcher::new(
            self.ops.clone(),
            self.gateway.clone(),
            None,
            dispatch_settings(),
        );
        let results = self.gateway.take_results().expect("results taken once");
        tokio::spawn(d.clone().run(results, self.stop.subscribe()));
        d
    }
}

struct TestNode {
    id: Uuid,
    ca_pem: String,
    cert_pem: String,
    key_pem: String,
    /// The agent's operation handlers and memory; kept across agent restarts.
    executor: Arc<Executor>,
}

impl TestNode {
    fn params(&self, addr: SocketAddr) -> Params {
        Params {
            address: format!("https://{addr}"),
            ca_pem: self.ca_pem.clone(),
            cert_pem: self.cert_pem.clone(),
            key_pem: SecretString::from(self.key_pem.clone()),
        }
    }

    fn start_agent(&self, addr: SocketAddr) -> (watch::Sender<bool>, tokio::task::JoinHandle<()>) {
        let (stop, stopped) = watch::channel(false);
        let params = self.params(addr);
        let executor = self.executor.clone();
        let task =
            tokio::spawn(async move { stream::run(params, executor, stopped).await.unwrap() });
        (stop, task)
    }
}

async fn make_node(p: &Plane, pool: &PgPool, name: &str) -> TestNode {
    make_node_with(p, pool, name, Registry::builtin(&std::env::temp_dir())).await
}

async fn make_node_with(p: &Plane, pool: &PgPool, name: &str, registry: Registry) -> TestNode {
    let id = Uuid::now_v7();
    let key = KeyPair::generate().unwrap();
    let csr = CertificateParams::new(Vec::<String>::new())
        .unwrap()
        .serialize_request(&key)
        .and_then(|c| c.pem())
        .unwrap();
    let issued =
        p.ca.issue_node_certificate(
            &csr,
            id,
            OffsetDateTime::now_utc(),
            time::Duration::days(90),
        )
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    nodes::insert_node(
        &mut conn,
        &NewNode {
            id,
            name,
            hostname: None,
            public_ip: None,
            agent_version: None,
        },
    )
    .await
    .unwrap();
    nodes::insert_node_certificate(
        &mut conn,
        &NewNodeCertificate {
            node_id: id,
            serial: &issued.serial_hex,
            fingerprint_sha256: &issued.fingerprint_sha256,
            not_before: issued.not_before,
            not_after: issued.not_after,
        },
    )
    .await
    .unwrap();
    TestNode {
        id,
        ca_pem: p.ca.certificate_pem().to_owned(),
        cert_pem: issued.cert_pem,
        key_pem: key.serialize_pem(),
        executor: Executor::new(registry, ResultCache::in_memory().unwrap(), 8),
    }
}

async fn wait_connected(p: &Plane, node: Uuid) {
    for _ in 0..100 {
        if p.gateway.is_connected(node) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("node never connected");
}

async fn wait_status(ops: &Operations, id: Uuid, wanted: Status, within: Duration) -> Row {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let row = ops.get(id).await.unwrap().unwrap();
        if row.status == wanted {
            return row;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "operation stayed {:?}, wanted {wanted:?}: {row:?}",
            row.status
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn a_ping_goes_to_the_agent_and_comes_back_succeeded(pool: PgPool) {
    let p = plane(&pool).await;
    p.start_dispatcher();
    let node = make_node(&p, &pool, "alpha").await;
    let (agent_stop, agent) = node.start_agent(p.addr);
    wait_connected(&p, node.id).await;

    let queued = p
        .ops
        .enqueue(&Request::new(
            node.id,
            Operation::Ping { nonce: 4242 },
            "ping-1",
        ))
        .await
        .unwrap();
    let id = queued.operation().id;
    let row = wait_status(&p.ops, id, Status::Succeeded, Duration::from_secs(5)).await;
    assert_eq!(row.attempts, 1);
    assert_eq!(
        row.output,
        Some(serde_json::json!({"type": "pong", "nonce": 4242}))
    );
    assert!(row.finished_at.is_some() && row.dispatched_at.is_some());

    // Asking again with the same key does not send a second ping.
    let again = p
        .ops
        .enqueue(&Request::new(
            node.id,
            Operation::Ping { nonce: 4242 },
            "ping-1",
        ))
        .await
        .unwrap();
    assert_eq!(again.operation().id, id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(p.ops.get(id).await.unwrap().unwrap().attempts, 1);

    agent_stop.send(true).unwrap();
    agent.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn an_operation_for_an_offline_node_waits_and_is_sent_when_it_connects(pool: PgPool) {
    let p = plane(&pool).await;
    p.start_dispatcher();
    let node = make_node(&p, &pool, "beta").await;
    let id = p
        .ops
        .enqueue(&Request::new(
            node.id,
            Operation::Ping { nonce: 1 },
            "waiting",
        ))
        .await
        .unwrap()
        .operation()
        .id;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let row = p.ops.get(id).await.unwrap().unwrap();
    assert_eq!(
        (row.status, row.attempts),
        (Status::Pending, 0),
        "nothing is sent to nobody"
    );

    let (agent_stop, agent) = node.start_agent(p.addr);
    wait_status(&p.ops, id, Status::Succeeded, Duration::from_secs(8)).await;
    agent_stop.send(true).unwrap();
    agent.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn an_operation_the_agent_cannot_run_fails_for_good_without_retries(pool: PgPool) {
    let p = plane(&pool).await;
    p.start_dispatcher();
    // An agent build without the capability handler (like an older agent).
    let mut registry = Registry::new();
    registry.register("ping", |_| async { Ok(Output::Pong { nonce: 0 }) });
    let node = make_node_with(&p, &pool, "gamma", registry).await;
    let (agent_stop, agent) = node.start_agent(p.addr);
    wait_connected(&p, node.id).await;
    let id = p
        .ops
        .enqueue(&Request::new(
            node.id,
            Operation::ReportCapabilities,
            "caps",
        ))
        .await
        .unwrap()
        .operation()
        .id;
    let row = wait_status(&p.ops, id, Status::Failed, Duration::from_secs(5)).await;
    assert_eq!(row.attempts, 1);
    assert_eq!(row.error.unwrap()["code"], "unsupported_operation");
    agent_stop.send(true).unwrap();
    agent.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn results_are_only_accepted_from_the_node_the_operation_went_to(pool: PgPool) {
    let p = plane(&pool).await;
    let d = Dispatcher::new(p.ops.clone(), p.gateway.clone(), None, dispatch_settings());
    let (a, b) = (
        make_node(&p, &pool, "node-a").await,
        make_node(&p, &pool, "node-b").await,
    );
    let id = p
        .ops
        .enqueue(&Request::new(a.id, Operation::Ping { nonce: 1 }, "k"))
        .await
        .unwrap()
        .operation()
        .id;
    assert_eq!(p.ops.claim_due(&[a.id], 10).await.unwrap().len(), 1);

    // B claims to have finished A's operation: ignored.
    d.handle_result(NodeResult {
        node_id: b.id,
        result: OperationResult::success(id, Output::Pong { nonce: 1 }),
    })
    .await;
    assert_eq!(
        p.ops.get(id).await.unwrap().unwrap().status,
        Status::Dispatched
    );
    // An id nobody knows is ignored too.
    d.handle_result(NodeResult {
        node_id: a.id,
        result: OperationResult::success(Uuid::now_v7(), Output::Pong { nonce: 1 }),
    })
    .await;

    // A's own answer counts, once.
    d.handle_result(NodeResult {
        node_id: a.id,
        result: OperationResult::success(id, Output::Pong { nonce: 1 }),
    })
    .await;
    assert_eq!(
        p.ops.get(id).await.unwrap().unwrap().status,
        Status::Succeeded
    );
    d.handle_result(NodeResult {
        node_id: a.id,
        result: OperationResult::failure(id, AgentError::new(ErrorCode::Internal, "late")),
    })
    .await;
    let row = p.ops.get(id).await.unwrap().unwrap();
    assert_eq!(row.status, Status::Succeeded);
    assert!(row.error.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn failed_and_retryable_results_follow_the_retry_policy(pool: PgPool) {
    let p = plane(&pool).await;
    let d = Dispatcher::new(p.ops.clone(), p.gateway.clone(), None, dispatch_settings());
    let n = make_node(&p, &pool, "node-a").await;
    let mut request = Request::new(n.id, Operation::Ping { nonce: 1 }, "k");
    request.max_attempts = 2;
    let id = p.ops.enqueue(&request).await.unwrap().operation().id;
    let report = |status_error: AgentError| NodeResult {
        node_id: n.id,
        result: OperationResult::failure(id, status_error),
    };

    p.ops.claim_due(&[n.id], 10).await.unwrap();
    d.handle_result(report(
        AgentError::new(ErrorCode::Internal, "disk busy").retryable(),
    ))
    .await;
    let row = p.ops.get(id).await.unwrap().unwrap();
    assert_eq!(row.status, Status::Retryable);
    assert_eq!(row.error.as_ref().unwrap()["message"], "disk busy");
    assert!(
        row.next_attempt_at > row.created_at,
        "waits before the next try"
    );

    // Second attempt fails retryably again, but attempts are used up.
    sqlx::query("UPDATE operations SET next_attempt_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    p.ops.claim_due(&[n.id], 10).await.unwrap();
    d.handle_result(report(
        AgentError::new(ErrorCode::Internal, "still busy").retryable(),
    ))
    .await;
    assert_eq!(p.ops.get(id).await.unwrap().unwrap().status, Status::Failed);

    // A permanent failure is final at once.
    let id2 = p
        .ops
        .enqueue(&Request::new(n.id, Operation::Ping { nonce: 2 }, "k2"))
        .await
        .unwrap()
        .operation()
        .id;
    p.ops.claim_due(&[n.id], 10).await.unwrap();
    d.handle_result(NodeResult {
        node_id: n.id,
        result: OperationResult {
            op_id: id2,
            status: ResultStatus::Failed,
            output: None,
            error: Some(AgentError::new(ErrorCode::InvalidArguments, "no")),
        },
    })
    .await;
    assert_eq!(
        p.ops.get(id2).await.unwrap().unwrap().status,
        Status::Failed
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn an_operation_a_node_never_answers_is_retried_and_then_fails_with_a_timeout(pool: PgPool) {
    let p = plane(&pool).await;
    p.start_dispatcher();
    let node = make_node(&p, &pool, "delta").await;
    // Connected, but silent about operations: a connection that is held open and never read.
    let params = node.params(p.addr);
    let config = stream::client_config(&params).unwrap();
    let tcp = tokio::net::TcpStream::connect(p.addr).await.unwrap();
    let tls = tokio_rustls::TlsConnector::from(config)
        .connect(
            rustls::pki_types::ServerName::try_from("127.0.0.1".to_owned()).unwrap(),
            tcp,
        )
        .await
        .unwrap();
    let (mut ws, _) = tokio_tungstenite::client_async(
        format!("wss://{}{}", p.addr, dhpanel_node_protocol::STREAM_PATH),
        tls,
    )
    .await
    .unwrap();
    use futures_util::SinkExt as _;
    let hello = dhpanel_node_protocol::encode(&dhpanel_node_protocol::AgentMessage::Hello(
        dhpanel_node_protocol::Hello {
            protocol_version: 1,
            agent_version: "mute".into(),
        },
    ))
    .unwrap();
    ws.send(tokio_tungstenite::tungstenite::Message::Text(hello.into()))
        .await
        .unwrap();
    wait_connected(&p, node.id).await;

    let mut request = Request::new(node.id, Operation::Ping { nonce: 1 }, "mute");
    request.max_attempts = 2;
    let id = p.ops.enqueue(&request).await.unwrap().operation().id;
    let row = wait_status(&p.ops, id, Status::Failed, Duration::from_secs(10)).await;
    assert_eq!(row.attempts, 2, "tried twice, never answered");
    assert_eq!(row.error.unwrap()["code"], "timeout");
    drop(ws);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn a_slow_operation_that_is_sent_again_still_runs_only_once(pool: PgPool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let p = plane(&pool).await;
    p.start_dispatcher();
    // A handler slower than the dispatcher's patience (1 s in these tests).
    let runs = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    let counter = runs.clone();
    registry.register("ping", move |op| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(2500)).await;
            match op {
                Operation::Ping { nonce } => Ok(Output::Pong { nonce }),
                _ => Err(dhpanel_node_agent::ops::wrong_operation()),
            }
        }
    });
    let node = make_node_with(&p, &pool, "slow", registry).await;
    let (agent_stop, agent) = node.start_agent(p.addr);
    wait_connected(&p, node.id).await;

    let id = p
        .ops
        .enqueue(&Request::new(
            node.id,
            Operation::Ping { nonce: 77 },
            "slow-ping",
        ))
        .await
        .unwrap()
        .operation()
        .id;
    // The control plane gives up waiting, sends it again, and the first run's
    // answer then completes the operation.
    let row = wait_status(&p.ops, id, Status::Succeeded, Duration::from_secs(10)).await;
    assert!(row.attempts >= 2, "it was sent again: {row:?}");
    assert_eq!(
        row.output,
        Some(serde_json::json!({"type": "pong", "nonce": 77}))
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(runs.load(Ordering::SeqCst), 1, "sent twice, run once");

    agent_stop.send(true).unwrap();
    agent.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn a_connected_node_is_asked_for_its_capabilities_and_they_are_stored(pool: PgPool) {
    let p = plane(&pool).await;
    p.start_dispatcher();
    let node = make_node(&p, &pool, "caps").await;
    let before = nodes::list_nodes(&pool, 10, 60).await.unwrap();
    assert_eq!(before[0].capabilities, serde_json::json!({}));
    assert!(before[0].capabilities_at.is_none());

    let (agent_stop, agent) = node.start_agent(p.addr);
    // Nobody asks by hand: connecting is enough.
    let mut stored = None;
    for _ in 0..100 {
        let listed = nodes::list_nodes(&pool, 10, 60).await.unwrap();
        if listed[0].capabilities_at.is_some() {
            stored = Some(listed[0].clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let stored = stored.expect("capabilities stored within 10 s");
    let caps = &stored.capabilities;
    assert_eq!(caps["agent_version"], dhpanel_node_agent::AGENT_VERSION);
    assert!(caps["cpu_cores"].as_u64().unwrap() >= 1);
    assert!(caps["memory_bytes"].as_u64().unwrap() > 0);
    assert!(
        caps["disk_total_bytes"].as_u64().unwrap() >= caps["disk_free_bytes"].as_u64().unwrap()
    );
    assert!(!caps["kernel"].as_str().unwrap().is_empty());
    assert!(caps["cgroup_v2"].is_boolean() && caps["seccomp"].is_boolean());

    // Asked once per time slot, however often the check runs.
    tokio::time::sleep(Duration::from_millis(900)).await;
    let asked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM operations WHERE node_id = $1 AND kind = 'report_capabilities'",
    )
    .bind(node.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(asked, 1);

    agent_stop.send(true).unwrap();
    agent.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn data_of_deleted_tenants_is_purged_when_due_and_the_tenant_goes_when_the_node_says_so(
    pool: PgPool,
) {
    let p = plane(&pool).await;
    // Not running: the test asks for purges itself, at the moment it wants to.
    let d = Dispatcher::new(p.ops.clone(), p.gateway.clone(), None, dispatch_settings());
    let node = make_node(&p, &pool, "purger").await;
    let account: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('client', 'zed', 'zed@example.com',
                 '$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let plan: Uuid = sqlx::query_scalar("SELECT id FROM plans WHERE is_default")
        .fetch_one(&pool)
        .await
        .unwrap();
    let tenant = |name: &'static str, uid: i32, days: i32| {
        let (pool, node_id) = (pool.clone(), node.id);
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (owner_account_id, node_id, plan_id, username, uid, status, deleted_at, purge_after)
                 VALUES ($1, $2, $3, $4, $5, 'deleting', now() - interval '10 days', now() + make_interval(days => $6)) RETURNING id",
            )
            .bind(account)
            .bind(node_id)
            .bind(plan)
            .bind(name)
            .bind(uid)
            .bind(days)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let due = tenant("duetenant", 10_000, -1).await;
    let later = tenant("latertenant", 10_001, 3).await;

    // Only the one whose time has come is asked for; asking again changes nothing.
    assert_eq!(d.request_purges().await, 1);
    assert_eq!(d.request_purges().await, 0);
    let rows: Vec<(Option<Uuid>, String, serde_json::Value)> = sqlx::query_as(
        "SELECT tenant_id, kind, payload FROM operations WHERE kind = 'delete_tenant'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, Some(due));
    assert_eq!(rows[0].2["purge_data"], true);
    assert_eq!(rows[0].2["username"], "duetenant");
    assert_ne!(rows[0].0, Some(later));

    // The node's success removes the tenant row; the other one stays.
    let op: Uuid = sqlx::query_scalar("SELECT id FROM operations WHERE kind = 'delete_tenant'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE operations SET status = 'dispatched', attempts = 1 WHERE id = $1")
        .bind(op)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        dhpanel_database::operations::succeed(&pool, op, &serde_json::json!({}))
            .await
            .unwrap()
    );
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenants ORDER BY username")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [later]);
    assert_eq!(d.request_purges().await, 0, "nothing more is due");
}
