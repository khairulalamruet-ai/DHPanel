//! Notifications (T18.03): the inbox, the channels people set up, the rules that send an event
//! to a channel, what is still to be delivered, and the panel's mail server.

use serde_json::Value;
use sqlx::{PgConnection, PgPool, types::Uuid};

/// A notification to put in an inbox.
#[derive(Debug, Clone)]
pub struct NewNotification<'a> {
    /// Whose inbox.
    pub account_id: Uuid,
    /// The event it comes from (`backup_failed`, ...).
    pub event_type: &'a str,
    /// `info`, `warning` or `critical`.
    pub severity: &'a str,
    /// One line.
    pub subject: &'a str,
    /// The text.
    pub body: &'a str,
    /// The ids it is about.
    pub data: &'a Value,
    /// Names the event, so that the same one is not turned into a second notification.
    pub dedupe_key: &'a str,
}

/// Puts a notification in an inbox; `None` when the same one is there already.
///
/// # Errors
/// Database errors.
pub async fn add(
    conn: &mut PgConnection,
    n: &NewNotification<'_>,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO notifications (account_id, event_type, severity, subject, body, data, dedupe_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (account_id, dedupe_key) DO NOTHING
         RETURNING id",
        n.account_id,
        n.event_type,
        n.severity,
        n.subject,
        n.body,
        n.data,
        n.dedupe_key,
    )
    .fetch_optional(conn)
    .await
}

/// A notification as the inbox shows it.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Id.
    pub id: Uuid,
    /// The event it comes from.
    pub event_type: String,
    /// `info`, `warning` or `critical`.
    pub severity: String,
    /// One line.
    pub subject: String,
    /// The text.
    pub body: String,
    /// The ids it is about.
    pub data: Value,
    /// When (Unix seconds).
    pub created_at: i64,
    /// When it was read (Unix seconds), if it was.
    pub read_at: Option<i64>,
}

/// The newest notifications of an inbox.
///
/// # Errors
/// Database errors.
pub async fn list(
    pool: &PgPool,
    account_id: Uuid,
    unread_only: bool,
    limit: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    sqlx::query_as!(
        Notification,
        r#"SELECT id, event_type, severity, subject, body, data,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM read_at)::bigint AS read_at
           FROM notifications
           WHERE account_id = $1 AND (NOT $2 OR read_at IS NULL)
           ORDER BY created_at DESC, id DESC LIMIT $3"#,
        account_id,
        unread_only,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// How many notifications of an inbox are not read.
///
/// # Errors
/// Database errors.
pub async fn unread_count(pool: &PgPool, account_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM notifications WHERE account_id = $1 AND read_at IS NULL"#,
        account_id
    )
    .fetch_one(pool)
    .await
}

/// Marks some notifications of an inbox as read (or, with no ids, all of them). Returns how many
/// changed.
///
/// # Errors
/// Database errors.
pub async fn mark_read(
    pool: &PgPool,
    account_id: Uuid,
    ids: Option<&[Uuid]>,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE notifications SET read_at = now()
         WHERE account_id = $1 AND read_at IS NULL AND ($2::uuid[] IS NULL OR id = ANY($2))",
        account_id,
        ids,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Pause and snooze of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Quiet {
    /// Nothing goes out of the inbox until this is turned off.
    pub paused: bool,
    /// Nothing goes out of the inbox until this time (Unix seconds).
    pub snoozed_until: Option<i64>,
}

/// An account's pause and snooze.
///
/// # Errors
/// Database errors.
pub async fn quiet(pool: &PgPool, account_id: Uuid) -> Result<Quiet, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT paused, extract(epoch FROM snoozed_until)::bigint AS snoozed_until
           FROM notification_settings WHERE account_id = $1"#,
        account_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or_else(Quiet::default, |r| Quiet {
        paused: r.paused,
        snoozed_until: r.snoozed_until,
    }))
}

