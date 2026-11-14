//! The certificate a domain is served with (T05.16).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/admin/domains/{id}/ssl` | `admin.domains.read` |
//! | GET | `/api/v1/client/domains/{id}/ssl` | the owner of the domain |
//!
//! The nodes report the certificates their web server holds; the control plane
//! keeps the latest per domain: who issued it and when it is valid. A domain
//! that has none yet (the first visit to it makes the web server fetch one) says
//! `none`. A certificate that ends within 14 days is `expiring`, one that has
//! ended `expired`. Nothing secret is stored or shown.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use dhpanel_database::certificates;
use dhpanel_http_kit::{AppError, ProblemDetails};
use dhpanel_rbac::{Principal, Require, permissions::DomainsRead};
use dhpanel_session::rfc3339;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{DomainsState, Hosting, client_hosting, internal, owned_domain, tenant_of_domain};

/// How many days before its end a certificate counts as expiring.
pub const EXPIRING_DAYS: i64 = 14;

/// Routes for administrators.
pub fn admin_router() -> Router<DomainsState> {
    Router::new().route("/api/v1/admin/domains/{id}/ssl", get(admin_ssl))
}

/// Routes for clients.
pub fn client_router() -> Router<DomainsState> {
    Router::new().route("/api/v1/client/domains/{id}/ssl", get(client_ssl))
}

/// A domain's certificate.
#[derive(Debug, Serialize, ToSchema)]
pub struct SslView {
    /// `none` (no certificate yet), `valid`, `expiring` (ends within 14 days) or `expired`.
    #[schema(example = "valid")]
    pub state: String,
    /// `acme` (obtained by the web server) or `custom` (uploaded).
    pub kind: Option<String>,
    /// Who issued it.
    #[schema(example = "R11")]
    pub issuer: Option<String>,
    /// Valid from (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub not_before: Option<String>,
    /// Valid until (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub not_after: Option<String>,
    /// Whole days until it ends (negative once it has).
    pub days_left: Option<i64>,
    /// When a node last reported it (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub last_seen_at: Option<String>,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// How a certificate that ends at `not_after` stands at `at`.
#[must_use]
pub fn state_of(not_after: i64, at: i64) -> (&'static str, i64) {
    // Floor division: a certificate with 20 hours left has 0 days left, one that
    // ended an hour ago has -1.
    let days_left = (not_after - at).div_euclid(86_400);
    let state = if not_after <= at {
        "expired"
    } else if days_left < EXPIRING_DAYS {
        "expiring"
    } else {
        "valid"
    };
    (state, days_left)
}

async fn view(
    state: &DomainsState,
    hosting: &Hosting,
    domain_id: Uuid,
) -> Result<Json<SslView>, AppError> {
    owned_domain(state, hosting.tenant.id, domain_id).await?;
    let Some(cert) = certificates::of_domain(&state.db, domain_id)
        .await
        .map_err(internal)?
    else {
        return Ok(Json(SslView {
            state: "none".to_owned(),
            kind: None,
            issuer: None,
            not_before: None,
            not_after: None,
            days_left: None,
            last_seen_at: None,
        }));
    };
    let (label, days_left) = state_of(cert.not_after, now());
    Ok(Json(SslView {
        state: label.to_owned(),
        kind: Some(cert.kind),
        issuer: Some(cert.issuer),
        not_before: Some(rfc3339(cert.not_before)),
        not_after: Some(rfc3339(cert.not_after)),
        days_left: Some(days_left),
        last_seen_at: Some(rfc3339(cert.last_seen_at)),
    }))
}

/// The certificate of a domain.
#[utoipa::path(
    get,
    path = "/api/v1/admin/domains/{id}/ssl",
    tag = "domains",
    operation_id = "adminDomainSsl",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "Domain id")),
    responses(
        (status = 200, description = "The certificate, or `none`", body = SslView),
        (status = 403, description = "`permission_denied`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such domain", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn admin_ssl(
    State(state): State<DomainsState>,
    _who: Require<DomainsRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<SslView>, AppError> {
    let h = tenant_of_domain(&state, id).await?;
    view(&state, &h, id).await
}

/// The certificate of one of your domains.
#[utoipa::path(
    get,
    path = "/api/v1/client/domains/{id}/ssl",
    tag = "domains",
    operation_id = "clientDomainSsl",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "Domain id")),
    responses(
        (status = 200, description = "The certificate, or `none`", body = SslView),
        (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such domain (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn client_ssl(
    State(state): State<DomainsState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<SslView>, AppError> {
    let h = client_hosting(&state, &who).await?;
    view(&state, &h, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_is_valid_expiring_or_expired() {
        let at = 1_000_000_000;
        let day = 86_400;
        assert_eq!(state_of(at + 60 * day, at), ("valid", 60));
        assert_eq!(
            state_of(at + 14 * day, at),
            ("valid", 14),
            "exactly 14 days is not yet expiring"
        );
        assert_eq!(state_of(at + 14 * day - 1, at), ("expiring", 13));
        assert_eq!(state_of(at + 20 * 3600, at), ("expiring", 0));
        assert_eq!(state_of(at, at), ("expired", 0));
        assert_eq!(state_of(at - 3600, at), ("expired", -1));
        assert_eq!(state_of(at - 3 * day, at), ("expired", -3));
    }
}
