//! The modules compiled into this binary, by their manifests (T03.01).
//!
//! Each manifest is embedded at build time and validated at startup. A broken
//! manifest stops the service with a message that lists every problem.
//! (The `Module` trait and the registry that will own this list are T03.02.)

use std::sync::Arc;

use dhpanel_settings::SettingsRegistry;

use dhpanel_kernel::{
    Manifest, ManifestError, ManifestOnly, Module, ModuleMount, ModuleRoutes, Registry,
};

/// Directory name under `modules/` and the manifest text found there.
const EMBEDDED: &[(&str, &str)] = &[
    ("auth", include_str!("../../../modules/auth/module.toml")),
    ("rbac", include_str!("../../../modules/rbac/module.toml")),
    ("audit", include_str!("../../../modules/audit/module.toml")),
    ("nodes", include_str!("../../../modules/nodes/module.toml")),
    ("system", dhpanel_mod_system::MANIFEST),
];

/// A module of this binary: its manifest and how to make its routes. The
/// routes are made each time the set of mounted modules changes.
struct Wired {
    manifest: Manifest,
    routes: Box<dyn Fn() -> ModuleRoutes + Send + Sync>,
}

impl Module for Wired {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn routes(&self) -> ModuleRoutes {
        (self.routes)()
    }

    fn settings_schema(&self) -> Option<serde_json::Value> {
        match self.manifest.module.name.as_str() {
            "system" => serde_json::from_str(dhpanel_mod_system::SETTINGS_SCHEMA).ok(),
            _ => None,
        }
    }
}

/// Checks the settings schemas of all modules (T03.08).
///
/// # Errors
/// The messages to show, one per unusable schema.
pub fn settings(registry: &Registry) -> Result<SettingsRegistry, Vec<String>> {
    let mut schemas = Vec::new();
    let mut problems = Vec::new();
    for module in registry.modules() {
        let name = module.manifest().module.name.clone();
        if name == "system" && module.settings_schema().is_none() {
            problems.push(format!(
                "settings schema of module `{name}`: not valid JSON"
            ));
        }
        if let Some(schema) = module.settings_schema() {
            schemas.push((name, schema));
        }
    }
    match SettingsRegistry::build(schemas) {
        Ok(settings) if problems.is_empty() => Ok(settings),
        Ok(_) => Err(problems),
        Err(errors) => {
            problems.extend(errors.iter().map(ToString::to_string));
            Err(problems)
        }
    }
}

/// The state each module's routes need.
pub struct States {
    /// Sign-in and sessions (also carries what the audit viewer needs).
    pub auth: dhpanel_mod_auth::AuthState,
    /// Administrators and roles.
    pub admin: dhpanel_mod_admin::AdminState,
    /// Nodes and operations.
    pub nodes: dhpanel_mod_nodes::NodesState,
}

/// Builds the registry of this binary with every module's routes attached,
/// and the live mount that serves them. `base` holds the routes outside any
/// module and the fallback for unknown paths.
///
/// # Errors
/// The messages to show when a manifest or the module set is broken.
pub fn mount(states: States, base: axum::Router) -> Result<ModuleMount, Vec<String>> {
    let manifests = load_from(EMBEDDED)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    // The system module's routes need the mount itself (to switch modules), which
    // exists only after the registry does.
    let late_mount: Arc<std::sync::OnceLock<ModuleMount>> = Arc::default();
    let (db, sessions) = (states.auth.db.clone(), states.auth.sessions.clone());
    let States { auth, admin, nodes } = states;
    let modules = manifests
        .into_iter()
        .map(|manifest| {
            let routes: Box<dyn Fn() -> ModuleRoutes + Send + Sync> =
                match manifest.module.name.as_str() {
                    "auth" => {
                        let auth = auth.clone();
                        Box::new(move || ModuleRoutes {
                            public: Some(dhpanel_mod_auth::router().with_state(auth.clone())),
                            ..ModuleRoutes::none()
                        })
                    }
                    "rbac" => {
                        let admin = admin.clone();
                        Box::new(move || ModuleRoutes {
                            admin: Some(dhpanel_mod_admin::router().with_state(admin.clone())),
                            ..ModuleRoutes::none()
                        })
                    }
                    "audit" => {
                        let auth = auth.clone();
                        Box::new(move || ModuleRoutes {
                            admin: Some(
                                dhpanel_audit::router::<dhpanel_mod_auth::AuthState>()
                                    .with_state(auth.clone()),
                            ),
                            ..ModuleRoutes::none()
                        })
                    }
                    "nodes" => {
                        let nodes = nodes.clone();
                        Box::new(move || ModuleRoutes {
                            admin: Some(dhpanel_mod_nodes::router().with_state(nodes.clone())),
                            ..ModuleRoutes::none()
                        })
                    }
                    "system" => {
                        let (db, sessions, late_mount) =
                            (db.clone(), sessions.clone(), late_mount.clone());
                        Box::new(move || match late_mount.get() {
                            Some(mount) => ModuleRoutes {
                                admin: Some(dhpanel_mod_system::router().with_state(
                                    dhpanel_mod_system::SystemState {
                                        db: db.clone(),
                                        sessions: sessions.clone(),
                                        mount: mount.clone(),
                                    },
                                )),
                                ..ModuleRoutes::none()
                            },
                            None => ModuleRoutes::none(),
                        })
                    }
                    // A manifest without code behind it has no routes.
                    _ => Box::new(ModuleRoutes::none),
                };
            Arc::new(Wired { manifest, routes }) as Arc<dyn Module>
        })
        .collect();
    let registry = Registry::build(modules).map_err(|err| vec![err.to_string()])?;
    let mount = ModuleMount::new(registry, base);
    let _ = late_mount.set(mount.clone());
    mount.rebuild();
    Ok(mount)
}