/// Sets an account's pause and snooze.
///
/// # Errors
/// Database errors.
pub async fn set_quiet(pool: &PgPool, account_id: Uuid, quiet: Quiet) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO notification_settings (account_id, paused, snoozed_until)
         VALUES ($1, $2, to_timestamp($3::bigint))
         ON CONFLICT (account_id) DO UPDATE
             SET paused = EXCLUDED.paused, snoozed_until = EXCLUDED.snoozed_until, updated_at = now()",
        account_id,
        quiet.paused,
        quiet.snoozed_until,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether nothing is to go out of the inbox of this account just now.
///
/// # Errors
/// Database errors.
pub async fn is_quiet(conn: &mut PgConnection, account_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(paused OR COALESCE(snoozed_until > now(), false), false) AS "quiet!"
           FROM notification_settings WHERE account_id = $1"#,
        account_id
    )
    .fetch_optional(conn)
    .await
    .map(Option::unwrap_or_default)
}

/// A channel as it is shown.
#[derive(Debug, Clone)]
pub struct Channel {
    /// Id.
    pub id: Uuid,
    /// `email` or `webhook`.
    pub kind: String,
    /// What the owner called it.
    pub name: String,
    /// The address or the URL.
    pub target: String,
    /// Whether it is used.
    pub enabled: bool,
    /// The events that go to it.
    pub events: Vec<String>,
}

/// An account's channels.
///
/// # Errors
/// Database errors.
pub async fn channels(pool: &PgPool, account_id: Uuid) -> Result<Vec<Channel>, sqlx::Error> {
    sqlx::query_as!(
        Channel,
        r#"SELECT c.id, c.kind, c.name, c.target, c.enabled,
                  COALESCE(array_agg(r.event_type ORDER BY r.event_type) FILTER (WHERE r.event_type IS NOT NULL), '{}') AS "events!"
           FROM notification_channels c
           LEFT JOIN notification_rules r ON r.channel_id = c.id
           WHERE c.account_id = $1
           GROUP BY c.id ORDER BY c.created_at, c.id"#,
        account_id
    )
    .fetch_all(pool)
    .await
}

/// Why a channel could not be made.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// The account has a channel of that name.
    #[error("a channel of that name exists")]
    Duplicate,
    /// The account has as many channels as it may.
    #[error("too many channels")]
    TooMany,
    /// A database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// How many channels an account may have.
pub const MAX_CHANNELS: i64 = 10;

