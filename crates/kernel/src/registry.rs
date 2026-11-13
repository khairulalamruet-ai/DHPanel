//! All modules of this build, checked and ordered (T03.02).
//!
//! [`Registry::build`] refuses a set of modules that cannot work together:
//! two modules with one name, a dependency on a module that is not there, a
//! dependency cycle, a core module that depends on a module which can be
//! switched off, or a permission, feature or limit key claimed twice. Every
//! problem is reported, each one naming the modules involved.
//!
//! A registry that was built is ordered so that every module comes after the
//! modules it depends on.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::module::Module;

/// Why a set of modules was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the modules of this build do not fit together:{}", .problems.iter().map(|p| format!("\n  - {p}")).collect::<String>())]
pub struct RegistryError {
    /// One entry per problem.
    pub problems: Vec<Problem>,
}

/// One thing wrong with the module set.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    /// Two modules have the same name.
    #[error("module `{0}` is registered twice")]
    DuplicateModule(String),
    /// A module depends on one that is not part of this build.
    #[error("module `{module}` depends on `{dependency}`, which is not part of this build")]
    UnknownDependency {
        /// The module with the dependency.
        module: String,
        /// The missing module.
        dependency: String,
    },
    /// Modules depend on each other in a circle. The path starts and ends with
    /// the same module.
    #[error("dependency cycle: {}", .0.join(" -> "))]
    Cycle(Vec<String>),
    /// A core module (always on) depends on a module that can be switched off.
    #[error(
        "core module `{module}` depends on `{dependency}`, which is not core and could be switched off"
    )]
    CoreDependsOnOptional {
        /// The core module.
        module: String,
        /// The optional module it depends on.
        dependency: String,
    },
    /// Two modules define the same permission, feature or limit key.
    #[error("{kind} `{key}` is defined by both `{first}` and `{second}`")]
    DuplicateKey {
        /// `permission`, `feature` or `limit`.
        kind: &'static str,
        /// The key.
        key: String,
        /// The module that had it first (by name).
        first: String,
        /// The other one.
        second: String,
    },
}

/// The modules of this build, in dependency order.
#[derive(Clone)]
pub struct Registry {
    ordered: Vec<Arc<dyn Module>>,
    index: BTreeMap<String, usize>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

impl Registry {
    /// Checks the modules and orders them.
    ///
    /// # Errors
    /// [`RegistryError`] with every problem found.
    pub fn build(modules: Vec<Arc<dyn Module>>) -> Result<Self, RegistryError> {
        let mut problems = Vec::new();
        let mut by_name: BTreeMap<String, Arc<dyn Module>> = BTreeMap::new();
        for module in modules {
            let name = module.manifest().module.name.clone();
            match by_name.entry(name) {
                std::collections::btree_map::Entry::Occupied(taken) => {
                    problems.push(Problem::DuplicateModule(taken.key().clone()));
                }
                std::collections::btree_map::Entry::Vacant(free) => {
                    free.insert(module);
                }
            }
        }

        // Dependencies exist; core modules lean only on core modules.
        for (name, module) in &by_name {
            let info = &module.manifest().module;
            for dependency in &info.depends_on {
                match by_name.get(dependency) {
                    None => problems.push(Problem::UnknownDependency {
                        module: name.clone(),
                        dependency: dependency.clone(),
                    }),
                    Some(target) if info.core && !target.manifest().module.core => {
                        problems.push(Problem::CoreDependsOnOptional {
                            module: name.clone(),
                            dependency: dependency.clone(),
                        });
                    }
                    Some(_) => {}
                }
            }
        }

        // No key is claimed twice.
        let mut owners: BTreeMap<(&'static str, String), String> = BTreeMap::new();
        for (name, module) in &by_name {
            let manifest = module.manifest();
            let claims = manifest
                .permissions
                .keys
                .iter()
                .map(|k| ("permission", k))
                .chain(manifest.features.keys.iter().map(|k| ("feature", k)))
                .chain(manifest.limits.keys().map(|k| ("limit", k)));
            for (kind, key) in claims {
                if let Some(first) = owners.insert((kind, key.clone()), name.clone()) {
                    problems.push(Problem::DuplicateKey {
                        kind,
                        key: key.clone(),
                        first,
                        second: name.clone(),
                    });
                }
            }
        }

        // Order by dependencies (Kahn's algorithm, by name for a stable order).
        // Only edges to modules that exist count; the missing ones are reported above.
        let mut waiting: BTreeMap<&str, BTreeSet<&str>> = by_name
            .iter()
            .map(|(name, module)| {
                let deps = module
                    .manifest()
                    .module
                    .depends_on
                    .iter()
                    .map(String::as_str)
                    .filter(|d| by_name.contains_key(*d))
                    .collect();
                (name.as_str(), deps)
            })
            .collect();
        let mut order: Vec<&str> = Vec::with_capacity(by_name.len());
        loop {
            let ready: Vec<&str> = waiting
                .iter()
                .filter(|(_, deps)| deps.is_empty())
                .map(|(name, _)| *name)
                .collect();
            if ready.is_empty() {
                break;
            }
            for name in ready {
                waiting.remove(name);
                for deps in waiting.values_mut() {
                    deps.remove(name);
                }
                order.push(name);
            }
        }
        if !waiting.is_empty() {
            problems.push(Problem::Cycle(find_cycle(&waiting)));
        }

        if !problems.is_empty() {
            return Err(RegistryError { problems });
        }
        let ordered: Vec<Arc<dyn Module>> = order
            .iter()
            .filter_map(|name| by_name.get(*name).cloned())
            .collect();
        let index = ordered
            .iter()
            .enumerate()
            .map(|(i, m)| (m.manifest().module.name.clone(), i))
            .collect();
        Ok(Self { ordered, index })
    }

    /// The modules, dependencies first.
    #[must_use]
    pub fn modules(&self) -> &[Arc<dyn Module>] {
        &self.ordered
    }

    /// Module names, dependencies first.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ordered
            .iter()
            .map(|m| m.manifest().module.name.as_str())
    }

