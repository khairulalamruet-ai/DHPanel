//! Web server configuration templates (T06.03).

use sqlx::{PgPool, types::Uuid};

/// A template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VhostTemplate {
    /// Id.
    pub id: Uuid,
    /// The web server it is for (`nginx`).
    pub server: String,
    /// Its name.
    pub name: String,
    /// The configuration with `{{variables}}`.
    pub body: String,
    /// Whether new sites use it.
    pub is_default: bool,
}

/// The template new sites of `server` use.
///
/// # Errors
/// Database errors.
pub async fn default_for(
    pool: &PgPool,
    server: &str,
) -> Result<Option<VhostTemplate>, sqlx::Error> {
    sqlx::query_as!(
        VhostTemplate,
        "SELECT id, server, name, body, is_default FROM vhost_templates
         WHERE server = $1 AND is_default",
        server,
    )
    .fetch_optional(pool)
    .await
}