/// Makes a channel.
///
/// # Errors
/// [`ChannelError`].
pub async fn add_channel(
    pool: &PgPool,
    account_id: Uuid,
    kind: &str,
    name: &str,
    target: &str,
    sealed_secret: Option<&[u8]>,
) -> Result<Uuid, ChannelError> {
    let mut tx = pool.begin().await?;
    // One at a time per account, so that the count and the insert agree.
    sqlx::query!(
        "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
        account_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let have = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM notification_channels WHERE account_id = $1"#,
        account_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if have >= MAX_CHANNELS {
        return Err(ChannelError::TooMany);
    }
    let id = sqlx::query_scalar!(
        "INSERT INTO notification_channels (account_id, kind, name, target, sealed_secret)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
        account_id,
        kind,
        name,
        target,
        sealed_secret,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(
        |e| match e.as_database_error().and_then(|d| d.constraint()) {
            Some("notification_channels_account_id_name_key") => ChannelError::Duplicate,
            _ => ChannelError::Database(e),
        },
    )?;
    tx.commit().await?;
    Ok(id)
}

/// Removes a channel of the account. Returns whether there was one.
///
/// # Errors
/// Database errors.
pub async fn delete_channel(
    pool: &PgPool,
    account_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM notification_channels WHERE id = $1 AND account_id = $2",
        id,
        account_id
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Switches a channel on or off and sets the events that go to it (when given). Returns whether the
/// account has such a channel.
///
/// # Errors
/// Database errors.
pub async fn update_channel(
    pool: &PgPool,
    account_id: Uuid,
    id: Uuid,
    enabled: Option<bool>,
    events: Option<&[String]>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let found = sqlx::query!(
        "UPDATE notification_channels SET enabled = COALESCE($3, enabled)
         WHERE id = $1 AND account_id = $2",
        id,
        account_id,
        enabled,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if found && let Some(events) = events {
        sqlx::query!("DELETE FROM notification_rules WHERE channel_id = $1", id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!(
            "INSERT INTO notification_rules (channel_id, event_type)
             SELECT $1, e FROM unnest($2::text[]) AS e ON CONFLICT DO NOTHING",
            id,
            events,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(found)
}

/// The enabled channels of an account that want this event.
///
/// # Errors
/// Database errors.
pub async fn channels_for(
    conn: &mut PgConnection,
    account_id: Uuid,
    event_type: &str,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT c.id FROM notification_channels c
         JOIN notification_rules r ON r.channel_id = c.id
         WHERE c.account_id = $1 AND c.enabled AND r.event_type = $2",
        account_id,
        event_type,
    )
    .fetch_all(conn)
    .await
}

/// Queues a notification for a channel (once).
///
/// # Errors
/// Database errors.
pub async fn enqueue(
    conn: &mut PgConnection,
    notification_id: Uuid,
    channel_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO notification_deliveries (notification_id, channel_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
        notification_id,
        channel_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// One delivery that is due, with all it needs.
#[derive(Debug, Clone)]
pub struct Due {
    /// The delivery.
    pub id: Uuid,
    /// How many tries there were.
    pub attempts: i32,
    /// The notification.
    pub notification_id: Uuid,
    /// The event it comes from.
    pub event_type: String,
    /// `info`, `warning` or `critical`.
    pub severity: String,
    /// One line.
    pub subject: String,
    /// The text.
    pub body: String,
    /// The ids it is about.
    pub data: Value,
    /// When it was made (Unix seconds).
    pub created_at: i64,
    /// `email` or `webhook`.
    pub kind: String,
    /// The address or the URL.
    pub target: String,
    /// A webhook's sealed signing secret.
    pub sealed_secret: Option<Vec<u8>>,
    /// The owner's language (`en` or `bn`).
    pub locale: String,
    /// The owner (a webhook's secret is bound to it).
    pub account_id: Uuid,
}

/// How long a delivery that was taken waits before it may be taken again (its worker may be gone).
pub const CLAIM_SECS: i32 = 120;

/// Takes the deliveries that are due (and pushes their next try [`CLAIM_SECS`] away, so that two
/// workers never send the same one).
///
/// # Errors
/// Database errors.
pub async fn claim_due(pool: &PgPool, limit: i64) -> Result<Vec<Due>, sqlx::Error> {
    sqlx::query_as!(
        Due,
        r#"WITH due AS (
               SELECT id FROM notification_deliveries
               WHERE status = 'pending' AND next_attempt_at <= now()
               ORDER BY next_attempt_at, id LIMIT $1
               FOR UPDATE SKIP LOCKED
           ), taken AS (
               UPDATE notification_deliveries d
               SET attempts = d.attempts + 1, next_attempt_at = now() + make_interval(secs => $2::int)
               FROM due WHERE d.id = due.id
               RETURNING d.id, d.attempts, d.notification_id, d.channel_id
           )
           SELECT t.id AS "id!", t.attempts AS "attempts!", n.id AS "notification_id!", n.event_type AS "event_type!",
                  n.severity AS "severity!", n.subject AS "subject!", n.body AS "body!", n.data AS "data!",
                  extract(epoch FROM n.created_at)::bigint AS "created_at!",
                  c.kind AS "kind!", c.target AS "target!", c.sealed_secret AS sealed_secret,
                  a.locale AS "locale!", a.id AS "account_id!"
           FROM taken t
           JOIN notifications n ON n.id = t.notification_id
           JOIN notification_channels c ON c.id = t.channel_id
           JOIN accounts a ON a.id = c.account_id
           ORDER BY t.id"#,
        limit,
        CLAIM_SECS,
    )
    .fetch_all(pool)
    .await
}

/// The delivery went out.
///
/// # Errors
/// Database errors.
pub async fn mark_sent(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE notification_deliveries SET status = 'sent', sent_at = now(), last_error = NULL WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The delivery did not go out: it is tried again after `retry_in_secs`, until it has been tried
/// `max_attempts` times, then it is `failed` for good.
///
/// # Errors
/// Database errors.
pub async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    error: &str,
    retry_in_secs: i32,
    max_attempts: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE notification_deliveries
         SET last_error = left($2, 500),
             status = CASE WHEN attempts >= $4 THEN 'failed' ELSE 'pending' END,
             next_attempt_at = now() + make_interval(secs => $3::int)
         WHERE id = $1",
        id,
        error,
        retry_in_secs,
        max_attempts,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The panel's mail server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smtp {
    /// Host name.
    pub host: String,
    /// Port.
    pub port: i32,
    /// `starttls`, `tls` or `none`.
    pub security: String,
    /// The login, if any.
    pub username: Option<String>,
    /// The password, sealed with the master key.
    pub sealed_password: Option<Vec<u8>>,
    /// The address mail comes from.
    pub from_address: String,
    /// The name mail comes from.
    pub from_name: Option<String>,
    /// Whether mail is sent.
    pub enabled: bool,
}

/// The mail server, if one was set.
///
/// # Errors
/// Database errors.
pub async fn smtp(pool: &PgPool) -> Result<Option<Smtp>, sqlx::Error> {
    sqlx::query_as!(
        Smtp,
        "SELECT host, port, security, username, sealed_password, from_address, from_name, enabled
         FROM notification_smtp WHERE id"
    )
    .fetch_optional(pool)
    .await
}

/// Sets the mail server. A password of `None` keeps the one there is.
///
/// # Errors
/// Database errors.
pub async fn set_smtp(pool: &PgPool, s: &Smtp) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO notification_smtp (id, host, port, security, username, sealed_password, from_address, from_name, enabled)
         VALUES (true, $1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET
             host = EXCLUDED.host, port = EXCLUDED.port, security = EXCLUDED.security,
             username = EXCLUDED.username,
             sealed_password = COALESCE(EXCLUDED.sealed_password, notification_smtp.sealed_password),
             from_address = EXCLUDED.from_address, from_name = EXCLUDED.from_name,
             enabled = EXCLUDED.enabled, updated_at = now()",
        s.host,
        s.port,
        s.security,
        s.username,
        s.sealed_password,
        s.from_address,
        s.from_name,
        s.enabled,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A person an event concerns.
#[derive(Debug, Clone)]
pub struct Recipient {
    /// The account.
    pub account_id: Uuid,
    /// `en` or `bn`.
    pub locale: String,
}

/// The administrators' accounts.
///
/// # Errors
/// Database errors.
pub async fn admins(pool: &PgPool) -> Result<Vec<Recipient>, sqlx::Error> {
    sqlx::query_as!(
        Recipient,
        "SELECT id AS account_id, locale FROM accounts WHERE kind = 'admin' AND status = 'active' ORDER BY id"
    )
    .fetch_all(pool)
    .await
}

/// The account that owns a hosting account.
///
/// # Errors
/// Database errors.
pub async fn tenant_owner(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Option<Recipient>, sqlx::Error> {
    sqlx::query_as!(
        Recipient,
        "SELECT a.id AS account_id, a.locale FROM tenants t JOIN accounts a ON a.id = t.owner_account_id
         WHERE t.id = $1 AND a.status = 'active'",
        tenant_id
    )
    .fetch_optional(pool)
    .await
}

/// A node's name.
///
/// # Errors
/// Database errors.
pub async fn node_name(pool: &PgPool, node_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT name FROM nodes WHERE id = $1", node_id)
        .fetch_optional(pool)
        .await
}

/// Removes what is old: read notifications after `keep_days`, finished deliveries after a month.
///
/// # Errors
/// Database errors.
pub async fn prune(pool: &PgPool, keep_days: i32) -> Result<u64, sqlx::Error> {
    let a = sqlx::query!(
        "DELETE FROM notifications WHERE created_at < now() - make_interval(days => $1::int)",
        keep_days
    )
    .execute(pool)
    .await?
    .rows_affected();
    let b = sqlx::query!(
        "DELETE FROM notification_deliveries WHERE status <> 'pending' AND created_at < now() - interval '30 days'"
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(a + b)
}
