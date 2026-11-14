//! Schema tests for T07.02 (databases, database users and grants).
//! Ignored by default; see tests/migrations.rs for how to run.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use sqlx::{PgPool, types::Uuid};

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

async fn tenant(pool: &PgPool, name: &str, uid: i32) -> Uuid {
    let account: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('client', $1, $1 || '@example.com', $2) RETURNING id",
    )
    .bind(name)
    .bind(HASH)
    .fetch_one(pool)
    .await
    .unwrap();
    let node: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (name) VALUES ('node1') ON CONFLICT (name) DO UPDATE SET name = 'node1' RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let plan: Uuid = sqlx::query_scalar("SELECT id FROM plans WHERE is_default")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query_scalar(
        "INSERT INTO tenants (owner_account_id, node_id, plan_id, username, uid, status)
         VALUES ($1, $2, $3, $4, $5, 'active') RETURNING id",
    )
    .bind(account)
    .bind(node)
    .bind(plan)
    .bind(name)
    .bind(uid)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn names_are_plain_and_unique_and_a_grant_never_crosses_accounts(pool: PgPool) {
    let alice = tenant(&pool, "alice", 10_000).await;
    let bobby = tenant(&pool, "bobby", 10_001).await;
    let add_db = |t: Uuid, name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO databases (tenant_id, name) VALUES ($1, $2) RETURNING id",
            )
            .bind(t)
            .bind(name)
            .fetch_one(&pool)
            .await
        }
    };
    let add_user = |t: Uuid, name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO db_users (tenant_id, name) VALUES ($1, $2) RETURNING id",
            )
            .bind(t)
            .bind(name)
            .fetch_one(&pool)
            .await
        }
    };
    let shop = add_db(alice, "alice_shop").await.unwrap();
    let theirs = add_db(bobby, "bobby_blog").await.unwrap();
    let shopper = add_user(alice, "alice_web").await.unwrap();
    // A name is one account's, whoever asks.
    assert!(add_db(bobby, "alice_shop").await.is_err());
    assert!(add_user(bobby, "alice_web").await.is_err());
    // What cannot be an identifier does not get in, whatever the application did.
    for bad in [
        "",
        "Shop",
        "a b",
        "a;b",
        "a`b",
        "a-b",
        "x".repeat(65).leak() as &str,
    ] {
        assert!(add_db(alice, bad).await.is_err(), "{bad:?}");
        assert!(add_user(alice, bad).await.is_err(), "{bad:?}");
    }
    let grant = |db: Uuid, user: Uuid, tenant: Uuid, access: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO db_grants (tenant_id, database_id, user_id, access) VALUES ($1, $2, $3, $4)",
            )
            .bind(tenant)
            .bind(db)
            .bind(user)
            .bind(access)
            .execute(&pool)
            .await
        }
    };
    assert!(grant(shop, shopper, alice, "all").await.is_ok());
    assert!(
        grant(shop, shopper, alice, "all").await.is_err(),
        "one grant per pair"
    );
    // Another account's database, or user, cannot be put into a grant, with either account as the owner.
    for tenant in [alice, bobby] {
        assert!(grant(theirs, shopper, tenant, "read_only").await.is_err());
        assert!(
            grant(
                shop,
                add_user(bobby, "bobby_web").await.unwrap_or(shopper),
                tenant,
                "read_only"
            )
            .await
            .is_err()
        );
    }
    assert!(grant(shop, shopper, alice, "everything").await.is_err());
    // Removing the user or the database removes the grant; removing the account, all.
    sqlx::query("DELETE FROM db_users WHERE id = $1")
        .bind(shopper)
        .execute(&pool)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM db_grants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM databases WHERE tenant_id = $1")
        .bind(alice)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
