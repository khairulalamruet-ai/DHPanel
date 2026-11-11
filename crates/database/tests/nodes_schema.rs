//! Schema tests for T02.04 (nodes, node_certificates).
//! Ignored by default; see tests/migrations.rs for how to run.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use sqlx::PgPool;
use sqlx::types::Uuid;

const FP_A: &str = "d8f74a584572d659b912df50cb16da8b49311804bef1d56e28272543505efc03";
const FP_B: &str = "fcd733218c7308de820fc91a9da0d8580e2d443538b821f22c79b671828cf946";
const SERIAL_A: &str = "4a1f0000000000000000000000000001";
const SERIAL_B: &str = "4a1f0000000000000000000000000002";

async fn insert_node(pool: &PgPool, name: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO nodes (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
}

/// Inserts a certificate valid from one hour ago for `days` days.
async fn insert_cert(
    pool: &PgPool,
    node: Uuid,
    serial: &str,
    fingerprint: &str,
    days: i32,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO node_certificates (node_id, serial, fingerprint_sha256, not_before, not_after)
         VALUES ($1, $2, $3, now() - interval '1 hour', now() + make_interval(days => $4))
         RETURNING id",
    )
    .bind(node)
    .bind(serial)
    .bind(fingerprint)
    .bind(days)
    .fetch_one(pool)
    .await
}

fn violated(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|e| e.constraint().map(str::to_owned))
        .unwrap_or_else(|| err.to_string())
}

