//! Schema tests for T01.01 (accounts, sessions, login_events).
//! Ignored by default; see tests/migrations.rs for how to run.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use sqlx::PgPool;
use sqlx::types::Uuid;

const HASH: &str = "$argon2id$v=19$m=65536,t=3,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

async fn insert_account(pool: &PgPool, username: &str, email: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash) VALUES ('client', $1, $2, $3) RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(HASH)
    .fetch_one(pool)
    .await
}

fn violated(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|e| e.constraint().map(str::to_owned))
        .unwrap_or_else(|| err.to_string())
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn valid_account_gets_uuidv7_and_defaults(pool: PgPool) {
    let id = insert_account(&pool, "alice", "alice@example.com")
        .await
        .expect("insert");
    assert_eq!(id.get_version_num(), 7);
    let (status, locale): (String, String) =
        sqlx::query_as("SELECT status::text, locale FROM accounts WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("select");
    assert_eq!((status.as_str(), locale.as_str()), ("active", "en"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn username_format_is_enforced(pool: PgPool) {
    for bad in [
        "Al",
        "ab",
        "1abc",
        "Alice",
        "has space",
        "a".repeat(33).as_str(),
    ] {
        let err = insert_account(&pool, bad, &format!("x{}@example.com", bad.len()))
            .await
            .expect_err(bad);
        assert_eq!(violated(&err), "accounts_username_format", "{bad}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn email_must_be_lowercase_and_wellformed(pool: PgPool) {
    for bad in ["Bob@Example.com", "no-at-sign", "a@b", "a b@example.com"] {
        let err = insert_account(&pool, "bob", bad).await.expect_err(bad);
        assert_eq!(violated(&err), "accounts_email_format", "{bad}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn username_and_email_are_unique(pool: PgPool) {
    insert_account(&pool, "carol", "carol@example.com")
        .await
        .expect("first");
    let err = insert_account(&pool, "carol", "other@example.com")
        .await
        .expect_err("dup username");
    assert_eq!(violated(&err), "accounts_username_key");
    let err = insert_account(&pool, "carol2", "carol@example.com")
        .await
        .expect_err("dup email");
    assert_eq!(violated(&err), "accounts_email_key");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn plain_text_password_is_rejected(pool: PgPool) {
    let err = sqlx::query(
        "INSERT INTO accounts (kind, username, email, password_hash) VALUES ('admin', 'dave', 'dave@example.com', 'hunter2')",
    )
    .execute(&pool)
    .await
    .expect_err("non-argon2id hash");
    assert_eq!(violated(&err), "accounts_password_hash_phc");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn updated_at_moves_on_update(pool: PgPool) {
    let id = insert_account(&pool, "erin", "erin@example.com")
        .await
        .expect("insert");
    // Commit the insert first so now() differs in the update transaction.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    sqlx::query("UPDATE accounts SET locale = 'bn' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("update");
    let moved: bool =
        sqlx::query_scalar("SELECT updated_at > created_at FROM accounts WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("select");
    assert!(moved);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn sessions_store_32_byte_hash_and_cascade(pool: PgPool) {
    let id = insert_account(&pool, "frank", "frank@example.com")
        .await
        .expect("insert");
    let insert = |hash: Vec<u8>| {
        sqlx::query(
            "INSERT INTO sessions (account_id, token_sha256, expires_at) VALUES ($1, $2, now() + interval '1 hour')",
        )
        .bind(id)
        .bind(hash)
        .execute(&pool)
    };
    let err = insert(vec![0u8; 16]).await.expect_err("short hash");
    assert_eq!(violated(&err), "sessions_token_sha256_len");
    insert(vec![7u8; 32]).await.expect("valid session");

    let err = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE account_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect_err("revocation needs a reason");
    assert_eq!(violated(&err), "sessions_revoked_has_reason");

    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("delete account");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 0, "sessions must cascade with the account");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL: set DATABASE_URL and pass --include-ignored"]
async fn login_events_record_unknown_users_and_keep_history(pool: PgPool) {
    sqlx::query(
        "INSERT INTO login_events (username_attempted, ip, success, reason) VALUES ('ghost', '203.0.113.9', false, 'bad_credentials')",
    )
    .execute(&pool)
    .await
    .expect("unknown user attempt is recorded");

    let err = sqlx::query(
        "INSERT INTO login_events (username_attempted, ip, success, reason) VALUES ('x', '203.0.113.9', true, 'bad_credentials')",
    )
    .execute(&pool)
    .await
    .expect_err("success must match reason");
    assert_eq!(violated(&err), "login_events_success_reason");

    let id = insert_account(&pool, "grace", "grace@example.com")
        .await
        .expect("insert");
    sqlx::query(
        "INSERT INTO login_events (account_id, username_attempted, ip, success, reason) VALUES ($1, 'grace', '198.51.100.7', true, 'ok')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("success event");
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("delete");
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM login_events WHERE username_attempted = 'grace' AND account_id IS NULL")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(kept, 1, "login history survives account deletion");
}
