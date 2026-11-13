//! Serving only the routes of enabled modules, and changing that while the
//! server runs (T03.04).
//!
//! [`ModuleMount`] owns one router made of a fixed base (health, API
//! description, the "not found" answer) plus the routes of every module that
//! is *effectively on*. Switching a module on or off builds a new router and
//! swaps it in; requests that are being served finish on the old one, new
//! requests see the new one. No restart, no dropped connection.
//!
//! A module is effectively on when it is core, or when it is switched on and
//! everything it depends on is effectively on. A switched-off module's routes
//! simply do not exist: they answer like any unknown path (404).

use std::{
    collections::BTreeSet,
    sync::{Arc, PoisonError, RwLock},
};

use axum::{Router, body::Body, http::Request, response::IntoResponse as _};
use tower::ServiceExt as _;

use crate::registry::Registry;

struct Current {
    router: Router,
    mounted: BTreeSet<String>,
}

struct Inner {
    registry: Registry,
    base: Router,
    current: RwLock<Current>,
}

/// The live set of module routes. Cheap to clone; all clones share the state.
#[derive(Clone)]
pub struct ModuleMount {
    inner: Arc<Inner>,
}

impl ModuleMount {
    /// Starts with only the core modules mounted (the safe set while the
    /// administrator's choices are not known yet, for example because the
    /// database is still starting). `base` holds the routes that are not part
    /// of any module and the fallback for unknown paths.
    #[must_use]
    pub fn new(registry: Registry, base: Router) -> Self {
        let mounted = effective(&registry, &BTreeSet::new());
        let router = build(&registry, &base, &mounted);
        Self {
            inner: Arc::new(Inner {
                registry,
                base,
                current: RwLock::new(Current { router, mounted }),
            }),
        }
    }

    /// The modules of this build.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.inner.registry
    }

    /// Names of the modules whose routes are being served right now.
    #[must_use]
    pub fn mounted(&self) -> BTreeSet<String> {
        self.inner
            .current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .mounted
            .clone()
    }

    /// The modules whose routes are being served right now, dependencies first.
    /// These are the modules that receive events.
    #[must_use]
    pub fn active_modules(&self) -> Vec<Arc<dyn crate::module::Module>> {
        let mounted = self.mounted();
        self.inner
            .registry
            .modules()
            .iter()
            .filter(|m| mounted.contains(&m.manifest().module.name))
            .cloned()
            .collect()
    }

    /// What would be served if exactly `enabled` were switched on: core
    /// modules always, others only with all their dependencies.
    #[must_use]
    pub fn effective(&self, enabled: &BTreeSet<String>) -> BTreeSet<String> {
        effective(&self.inner.registry, enabled)
    }

    /// Serves the routes for this set of switched-on modules from now on.
    /// Returns whether anything changed.
    pub fn apply(&self, enabled: &BTreeSet<String>) -> bool {
        let mounted = effective(&self.inner.registry, enabled);
        if mounted == self.mounted() {
            return false;
        }
        // Built outside the lock: building calls into every module.
        let router = build(&self.inner.registry, &self.inner.base, &mounted);
        *self
            .inner
            .current
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Current { router, mounted };
        true
    }

    /// Builds the routes again for the modules that are mounted now. For
    /// modules whose routes depend on something that became available after
    /// the mount was created.
    pub fn rebuild(&self) {
        let mounted = self.mounted();
        let router = build(&self.inner.registry, &self.inner.base, &mounted);
        *self
            .inner
            .current
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Current { router, mounted };
    }

    /// A router that forwards every request to whatever is mounted at that
    /// moment. Put the application's middleware around this.
    pub fn service(&self) -> Router {
        let inner = self.inner.clone();
        Router::new().fallback(move |request: Request<Body>| {
            let router = inner
                .current
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .router
                .clone();
            async move {
                match router.oneshot(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                }
                .into_response()
            }
        })
    }
}

fn effective(registry: &Registry, enabled: &BTreeSet<String>) -> BTreeSet<String> {
    let mut on = BTreeSet::new();
    // The registry is ordered dependencies-first, so one pass settles it.
    for module in registry.modules() {
        let info = &module.manifest().module;
        let wanted = info.core || enabled.contains(&info.name);
        if wanted && info.depends_on.iter().all(|dep| on.contains(dep)) {
            on.insert(info.name.clone());
        }
    }
    on
}