/// The lookup the mTLS handshake will use (T02.08/T02.09): a connection is
/// accepted only for a certificate that is neither revoked nor expired and
/// whose node is not revoked.
async fn handshake_node(pool: &PgPool, fingerprint: &str) -> Option<Uuid> {
    sqlx::query_scalar(
        "SELECT n.id FROM node_certificates c JOIN nodes n ON n.id = c.node_id
         WHERE c.fingerprint_sha256 = $1 AND c.revoked_at IS NULL
           AND c.not_before <= now() AND c.not_after > now() AND n.status <> 'revoked'",
    )
    .bind(fingerprint)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn a_node_gets_uuidv7_and_safe_defaults(pool: PgPool) {
    let id = insert_node(&pool, "node-1").await.unwrap();
    let (version, status, capabilities, last_seen): (i32, String, String, Option<String>) =
        sqlx::query_as(
            "SELECT uuid_extract_version(id)::int4, status, capabilities::text, last_seen_at::text FROM nodes WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((version, status.as_str(), last_seen), (7, "offline", None));
    assert_eq!(capabilities, "{}");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn node_names_are_validated_and_unique(pool: PgPool) {
    for ok in [
        "ab",
        "node-1",
        "eu-west-2a",
        &format!("a{}", "b".repeat(62)),
    ] {
        insert_node(&pool, ok)
            .await
            .unwrap_or_else(|e| panic!("{ok}: {e}"));
    }
    for bad in [
        "",
        "a",
        "Node",
        "1node",
        "-node",
        "no_de",
        "no de",
        "node.1",
        &format!("a{}", "b".repeat(63)),
    ] {
        let err = insert_node(&pool, bad).await.expect_err(bad);
        assert_eq!(violated(&err), "nodes_name_format", "{bad:?}");
    }
    let err = insert_node(&pool, "node-1").await.unwrap_err();
    assert_eq!(violated(&err), "nodes_name_key");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn status_and_descriptive_columns_are_constrained(pool: PgPool) {
    let id = insert_node(&pool, "node-1").await.unwrap();
    for ok in ["online", "offline", "draining", "revoked"] {
        sqlx::query("UPDATE nodes SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(ok)
            .execute(&pool)
            .await
            .unwrap();
    }
    let err = sqlx::query("UPDATE nodes SET status = 'pending' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(violated(&err), "nodes_status");
    let long_host = "h".repeat(254);
    let err = sqlx::query("UPDATE nodes SET hostname = $2 WHERE id = $1")
        .bind(id)
        .bind(&long_host)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(violated(&err), "nodes_hostname_len");
    let err = sqlx::query("UPDATE nodes SET agent_version = '' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(violated(&err), "nodes_agent_version_len");
    sqlx::query("UPDATE nodes SET public_ip = '203.0.113.9', hostname = 'node1.example.com', agent_version = '0.1.0' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let err = sqlx::query("UPDATE nodes SET public_ip = 'not-an-ip' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("inet"), "{err}");
    // Capabilities: an object, and not unbounded.
    let err = sqlx::query("UPDATE nodes SET capabilities = '[1]' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(violated(&err), "nodes_capabilities_object");
    let huge = format!(r#"{{"big":"{}"}}"#, "x".repeat(17_000));
    let err = sqlx::query("UPDATE nodes SET capabilities = $2::jsonb WHERE id = $1")
        .bind(id)
        .bind(huge)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(violated(&err), "nodes_capabilities_size");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn updated_at_moves_on_update(pool: PgPool) {
    let id = insert_node(&pool, "node-1").await.unwrap();
    let before: String = sqlx::query_scalar("SELECT updated_at::text FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    sqlx::query("UPDATE nodes SET status = 'online' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let after: String = sqlx::query_scalar("SELECT updated_at::text FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(before, after);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn a_certificate_fingerprint_can_belong_to_only_one_certificate(pool: PgPool) {
    let a = insert_node(&pool, "node-a").await.unwrap();
    let b = insert_node(&pool, "node-b").await.unwrap();
    insert_cert(&pool, a, SERIAL_A, FP_A, 90).await.unwrap();

    // The same fingerprint again: refused, for the same node and for another one.
    for node in [a, b] {
        let err = insert_cert(&pool, node, SERIAL_B, FP_A, 90)
            .await
            .unwrap_err();
        assert_eq!(
            violated(&err),
            "node_certificates_fingerprint_key",
            "node {node}"
        );
    }
    // The same serial with another fingerprint: also refused (the CA never reuses a serial).
    let err = insert_cert(&pool, b, SERIAL_A, FP_B, 90).await.unwrap_err();
    assert_eq!(violated(&err), "node_certificates_serial_key");
    // Different serial and fingerprint are fine, also several for one node (rotation).
    insert_cert(&pool, a, SERIAL_B, FP_B, 90).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM node_certificates WHERE node_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn certificate_columns_are_validated(pool: PgPool) {
    let node = insert_node(&pool, "node-a").await.unwrap();
    for bad in [
        &FP_A.to_uppercase(),
        &FP_A[..63],
        &format!("{FP_A}0"),
        "zz",
        "",
    ] {
        let err = insert_cert(&pool, node, SERIAL_A, bad, 90)
            .await
            .expect_err(bad);
        assert_eq!(
            violated(&err),
            "node_certificates_fingerprint_format",
            "{bad:?}"
        );
    }
    for bad in [
        &SERIAL_A.to_uppercase(),
        &SERIAL_A[..31],
        &format!("{SERIAL_A}0"),
        "xyz",
    ] {
        let err = insert_cert(&pool, node, bad, FP_A, 90)
            .await
            .expect_err(bad);
        assert_eq!(violated(&err), "node_certificates_serial_format", "{bad:?}");
    }
    // The validity window must be positive.
    let err = sqlx::query(
        "INSERT INTO node_certificates (node_id, serial, fingerprint_sha256, not_before, not_after)
         VALUES ($1, $2, $3, now(), now())",
    )
    .bind(node)
    .bind(SERIAL_A)
    .bind(FP_A)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(violated(&err), "node_certificates_validity");
    // A certificate needs an existing node.
    let err = insert_cert(&pool, Uuid::now_v7(), SERIAL_A, FP_A, 90)
        .await
        .unwrap_err();
    assert!(
        err.as_database_error()
            .is_some_and(|e| e.is_foreign_key_violation()),
        "{err}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn revocation_always_records_why(pool: PgPool) {
    let node = insert_node(&pool, "node-a").await.unwrap();
    let cert = insert_cert(&pool, node, SERIAL_A, FP_A, 90).await.unwrap();
    for (sql, why) in [
        (
            "UPDATE node_certificates SET revoked_at = now() WHERE id = $1",
            "revoked without a reason",
        ),
        (
            "UPDATE node_certificates SET revoke_reason = 'admin' WHERE id = $1",
            "a reason without a revocation",
        ),
    ] {
        let err = sqlx::query(sql)
            .bind(cert)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            violated(&err),
            "node_certificates_revoked_has_reason",
            "{why}"
        );
    }
    let err = sqlx::query(
        "UPDATE node_certificates SET revoked_at = now(), revoke_reason = 'because' WHERE id = $1",
    )
    .bind(cert)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(violated(&err), "node_certificates_revoke_reason");
    for reason in ["admin", "rotated", "compromised", "node_revoked"] {
        sqlx::query(
            "UPDATE node_certificates SET revoked_at = now(), revoke_reason = $2 WHERE id = $1",
        )
        .bind(cert)
        .bind(reason)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("{reason}: {e}"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn only_usable_certificates_pass_the_handshake_lookup(pool: PgPool) {
    let node = insert_node(&pool, "node-a").await.unwrap();
    let other = insert_node(&pool, "node-b").await.unwrap();
    let cert = insert_cert(&pool, node, SERIAL_A, FP_A, 90).await.unwrap();
    assert_eq!(handshake_node(&pool, FP_A).await, Some(node));
    assert_eq!(
        handshake_node(&pool, FP_B).await,
        None,
        "unknown fingerprint"
    );

    // Rotation: the new certificate works, and so does the old one until it is revoked.
    insert_cert(&pool, node, SERIAL_B, FP_B, 90).await.unwrap();
    assert_eq!(handshake_node(&pool, FP_B).await, Some(node));
    assert_eq!(handshake_node(&pool, FP_A).await, Some(node));
    sqlx::query(
        "UPDATE node_certificates SET revoked_at = now(), revoke_reason = 'rotated' WHERE id = $1",
    )
    .bind(cert)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        handshake_node(&pool, FP_A).await,
        None,
        "revoked certificate"
    );
    assert_eq!(handshake_node(&pool, FP_B).await, Some(node));

    // Expired certificates stop working on their own.
    sqlx::query("UPDATE node_certificates SET not_before = now() - interval '2 days', not_after = now() - interval '1 day' WHERE fingerprint_sha256 = $1")
        .bind(FP_B)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        handshake_node(&pool, FP_B).await,
        None,
        "expired certificate"
    );

    // Revoking the node shuts out all its certificates, even valid ones.
    sqlx::query("UPDATE node_certificates SET not_after = now() + interval '1 day' WHERE fingerprint_sha256 = $1")
        .bind(FP_B)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(handshake_node(&pool, FP_B).await, Some(node));
    sqlx::query("UPDATE nodes SET status = 'revoked' WHERE id = $1")
        .bind(node)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(handshake_node(&pool, FP_B).await, None, "revoked node");
    assert_ne!(other, node);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn removing_a_node_removes_its_certificates(pool: PgPool) {
    let node = insert_node(&pool, "node-a").await.unwrap();
    insert_cert(&pool, node, SERIAL_A, FP_A, 90).await.unwrap();
    insert_cert(&pool, node, SERIAL_B, FP_B, 90).await.unwrap();
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(node)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM node_certificates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        handshake_node(&pool, FP_A).await,
        None,
        "a removed node cannot connect"
    );
}
