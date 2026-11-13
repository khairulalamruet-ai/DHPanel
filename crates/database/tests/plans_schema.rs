//! Schema tests for T04.01 (feature_sets, plans).
//! Ignored by default; see tests/migrations.rs for how to run.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use sqlx::PgPool;

async fn fails(pool: &PgPool, sql: &str) -> bool {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .is_err()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn the_four_presets_are_there_and_linked(pool: PgPool) {
    let sets: Vec<(String, bool, Vec<String>, bool)> = sqlx::query_as(
        "SELECT name, all_features, features::text[], built_in FROM feature_sets ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = sets.iter().map(|s| s.0.as_str()).collect();
    assert_eq!(names, ["basic", "db_only", "default", "dns_only"]);
    assert!(sets.iter().all(|s| s.3), "presets are built in");
    let default = sets.iter().find(|s| s.0 == "default").unwrap();
    assert!(
        default.1 && default.2.is_empty(),
        "default means everything"
    );
    assert_eq!(sets.iter().find(|s| s.0 == "dns_only").unwrap().2, ["dns"]);
    assert_eq!(
        sets.iter().find(|s| s.0 == "db_only").unwrap().2,
        ["databases"]
    );
    assert!(
        sets.iter()
            .find(|s| s.0 == "basic")
            .unwrap()
            .2
            .contains(&"domains".to_owned())
    );

    let plans: Vec<(String, String, Option<i32>, Option<i64>, serde_json::Value)> = sqlx::query_as(
        "SELECT p.name, f.name, p.cpu_millis, p.bandwidth_bytes, p.limits
         FROM plans p JOIN feature_sets f ON f.id = p.feature_set_id ORDER BY p.name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(plans.len(), 4);
    for (plan, set, ..) in &plans {
        assert_eq!(
            plan, set,
            "each preset plan uses the feature set of the same name"
        );
    }
    let default = plans.iter().find(|p| p.0 == "default").unwrap();
    assert_eq!(
        (default.2, default.3),
        (Some(2000), None),
        "unlimited bandwidth is NULL"
    );
    assert_eq!(default.4["max_domains"], -1);
    let dns = plans.iter().find(|p| p.0 == "dns_only").unwrap();
    assert_eq!(dns.2, None, "a plan without containers has no CPU share");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn feature_sets_refuse_bad_names_keys_and_contradictions(pool: PgPool) {
    for sql in [
        "INSERT INTO feature_sets (name) VALUES ('Bad Name')",
        "INSERT INTO feature_sets (name) VALUES ('x')",
        "INSERT INTO feature_sets (name) VALUES ('default')",
        "INSERT INTO feature_sets (name, features) VALUES ('f1', ARRAY['Has Space']::feature_key[])",
        "INSERT INTO feature_sets (name, features) VALUES ('f2', ARRAY['ok', 'bad,key']::feature_key[])",
        "INSERT INTO feature_sets (name, features) VALUES ('f3', ARRAY['1abc']::feature_key[])",
        "INSERT INTO feature_sets (name, all_features, features) VALUES ('f4', true, ARRAY['dns']::feature_key[])",
        "INSERT INTO feature_sets (name, description) VALUES ('f5', repeat('x', 256))",
    ] {
        assert!(fails(&pool, sql).await, "{sql}");
    }
    assert!(!fails(&pool, "INSERT INTO feature_sets (name, features) VALUES ('good', ARRAY['dns', 'ssl']::feature_key[])").await);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL"]
async fn plans_refuse_nonsense_and_protect_their_feature_set(pool: PgPool) {
    let set: sqlx::types::Uuid =
        sqlx::query_scalar("SELECT id FROM feature_sets WHERE name = 'basic'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let insert = |extra_columns: &str, extra_values: &str| {
        format!(
            "INSERT INTO plans (name, feature_set_id{extra_columns}) VALUES ('p{}', '{set}'{extra_values})",
            rand_suffix()
        )
    };
    assert!(
        !fails(&pool, &insert("", "")).await,
        "a plan with only the required columns"
    );
    for (cols, vals) in [
        (", cpu_millis", ", 5"),
        (", cpu_millis", ", 999999"),
        (", memory_bytes", ", 1024"),
        (", pids", ", 0"),
        (", disk_bytes", ", 100"),
        (", inodes", ", 5"),
        (", bandwidth_bytes", ", -1"),
        (", limits", ", '[]'"),
        (", limits", ", '\"text\"'"),
        (", isolation", ", 'none'"),
    ] {
        assert!(fails(&pool, &insert(cols, vals)).await, "{cols} {vals}");
    }
    assert!(
        fails(
            &pool,
            "INSERT INTO plans (name, feature_set_id) VALUES ('Bad', gen_random_uuid())"
        )
        .await,
        "unknown feature set"
    );
    assert!(
        fails(
            &pool,
            &format!("INSERT INTO plans (name, feature_set_id) VALUES ('basic', '{set}')")
        )
        .await,
        "duplicate name"
    );

    // A feature set that a plan uses cannot be deleted.
    assert!(fails(&pool, "DELETE FROM feature_sets WHERE name = 'basic'").await);
    // One that no plan uses can.
    sqlx::query("INSERT INTO feature_sets (name) VALUES ('unused')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM feature_sets WHERE name = 'unused'")
        .execute(&pool)
        .await
        .unwrap();
}

fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    N.fetch_add(1, Ordering::SeqCst).to_string()
}
