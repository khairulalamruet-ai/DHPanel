//! The modules compiled into this binary, by their manifests (T03.01).
//!
//! Each manifest is embedded at build time and validated at startup. A broken
//! manifest stops the service with a message that lists every problem.
//! (The `Module` trait and the registry that will own this list are T03.02.)

use std::sync::Arc;

use dhpanel_kernel::{Manifest, ManifestError, ManifestOnly, Module, Registry};

/// Directory name under `modules/` and the manifest text found there.
const EMBEDDED: &[(&str, &str)] = &[
    ("auth", include_str!("../../../modules/auth/module.toml")),
    ("rbac", include_str!("../../../modules/rbac/module.toml")),
    ("audit", include_str!("../../../modules/audit/module.toml")),
    ("nodes", include_str!("../../../modules/nodes/module.toml")),
];

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
        assert_eq!(names, ["auth", "rbac", "audit", "nodes"]);
        // And they fit together: dependencies first.
        let registry = registry().unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            ["auth", "rbac", "audit", "nodes"]
        );
        assert_eq!(registry.dependents_of("rbac"), ["audit", "nodes"]);
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
        ] {
            assert!(declared.iter().any(|d| d == key), "{key} is in no manifest");
        }
        assert_eq!(
            declared.len(),
            7,
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