    /// A module by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Module>> {
        self.index.get(name).map(|i| &self.ordered[*i])
    }

    /// Everything `name` needs, directly or indirectly, dependencies first.
    /// Empty for an unknown module.
    #[must_use]
    pub fn dependencies_of(&self, name: &str) -> Vec<&str> {
        let mut needed = BTreeSet::new();
        let mut stack = vec![name];
        while let Some(current) = stack.pop() {
            let Some(module) = self.get(current) else {
                continue;
            };
            for dep in &module.manifest().module.depends_on {
                if needed.insert(dep.as_str()) {
                    stack.push(dep.as_str());
                }
            }
        }
        self.names().filter(|n| needed.contains(n)).collect()
    }

    /// Every module that needs `name`, directly or indirectly, in dependency
    /// order. Switching `name` off breaks all of them.
    #[must_use]
    pub fn dependents_of(&self, name: &str) -> Vec<&str> {
        let mut affected: BTreeSet<&str> = BTreeSet::new();
        // The list is in dependency order, so one pass is enough.
        for module in &self.ordered {
            let info = &module.manifest().module;
            if info
                .depends_on
                .iter()
                .any(|d| d == name || affected.contains(d.as_str()))
            {
                affected.insert(info.name.as_str());
            }
        }
        self.names().filter(|n| affected.contains(n)).collect()
    }
}