fn build(registry: &Registry, base: &Router, mounted: &BTreeSet<String>) -> Router {
    let mut router = base.clone();
    for module in registry.modules() {
        if !mounted.contains(&module.manifest().module.name) {
            continue;
        }
        let routes = module.routes();
        for part in [routes.public, routes.admin, routes.client]
            .into_iter()
            .flatten()
        {
            router = router.merge(part);
        }
    }
    router
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use axum::{http::StatusCode, routing::get};

    use super::*;
    use crate::{
        manifest::Manifest,
        module::{Module, ModuleRoutes},
    };

    struct Fake {
        manifest: Manifest,
    }

    impl Module for Fake {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn routes(&self) -> ModuleRoutes {
            let name = self.manifest.module.name.clone();
            ModuleRoutes {
                admin: Some(Router::new().route(
                    &format!("/api/v1/admin/{name}"),
                    get(move || {
                        let name = name.clone();
                        async move { name }
                    }),
                )),
                ..ModuleRoutes::none()
            }
        }
    }

    fn fake(name: &str, core: bool, deps: &[&str]) -> Arc<dyn Module> {
        let deps = deps
            .iter()
            .map(|d| format!("\"{d}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let category = if core { "core" } else { "web" };
        let text = format!(
            "[module]\nname = \"{name}\"\nversion = \"1.0.0\"\ntitle = {{ en = \"{name}\" }}\ncategory = \"{category}\"\ncore = {core}\ndepends_on = [{deps}]\n"
        );
        Arc::new(Fake {
            manifest: Manifest::parse(name, &text).unwrap(),
        })
    }

    fn mount() -> ModuleMount {
        let registry = Registry::build(vec![
            fake("auth", true, &[]),
            fake("dns", false, &[]),
            fake("domains", false, &["dns"]),
            fake("ssl", false, &["domains"]),
        ])
        .unwrap();
        let base = Router::new()
            .route("/health", get(|| async { "ok" }))
            .fallback(|| async { (StatusCode::NOT_FOUND, "no such route") });
        ModuleMount::new(registry, base)
    }

    async fn status(app: &Router, path: &str) -> StatusCode {
        app.clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[tokio::test]
    async fn only_enabled_modules_answer_and_switching_needs_no_restart() {
        let mount = mount();
        // One service handle for the whole test: what a running server holds.
        let app = mount.service();
        assert_eq!(mount.mounted(), set(&["auth"]), "core only at first");
        assert_eq!(status(&app, "/health").await, StatusCode::OK);
        assert_eq!(status(&app, "/api/v1/admin/auth").await, StatusCode::OK);
        assert_eq!(
            status(&app, "/api/v1/admin/dns").await,
            StatusCode::NOT_FOUND
        );

        assert!(mount.apply(&set(&["dns", "domains"])));
        assert_eq!(status(&app, "/api/v1/admin/dns").await, StatusCode::OK);
        assert_eq!(status(&app, "/api/v1/admin/domains").await, StatusCode::OK);
        assert_eq!(
            status(&app, "/api/v1/admin/ssl").await,
            StatusCode::NOT_FOUND
        );
        assert!(!mount.apply(&set(&["dns", "domains"])), "nothing changed");

        // Switched off again: its routes are gone like any unknown path.
        assert!(mount.apply(&set(&["dns"])));
        assert_eq!(
            status(&app, "/api/v1/admin/domains").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(status(&app, "/api/v1/admin/dns").await, StatusCode::OK);
        assert_eq!(status(&app, "/health").await, StatusCode::OK);
        assert_eq!(status(&app, "/nowhere").await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_module_without_its_dependencies_is_not_served_and_core_always_is() {
        let mount = mount();
        let app = mount.service();
        // ssl and domains are switched on, dns is not: neither can work.
        assert!(!mount.apply(&set(&["domains", "ssl"])));
        assert_eq!(mount.mounted(), set(&["auth"]));
        assert_eq!(
            status(&app, "/api/v1/admin/ssl").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            mount.effective(&set(&["dns", "domains", "ssl"])),
            set(&["auth", "dns", "domains", "ssl"])
        );
        // Core cannot be left out, and unknown names are ignored.
        mount.apply(&set(&["ghost"]));
        assert_eq!(status(&app, "/api/v1/admin/auth").await, StatusCode::OK);
        assert_eq!(mount.mounted(), set(&["auth"]));
    }

    #[tokio::test]
    async fn a_request_in_flight_finishes_on_the_router_it_started_with() {
        let registry =
            Registry::build(vec![fake("auth", true, &[]), fake("dns", false, &[])]).unwrap();
        let (entered, started) = tokio::sync::oneshot::channel::<()>();
        let (release, gate) = tokio::sync::oneshot::channel::<()>();
        let entered = Arc::new(std::sync::Mutex::new(Some(entered)));
        let gate = Arc::new(tokio::sync::Mutex::new(Some(gate)));
        let base = Router::new().route(
            "/slow",
            get(move || {
                let (entered, gate) = (entered.clone(), gate.clone());
                async move {
                    if let Some(tx) = entered.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    if let Some(rx) = gate.lock().await.take() {
                        let _ = rx.await;
                    }
                    "done"
                }
            }),
        );
        let mount = ModuleMount::new(registry, base);
        let app = mount.service();
        let slow = tokio::spawn({
            let app = app.clone();
            async move { status(&app, "/slow").await }
        });
        started.await.unwrap();
        assert!(
            mount.apply(&set(&["dns"])),
            "swapped while a request is running"
        );
        release.send(()).unwrap();
        assert_eq!(slow.await.unwrap(), StatusCode::OK);
        assert_eq!(status(&app, "/api/v1/admin/dns").await, StatusCode::OK);
    }
}
