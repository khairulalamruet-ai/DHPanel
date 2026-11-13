//! The `feature_sets` table.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A feature set with how many plans use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSet {
    /// Id.
    pub id: Uuid,
    /// Unique name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Every feature of every module.
    pub all_features: bool,
    /// The feature keys (empty when `all_features`).
    pub features: Vec<String>,
    /// Created by the installation.
    pub built_in: bool,
    /// Plans using it.
    pub plans: i64,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Last change (Unix seconds).
    pub updated_at: i64,
}

/// Why a write failed.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Another feature set has this name.
    #[error("a feature set with this name already exists")]
    NameTaken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    name: String,
    description: String,
    all_features: bool,
    features: Vec<String>,
    built_in: bool,
    plans: i64,
    created_at: i64,
    updated_at: i64,
}

impl From<Row> for FeatureSet {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            all_features: r.all_features,
            features: r.features,
            built_in: r.built_in,
            plans: r.plans,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// All feature sets, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<FeatureSet>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT f.id, f.name, f.description, f.all_features,
                  f.features::text[] AS "features!", f.built_in,
                  (SELECT count(*) FROM plans p WHERE p.feature_set_id = f.id) AS "plans!",
                  extract(epoch FROM f.created_at)::bigint AS "created_at!",
                  extract(epoch FROM f.updated_at)::bigint AS "updated_at!"
           FROM feature_sets f ORDER BY f.name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One feature set.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<FeatureSet>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT f.id, f.name, f.description, f.all_features,
                  f.features::text[] AS "features!", f.built_in,
                  (SELECT count(*) FROM plans p WHERE p.feature_set_id = f.id) AS "plans!",
                  extract(epoch FROM f.created_at)::bigint AS "created_at!",
                  extract(epoch FROM f.updated_at)::bigint AS "updated_at!"
           FROM feature_sets f WHERE f.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

fn name_taken(err: &sqlx::Error) -> bool {
    err.as_database_error().and_then(|e| e.constraint()) == Some("feature_sets_name_key")
}

/// Creates a feature set and returns its id.
///
/// # Errors
/// [`WriteError::NameTaken`] or database errors.
pub async fn insert(
    conn: &mut PgConnection,
    name: &str,
    description: &str,
    all_features: bool,
    features: &[String],
) -> Result<Uuid, WriteError> {
    sqlx::query_scalar!(
        r#"INSERT INTO feature_sets (name, description, all_features, features)
           VALUES ($1, $2, $3, $4::text[]::feature_key[]) RETURNING id"#,
        name,
        description,
        all_features,
        features,
    )
    .fetch_one(conn)
    .await
    .map_err(|e| {
        if name_taken(&e) {
            WriteError::NameTaken
        } else {
            e.into()
        }
    })
}

/// Replaces a feature set's description, mode and feature list (and its name,
/// unless it is built in). Returns whether it exists.
///
/// # Errors
/// [`WriteError::NameTaken`] or database errors.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    description: &str,
    all_features: bool,
    features: &[String],
) -> Result<bool, WriteError> {
    let done = sqlx::query!(
        r#"UPDATE feature_sets
           SET name = CASE WHEN built_in THEN name ELSE $2 END,
               description = $3, all_features = $4, features = $5::text[]::feature_key[]
           WHERE id = $1"#,
        id,
        name,
        description,
        all_features,
        features,
    )
    .execute(conn)
    .await
    .map_err(|e| {
        if name_taken(&e) {
            WriteError::NameTaken
        } else {
            e.into()
        }
    })?;
    Ok(done.rows_affected() == 1)
}

/// What [`delete`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// Deleted.
    Deleted,
    /// No such feature set.
    Unknown,
    /// Created by the installation.
    BuiltIn,
    /// Plans still use it.
    InUse,
}

/// Deletes a feature set that is not built in and not used by any plan.
///
/// # Errors
/// Database errors.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<DeleteOutcome, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT built_in, EXISTS (SELECT 1 FROM plans WHERE feature_set_id = $1) AS "in_use!"
           FROM feature_sets WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        None => Ok(DeleteOutcome::Unknown),
        Some(r) if r.built_in => Ok(DeleteOutcome::BuiltIn),
        Some(r) if r.in_use => Ok(DeleteOutcome::InUse),
        Some(_) => {
            sqlx::query!("DELETE FROM feature_sets WHERE id = $1", id)
                .execute(conn)
                .await?;
            Ok(DeleteOutcome::Deleted)
        }
    }
}
