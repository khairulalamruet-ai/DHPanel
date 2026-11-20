//! The software marketplace (server software in a customer's account: databases, message queues,
//! Java tools, ...), on top of the containers of [`crate::containers`].
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/marketplace` | the owner; the `runtime_apps` feature |
//! | POST | `/api/v1/client/marketplace/{slug}/install` | the same |
//! | POST | `/api/v1/client/marketplace/{slug}/update` | the same |
//! | DELETE | `/api/v1/client/marketplace/{slug}` | the same |
//!
//! The list is the marketplace of the build (`deploy/marketplace/`) with, for each software, what
//! this account has installed and whether a newer version is on offer. Installing adds the
//! entry's image (pinned by digest, marked `marketplace:<slug>:<version>` in its note) to the
//! image catalog when it is not there and runs it as a container with the entry's port, memory
//! and environment, so every limit and every check of a client container applies as it is. An
//! administrator who disables the image stops new installs of it. An update runs the container
//! again from the newer image with the same /data folder (what a software keeps anywhere else in
//! its own file system is new after an update, as with any container); uninstalling removes the
//! container.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use builtpanel_catalog::marketplace::{self, Kind, Software};
use builtpanel_database::{
    container_images::{self as images, Image},
    runtime_apps as store,
};
use builtpanel_http_kit::{AppError, ProblemDetails};
use builtpanel_rbac::Principal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{AppsState, containers, internal, runtime};

/// The routes.
pub fn client_router() -> Router<AppsState> {
    Router::new()
        .route("/api/v1/client/marketplace", get(list))
        .route("/api/v1/client/marketplace/{slug}/install", post(install))
        .route("/api/v1/client/marketplace/{slug}/update", post(update))
        .route("/api/v1/client/marketplace/{slug}", delete(uninstall))
}

/// What this account has of a software.
#[derive(Debug, Serialize, ToSchema)]
pub struct Installed {
    /// The container's id.
    pub id: Uuid,
    /// The version it runs.
    pub version: String,
    /// `pending`, `ready`, `failed` or `removing`.
    pub status: String,
    /// Why it failed.
    pub error: Option<String>,
    /// A newer version is on offer.
    pub update_available: bool,
}

/// One piece of software of the marketplace.
#[derive(Debug, Serialize, ToSchema)]
pub struct SoftwareView {
    /// Identifier.
    pub slug: String,
    /// Name.
    pub name: String,
    /// The group (`databases`, `web_servers`, ...).
    pub category: String,
    /// The group's name.
    pub category_name: String,
    /// What it is.
    pub description: String,
    /// SPDX license.
    pub license: String,
    /// Project page.
    pub homepage: String,
    /// `container` (install here), `builtin` (the panel has it) or `unavailable`.
    pub kind: String,
    /// The version on offer.
    pub version: String,
    /// The Webuzo names this entry stands for.
    pub covers: Vec<String>,
    /// `builtin`: where in the panel.
    pub provided_by: String,
    /// `unavailable`: why.
    pub reason: String,
    /// `container`: how to reach it from the account's other applications.
    pub usage: String,
    /// `container`: the port inside the account's network.
    pub port: i32,
    /// `container`: the memory it gets, MiB.
    pub memory_mb: i32,
    /// What the account has.
    pub installed: Option<Installed>,
}

/// The account's installed software: container id, slug, version, status, error.
struct Have {
    id: Uuid,
    slug: String,
    version: String,
    status: String,
    error: Option<String>,
}

fn parse_note(note: &str) -> Option<(String, String)> {
    let rest = note.strip_prefix("marketplace:")?;
    let (slug, version) = rest.split_once(':')?;
    Some((slug.to_owned(), version.to_owned()))
}

async fn installed_of(state: &AppsState, tenant_id: Uuid) -> Result<Vec<Have>, AppError> {
    let rows: Vec<_> = store::list(&state.db, tenant_id)
        .await
        .map_err(internal)?
        .into_iter()
        .filter(|r| r.kind == "container")
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let links = images::of_apps(&state.db, &ids).await.map_err(internal)?;
    let all = images::list(&state.db, false).await.map_err(internal)?;
    let mut have = Vec::new();
    for row in rows {
        let Some((_, image_id)) = links.iter().find(|(app, _)| *app == row.id) else {
            continue;
        };
        let Some((slug, version)) = all
            .iter()
            .find(|i| i.id == *image_id)
            .and_then(|i| parse_note(&i.note))
        else {
            continue;
        };
        have.push(Have {
            id: row.id,
            slug,
            version,
            status: row.status.clone(),
            error: row.error.clone(),
        });
    }
    Ok(have)
}

fn catalog() -> Result<Vec<Software>, AppError> {
    marketplace::embedded().map_err(internal)
}

fn find(slug: &str) -> Result<Software, AppError> {
    catalog()?
        .into_iter()
        .find(|s| s.slug == slug)
        .ok_or_else(|| AppError::not_found("software"))
}

fn category_name(key: &str) -> String {
    marketplace::CATEGORIES
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.to_owned(), |(_, n)| (*n).to_owned())
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Container => "container",
        Kind::Builtin => "builtin",
        Kind::Unavailable => "unavailable",
    }
}

/// The marketplace, with what this account has installed.
#[utoipa::path(get, path = "/api/v1/client/marketplace", tag = "apps",
    operation_id = "clientListMarketplace", security(("session" = [])),
    responses((status = 200, description = "The software, grouped by category order", body = Vec<SoftwareView>),
              (status = 403, description = "`client_only` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn list(
    State(state): State<AppsState>,
    who: Principal,
) -> Result<Json<Vec<SoftwareView>>, AppError> {
    let (tenant, _) = runtime::feature(&state, &who).await?;
    let have = installed_of(&state, tenant.id).await?;
    Ok(Json(
        catalog()?
            .into_iter()
            .map(|s| {
                let installed = have
                    .iter()
                    .find(|h| h.slug == s.slug && h.status != "removing")
                    .map(|h| Installed {
                        id: h.id,
                        version: h.version.clone(),
                        status: h.status.clone(),
                        error: h.error.clone(),
                        update_available: h.version != s.version,
                    });
                SoftwareView {
                    category_name: category_name(&s.category),
                    kind: kind_name(s.kind).to_owned(),
                    slug: s.slug,
                    name: s.name,
                    category: s.category,
                    description: s.description,
                    license: s.license,
                    homepage: s.homepage,
                    version: s.version,
                    covers: s.covers,
                    provided_by: s.provided_by,
                    reason: s.reason,
                    usage: s.usage,
                    port: s.port,
                    memory_mb: s.memory_mb,
                    installed,
                }
            })
            .collect(),
    ))
}

/// The image of this entry in the image catalog (added when it is not there).
async fn image_of(state: &AppsState, s: &Software) -> Result<Image, AppError> {
    let reference = s.reference();
    if let Some(i) = images::list(&state.db, false)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|i| i.reference == reference)
    {
        return Ok(i);
    }
    let name = format!("marketplace/{}/{}", s.slug, s.version);
    let note = s.image_note();
    let write = images::Write {
        name: &name,
        reference: &reference,
        default_port: s.port,
        default_memory_mb: s.memory_mb,
        note: &note,
        enabled: true,
    };
    match images::insert(&state.db, &write).await {
        Ok(i) => Ok(i),
        // Added by a request at the same moment.
        Err(images::WriteError::Taken) => images::list(&state.db, false)
            .await
            .map_err(internal)?
            .into_iter()
            .find(|i| i.reference == reference)
            .ok_or_else(|| internal(std::io::Error::other("the image could not be added"))),
        Err(images::WriteError::Database(e)) => Err(internal(e)),
    }
}

fn installable(s: &Software) -> Result<(), AppError> {
    match s.kind {
        Kind::Container => Ok(()),
        Kind::Builtin => Err(AppError::conflict(
            "builtin",
            format!("{} is part of the panel: {}.", s.name, s.provided_by),
        )),
        Kind::Unavailable => Err(AppError::conflict(
            "unavailable",
            format!("{} cannot be installed here: {}", s.name, s.reason),
        )),
    }
}

/// What to give for a new installation.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallRequest {
    /// The domain that shows it (software that has a web page of its own).
    pub domain_id: Option<Uuid>,
}

/// Install a piece of software.
#[utoipa::path(post, path = "/api/v1/client/marketplace/{slug}/install", tag = "apps",
    operation_id = "clientInstallSoftware", security(("session" = [])),
    params(("slug" = String, Path, description = "Software"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = InstallRequest,
    responses((status = 202, description = "Queued", body = containers::ContainerView),
              (status = 404, description = "No such software", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 409, description = "`already_installed`, `builtin`, `unavailable`, `limit_reached` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn install(
    State(state): State<AppsState>,
    who: Principal,
    Path(slug): Path<String>,
    body: Option<Json<InstallRequest>>,
) -> Result<(StatusCode, Json<containers::ContainerView>), AppError> {
    let s = find(&slug)?;
    installable(&s)?;
    let (tenant, _) = runtime::feature(&state, &who).await?;
    if installed_of(&state, tenant.id)
        .await?
        .iter()
        .any(|h| h.slug == s.slug && h.status != "removing")
    {
        return Err(AppError::conflict(
            "already_installed",
            format!("{} is installed already.", s.name),
        ));
    }
    let image = image_of(&state, &s).await?;
    let request = containers::CreateContainer {
        name: s.slug.clone(),
        image_id: image.id,
        port: Some(s.port),
        memory_mb: Some(s.memory_mb),
        env: Some(s.env.clone()),
        domain_id: body.and_then(|Json(b)| b.domain_id),
    };
    containers::client_create(State(state), who, Json(request)).await
}

/// Run an installed software again from the version on offer.
#[utoipa::path(post, path = "/api/v1/client/marketplace/{slug}/update", tag = "apps",
    operation_id = "clientUpdateSoftware", security(("session" = [])),
    params(("slug" = String, Path, description = "Software"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 202, description = "Queued", body = containers::ContainerView),
              (status = 404, description = "No such software, or it is not installed", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 409, description = "`up_to_date`, `limit_reached` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn update(
    State(state): State<AppsState>,
    who: Principal,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<containers::ContainerView>), AppError> {
    let s = find(&slug)?;
    installable(&s)?;
    let (tenant, _) = runtime::feature(&state, &who).await?;
    let have = installed_of(&state, tenant.id)
        .await?
        .into_iter()
        .find(|h| h.slug == s.slug && h.status != "removing")
        .ok_or_else(|| AppError::not_found("installation"))?;
    if have.version == s.version {
        return Err(AppError::conflict(
            "up_to_date",
            format!("{} is at {} already.", s.name, s.version),
        ));
    }
    let image = image_of(&state, &s).await?;
    let request = containers::ChangeContainer {
        image_id: Some(image.id),
        port: Some(s.port),
        memory_mb: None,
        env: Some(s.env.clone()),
        domain_id: None,
    };
    containers::client_update(State(state), who, Path(have.id), Json(request)).await
}

/// Remove an installed software (its container goes).
#[utoipa::path(delete, path = "/api/v1/client/marketplace/{slug}", tag = "apps",
    operation_id = "clientUninstallSoftware", security(("session" = [])),
    params(("slug" = String, Path, description = "Software"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 202, description = "Queued"),
              (status = 404, description = "No such software, or it is not installed", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn uninstall(
    State(state): State<AppsState>,
    who: Principal,
    Path(slug): Path<String>,
) -> Result<StatusCode, AppError> {
    let s = find(&slug)?;
    let (tenant, _) = runtime::feature(&state, &who).await?;
    let have = installed_of(&state, tenant.id)
        .await?
        .into_iter()
        .find(|h| h.slug == s.slug && h.status != "removing")
        .ok_or_else(|| AppError::not_found("installation"))?;
    runtime::remove(State(state), who, Path(have.id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_of_an_image_names_software_and_version() {
        assert_eq!(
            parse_note("marketplace:redis:8.0.2"),
            Some(("redis".into(), "8.0.2".into()))
        );
        assert_eq!(
            parse_note("marketplace:elasticsearch:9.2.0"),
            Some(("elasticsearch".into(), "9.2.0".into()))
        );
        assert_eq!(parse_note("a note an administrator wrote"), None);
        assert_eq!(parse_note("marketplace:nover"), None);
    }

    #[test]
    fn every_container_entry_is_acceptable_to_the_container_rules() {
        for s in marketplace::embedded().unwrap_or_default() {
            if s.kind == Kind::Container {
                assert!(
                    crate::runtime::check_env(&s.env).is_ok(),
                    "{} has an environment the container rules refuse",
                    s.slug
                );
                assert!(crate::runtime::valid_name(&s.slug), "{}", s.slug);
            }
        }
    }
}