/// Keeps the served routes in step with the modules table: once as soon as the
/// database answers, then every `every` (which also picks up a change made by
/// another control plane instance).
pub async fn follow_enabled(
    db: dhpanel_database::sqlx::PgPool,
    mount: ModuleMount,
    every: std::time::Duration,
) {
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match dhpanel_database::modules::enabled_names(&db).await {
            Ok(names) => {
                if mount.apply(&names.into_iter().collect()) {
                    tracing::info!(active = ?mount.mounted(), "module routes follow the modules table");
                }
            }
            Err(err) => tracing::debug!(error = %err, "could not read the modules table"),
        }
    }
}

/// Parses the embedded manifests.
///
/// # Errors
/// One [`ManifestError`] per broken manifest.
#[cfg(test)]
pub fn load() -> Result<Vec<Manifest>, Vec<ManifestError>> {
    load_from(EMBEDDED)
}

/// Parses `(directory, text)` pairs. Besides each manifest's own rules, the
/// module name must equal its directory. How the modules fit together is
/// checked by [`registry`].
pub fn load_from(sources: &[(&str, &str)]) -> Result<Vec<Manifest>, Vec<ManifestError>> {
    let mut manifests = Vec::new();
    let mut errors = Vec::new();
    for (dir, text) in sources {
        let origin = format!("modules/{dir}/module.toml");
        match Manifest::parse(&origin, text) {
            Ok(manifest) if manifest.module.name != *dir => errors.push(ManifestError {
                origin,
                problems: vec![format!(
                    "module.name: `{}` must match its directory `{dir}`",
                    manifest.module.name
                )],
            }),
            Ok(manifest) => manifests.push(manifest),
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Ok(manifests)
    } else {
        Err(errors)
    }
}

/// Builds the module registry of this binary: every manifest valid, every
/// dependency present, no cycles, no key defined twice.
///
/// # Errors
/// The messages to show, one per broken manifest or one for the module set.
pub fn registry() -> Result<Registry, Vec<String>> {
    registry_from(EMBEDDED)
}

/// [`registry`] for the given manifests.
pub fn registry_from(sources: &[(&str, &str)]) -> Result<Registry, Vec<String>> {
    let manifests = load_from(sources)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    // Until the modules' code is behind the `Module` trait (T03.04) they are
    // represented by their manifests.
    let modules = manifests
        .into_iter()
        .map(|m| Arc::new(ManifestOnly(m)) as Arc<dyn Module>)
        .collect();
    Registry::build(modules).map_err(|err| vec![err.to_string()])
}

/// Writes the modules of this build into the `modules` table (T03.03): new
/// ones appear switched on, known ones are refreshed, and modules that are no
/// longer part of the build are marked missing.
///
/// # Errors
/// Database errors (the caller retries; the API works without the sync).
pub async fn sync(
    db: &dhpanel_database::sqlx::PgPool,
    registry: &Registry,
) -> Result<dhpanel_database::modules::SyncReport, dhpanel_database::sqlx::Error> {
    let manifests: Vec<(&Manifest, serde_json::Value)> = registry
        .modules()
        .iter()
        .map(|m| {
            let manifest = m.manifest();
            (manifest, serde_json::to_value(manifest).unwrap_or_default())
        })
        .collect();
    let records: Vec<dhpanel_database::modules::ModuleRecord<'_>> = manifests
        .iter()
        .map(|(manifest, json)| dhpanel_database::modules::ModuleRecord {
            name: &manifest.module.name,
            version: &manifest.module.version,
            category: manifest.module.category.as_str(),
            core: manifest.module.core,
            manifest: json,
        })
        .collect();
    dhpanel_database::modules::sync(db, &records).await
}

/// Writes the permission keys the manifests declare into the catalogue (T03.06).
///
/// # Errors
/// Database errors.
pub async fn sync_permissions(
    db: &dhpanel_database::sqlx::PgPool,
    registry: &Registry,
) -> Result<dhpanel_database::permissions::SyncReport, dhpanel_database::sqlx::Error> {
    let keys: Vec<(&str, &str)> = registry
        .modules()
        .iter()
        .flat_map(|m| {
            let manifest = m.manifest();
            manifest
                .permissions
                .keys
                .iter()
                .map(|key| (key.as_str(), manifest.module.name.as_str()))
        })
        .collect();
    dhpanel_database::permissions::sync(db, &keys).await
}

/// Runs [`sync`] until it succeeds once (the database may come up after the
/// API does), logging what changed.
pub async fn sync_until_done(db: dhpanel_database::sqlx::PgPool, registry: Registry) {
    let mut wait = std::time::Duration::from_secs(1);
    loop {
        match sync_permissions(&db, &registry).await {
            Ok(report) if !report.is_empty() => tracing::info!(
                added = ?report.added,
                restored = ?report.restored,
                moved = ?report.moved,
                missing = ?report.missing,
                "permission catalogue synced with this build"
            ),
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(error = %err, retry_in = ?wait, "could not sync the permission catalogue");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(std::time::Duration::from_secs(30));
                continue;
            }
        }
        match sync(&db, &registry).await {
            Ok(report) => {
                if report.is_empty() {
                    tracing::debug!("modules table already matches this build");
                } else {
                    tracing::info!(
                        added = ?report.added,
                        restored = ?report.restored,
                        updated = ?report.updated,
                        missing = ?report.missing,
                        "modules table synced with this build"
                    );
                }
                return;
            }
            Err(err) => {
                tracing::warn!(error = %err, retry_in = ?wait, "could not sync the modules table");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(std::time::Duration::from_secs(30));
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_manifests_are_valid() {
        let manifests = load().unwrap_or_else(|errors| {
            panic!(
                "{}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        let names: Vec<_> = manifests.iter().map(|m| m.module.name.as_str()).collect();
        assert_eq!(names, ["auth", "rbac", "audit", "nodes", "system"]);
        // And they fit together: dependencies first.
        let registry = registry().unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            ["auth", "rbac", "audit", "nodes", "system"]
        );
        assert_eq!(registry.dependents_of("rbac"), ["audit", "nodes", "system"]);
        // Every dependency names a module that exists.
        for m in &manifests {
            for dep in &m.module.depends_on {
                assert!(
                    names.contains(&dep.as_str()),
                    "{} depends on unknown {dep}",
                    m.module.name
                );
            }
        }
    }

    #[test]
    fn every_permission_the_code_checks_is_declared_by_a_manifest() {
        use dhpanel_rbac::{PermissionKey as _, permissions as p};
        let declared: Vec<String> = load()
            .unwrap()
            .into_iter()
            .flat_map(|m| m.permissions.keys)
            .collect();
        for key in [
            p::AdministratorsRead::KEY,
            p::AdministratorsManage::KEY,
            p::RolesRead::KEY,
            p::RolesManage::KEY,
            p::AuditRead::KEY,
            p::NodesRead::KEY,
            p::NodesManage::KEY,
            p::ModulesRead::KEY,
            p::ModulesManage::KEY,
            p::ModulesRead::KEY,
            p::ModulesManage::KEY,
        ] {
            assert!(declared.iter().any(|d| d == key), "{key} is in no manifest");
        }
        assert_eq!(
            declared.len(),
            9,
            "a manifest declares a key the code does not know: {declared:?}"
        );
    }

    #[test]
    fn a_broken_manifest_gives_a_clear_error() {
        let good = "[module]\nname = \"alpha\"\nversion = \"1.0.0\"\ntitle = { en = \"Alpha\" }\ncategory = \"web\"\n[permissions]\nkeys = [\"alpha.read\"]\n";
        let errors = load_from(&[
            ("alpha", good),
            ("beta", "[module]\nname = \"beta\"\nversion = \"one\"\ntitle = { en = \"Beta\" }\ncategory = \"web\"\ndepend_on = []\n"),
        ])
        .unwrap_err();
        assert_eq!(errors.len(), 1);
        let text = errors[0].to_string();
        assert!(
            text.starts_with("invalid manifest for modules/beta/module.toml:"),
            "{text}"
        );
        assert!(text.contains("depend_on"), "the typo is named: {text}");

        // The name must match the directory.
        let text = load_from(&[("gamma", good)]).unwrap_err()[0].to_string();
        assert!(
            text.contains("`alpha` must match its directory `gamma`"),
            "{text}"
        );

        // Two modules cannot define the same permission key, and a missing
        // dependency or a cycle stops the build of the registry.
        let thief = good.replace("name = \"alpha\"", "name = \"delta\"");
        let text = registry_from(&[("alpha", good), ("delta", &thief)])
            .unwrap_err()
            .join("\n");
        assert!(
            text.contains("permission `alpha.read` is defined by both `alpha` and `delta`"),
            "{text}"
        );
        let needy = "[module]\nname = \"needy\"\nversion = \"1.0.0\"\ntitle = { en = \"N\" }\ncategory = \"web\"\ndepends_on = [\"ghost\"]\n";
        let text = registry_from(&[("needy", needy)]).unwrap_err().join("\n");
        assert!(
            text.contains("module `needy` depends on `ghost`, which is not part of this build"),
            "{text}"
        );
    }
}