/// Finds one cycle among modules that could not be ordered. Every module in
/// `waiting` still has an unmet dependency inside `waiting`, so following the
/// first one from anywhere must come back to a module already seen.
fn find_cycle(waiting: &BTreeMap<&str, BTreeSet<&str>>) -> Vec<String> {
    let Some(mut current) = waiting.keys().next().copied() else {
        return Vec::new();
    };
    let mut path: Vec<&str> = Vec::new();
    loop {
        if let Some(start) = path.iter().position(|seen| *seen == current) {
            let mut cycle: Vec<String> = path[start..].iter().map(|s| (*s).to_owned()).collect();
            cycle.push(current.to_owned());
            return cycle;
        }
        path.push(current);
        match waiting
            .get(current)
            .and_then(|deps| deps.iter().find(|d| waiting.contains_key(*d)))
        {
            Some(next) => current = next,
            // Cannot happen (see above); end the walk rather than loop.
            None => return path.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{manifest::Manifest, module::ManifestOnly};

    /// A module with dependencies; `core` and extra keys optional.
    fn module(name: &str, core: bool, deps: &[&str], extra: &str) -> Arc<dyn Module> {
        let deps = deps
            .iter()
            .map(|d| format!("\"{d}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let category = if core { "core" } else { "web" };
        let text = format!(
            "[module]\nname = \"{name}\"\nversion = \"1.0.0\"\ntitle = {{ en = \"{name}\" }}\ncategory = \"{category}\"\ncore = {core}\ndepends_on = [{deps}]\n{extra}"
        );
        Arc::new(ManifestOnly(Manifest::parse(name, &text).unwrap()))
    }

    fn problems(modules: Vec<Arc<dyn Module>>) -> Vec<Problem> {
        Registry::build(modules).unwrap_err().problems
    }

    #[test]
    fn modules_are_ordered_after_their_dependencies() {
        let registry = Registry::build(vec![
            module("domains", false, &["webserver", "dns"], ""),
            module("ssl", false, &["domains"], ""),
            module("dns", false, &[], ""),
            module("webserver", false, &["auth"], ""),
            module("auth", true, &[], ""),
        ])
        .unwrap();
        let order: Vec<&str> = registry.names().collect();
        assert_eq!(order, ["auth", "dns", "webserver", "domains", "ssl"]);
        // The same set in another order gives the same result.
        let again = Registry::build(vec![
            module("auth", true, &[], ""),
            module("ssl", false, &["domains"], ""),
            module("webserver", false, &["auth"], ""),
            module("dns", false, &[], ""),
            module("domains", false, &["webserver", "dns"], ""),
        ])
        .unwrap();
        assert_eq!(again.names().collect::<Vec<_>>(), order);

        assert_eq!(registry.get("dns").unwrap().manifest().module.name, "dns");
        assert!(registry.get("mail").is_none());
        assert_eq!(
            registry.dependencies_of("ssl"),
            ["auth", "dns", "webserver", "domains"]
        );
        assert_eq!(registry.dependencies_of("auth"), Vec::<&str>::new());
        // Switching dns off breaks domains, and ssl through it.
        assert_eq!(registry.dependents_of("dns"), ["domains", "ssl"]);
        assert_eq!(
            registry.dependents_of("auth"),
            ["webserver", "domains", "ssl"]
        );
        assert_eq!(registry.dependents_of("ssl"), Vec::<&str>::new());
        assert_eq!(registry.dependents_of("unknown"), Vec::<&str>::new());
    }

    #[test]
    fn a_dependency_cycle_is_refused_and_shown() {
        let found = problems(vec![
            module("aa", false, &["cc"], ""),
            module("bb", false, &["aa"], ""),
            module("cc", false, &["bb"], ""),
            module("free", false, &[], ""),
            module("victim", false, &["aa"], ""),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        let Problem::Cycle(path) = &found[0] else {
            panic!("not a cycle: {found:?}")
        };
        assert_eq!(path.first(), path.last(), "the path closes: {path:?}");
        assert_eq!(path.len(), 4);
        for name in ["aa", "bb", "cc"] {
            assert!(path.contains(&name.to_owned()), "{path:?}");
        }
        assert!(
            !path.contains(&"victim".to_owned()),
            "only the circle itself: {path:?}"
        );
        assert_eq!(
            found[0].to_string(),
            format!("dependency cycle: {}", path.join(" -> "))
        );

        // Two modules that need each other.
        let found = problems(vec![
            module("ab", false, &["ba"], ""),
            module("ba", false, &["ab"], ""),
        ]);
        assert!(
            matches!(&found[0], Problem::Cycle(p) if p.len() == 3),
            "{found:?}"
        );
    }

    #[test]
    fn missing_duplicate_and_ill_founded_modules_are_all_reported() {
        let found = problems(vec![
            module("auth", true, &["plans"], ""),
            module("plans", false, &[], ""),
            module("domains", false, &["dns", "webserver"], ""),
            module("domains", false, &[], ""),
        ]);
        assert!(
            found.contains(&Problem::DuplicateModule("domains".into())),
            "{found:?}"
        );
        assert!(found.contains(&Problem::CoreDependsOnOptional {
            module: "auth".into(),
            dependency: "plans".into()
        }));
        assert!(found.contains(&Problem::UnknownDependency {
            module: "domains".into(),
            dependency: "dns".into()
        }));
        assert!(found.contains(&Problem::UnknownDependency {
            module: "domains".into(),
            dependency: "webserver".into()
        }));
        assert_eq!(found.len(), 4);
        let text = Registry::build(vec![module("auth", true, &["plans"], "")])
            .unwrap_err()
            .to_string();
        assert!(text.starts_with("the modules of this build do not fit together:\n  - module `auth` depends on `plans`"), "{text}");
    }

    #[test]
    fn a_key_can_belong_to_one_module_only() {
        let found = problems(vec![
            module(
                "one",
                false,
                &[],
                "[permissions]\nkeys = [\"shared.read\"]\n[features]\nkeys = [\"thing\"]\n[limits]\nmax_things = { type = \"int\", default = 1 }\n",
            ),
            module(
                "two",
                false,
                &[],
                "[permissions]\nkeys = [\"shared.read\", \"two.read\"]\n[features]\nkeys = [\"thing\"]\n[limits]\nmax_things = { type = \"int\", default = 2 }\n",
            ),
        ]);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.contains(&Problem::DuplicateKey {
            kind: "permission",
            key: "shared.read".into(),
            first: "one".into(),
            second: "two".into()
        }));
        assert!(found.iter().any(|p| matches!(
            p,
            Problem::DuplicateKey {
                kind: "feature",
                ..
            }
        )));
        assert!(
            found
                .iter()
                .any(|p| matches!(p, Problem::DuplicateKey { kind: "limit", .. }))
        );
        // The same word as a permission in one module and a feature in another is fine.
        assert!(
            Registry::build(vec![
                module("one", false, &[], "[features]\nkeys = [\"thing\"]\n"),
                module(
                    "two",
                    false,
                    &[],
                    "[limits]\nthing = { type = \"int\", default = 2 }\n"
                ),
            ])
            .is_ok()
        );
    }

    #[test]
    fn an_empty_build_is_a_valid_registry() {
        let registry = Registry::build(Vec::new()).unwrap();
        assert!(registry.modules().is_empty());
    }
}
