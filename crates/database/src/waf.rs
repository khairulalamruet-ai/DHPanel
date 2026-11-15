//! WAF policies (T11.02): global, per account, per domain.

use sqlx::{PgPool, types::Uuid};

/// How the firewall acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// It does nothing.
    Off,
    /// It notes what it would block and lets everything through.
    Detect,
    /// It refuses what it finds.
    Block,
}

impl Mode {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Detect => "detect",
            Self::Block => "block",
        }
    }

    /// Reads a stored or given word.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "off" => Some(Self::Off),
            "detect" => Some(Self::Detect),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

/// Where a policy applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The whole panel.
    Global,
    /// One account.
    Tenant(Uuid),
    /// One domain of an account.
    Domain {
        /// The account.
        tenant_id: Uuid,
        /// The domain.
        domain_id: Uuid,
    },
}

/// A policy as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// The mode.
    pub mode: Mode,
    /// Rule ids that do not apply.
    pub disabled_rules: Vec<i32>,
}

/// What a domain really gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// The mode of the most specific policy that exists (`detect` when none does).
    pub mode: Mode,
    /// Where the mode came from: `domain`, `tenant`, `global` or `default`.
    pub mode_from: &'static str,
    /// The rules switched off by any of the three, sorted, each once.
    pub disabled_rules: Vec<i32>,
}

/// The mode used when nobody has set one.
pub const DEFAULT_MODE: Mode = Mode::Detect;

/// Combines the policies of the three scopes (most specific first in the decision).
#[must_use]
pub fn combine(
    global: Option<&Policy>,
    tenant: Option<&Policy>,
    domain: Option<&Policy>,
) -> Effective {
    let (mode, mode_from) = if let Some(p) = domain {
        (p.mode, "domain")
    } else if let Some(p) = tenant {
        (p.mode, "tenant")
    } else if let Some(p) = global {
        (p.mode, "global")
    } else {
        (DEFAULT_MODE, "default")
    };
    let mut disabled_rules: Vec<i32> = [global, tenant, domain]
        .into_iter()
        .flatten()
        .flat_map(|p| p.disabled_rules.iter().copied())
        .collect();
    disabled_rules.sort_unstable();
    disabled_rules.dedup();
    Effective {
        mode,
        mode_from,
        disabled_rules,
    }
}

struct Row {
    mode: String,
    disabled_rules: Vec<i32>,
}

fn policy(r: Row) -> Option<Policy> {
    Some(Policy {
        mode: Mode::parse(&r.mode)?,
        disabled_rules: r.disabled_rules,
    })
}

/// The policy set exactly at a scope.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, scope: Scope) -> Result<Option<Policy>, sqlx::Error> {
    let row = match scope {
        Scope::Global => {
            sqlx::query_as!(
                Row,
                "SELECT mode, disabled_rules FROM waf_policies WHERE scope = 'global'"
            )
            .fetch_optional(pool)
            .await?
        }
        Scope::Tenant(t) => {
            sqlx::query_as!(
                Row,
                "SELECT mode, disabled_rules FROM waf_policies WHERE scope = 'tenant' AND tenant_id = $1",
                t
            )
            .fetch_optional(pool)
            .await?
        }
        Scope::Domain { domain_id, .. } => {
            sqlx::query_as!(
                Row,
                "SELECT mode, disabled_rules FROM waf_policies WHERE scope = 'domain' AND domain_id = $1",
                domain_id
            )
            .fetch_optional(pool)
            .await?
        }
    };
    Ok(row.and_then(policy))
}

/// Sets the policy at a scope (replacing the old one).
///
/// # Errors
/// Database errors.
pub async fn set(
    pool: &PgPool,
    scope: Scope,
    mode: Mode,
    disabled_rules: &[i32],
    by: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let mut rules = disabled_rules.to_vec();
    rules.sort_unstable();
    rules.dedup();
    match scope {
        Scope::Global => {
            sqlx::query!(
                "INSERT INTO waf_policies (scope, mode, disabled_rules, updated_by)
                 VALUES ('global', $1, $2, $3)
                 ON CONFLICT (scope) WHERE scope = 'global'
                 DO UPDATE SET mode = $1, disabled_rules = $2, updated_by = $3, updated_at = now()",
                mode.as_str(),
                &rules,
                by,
            )
            .execute(pool)
            .await?;
        }
        Scope::Tenant(t) => {
            sqlx::query!(
                "INSERT INTO waf_policies (scope, tenant_id, mode, disabled_rules, updated_by)
                 VALUES ('tenant', $1, $2, $3, $4)
                 ON CONFLICT (tenant_id) WHERE scope = 'tenant'
                 DO UPDATE SET mode = $2, disabled_rules = $3, updated_by = $4, updated_at = now()",
                t,
                mode.as_str(),
                &rules,
                by,
            )
            .execute(pool)
            .await?;
        }
        Scope::Domain {
            tenant_id,
            domain_id,
        } => {
            sqlx::query!(
                "INSERT INTO waf_policies (scope, tenant_id, domain_id, mode, disabled_rules, updated_by)
                 VALUES ('domain', $1, $2, $3, $4, $5)
                 ON CONFLICT (domain_id) WHERE scope = 'domain'
                 DO UPDATE SET mode = $3, disabled_rules = $4, updated_by = $5, updated_at = now()",
                tenant_id,
                domain_id,
                mode.as_str(),
                &rules,
                by,
            )
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// Removes the policy at a scope (the one above applies again). `false` when there was none.
///
/// # Errors
/// Database errors.
pub async fn clear(pool: &PgPool, scope: Scope) -> Result<bool, sqlx::Error> {
    let done = match scope {
        Scope::Global => {
            sqlx::query!("DELETE FROM waf_policies WHERE scope = 'global'")
                .execute(pool)
                .await?
        }
        Scope::Tenant(t) => {
            sqlx::query!(
                "DELETE FROM waf_policies WHERE scope = 'tenant' AND tenant_id = $1",
                t
            )
            .execute(pool)
            .await?
        }
        Scope::Domain { domain_id, .. } => {
            sqlx::query!(
                "DELETE FROM waf_policies WHERE scope = 'domain' AND domain_id = $1",
                domain_id
            )
            .execute(pool)
            .await?
        }
    };
    Ok(done.rows_affected() == 1)
}

/// What a domain of an account really gets.
///
/// # Errors
/// Database errors.
pub async fn effective(
    pool: &PgPool,
    tenant_id: Uuid,
    domain_id: Uuid,
) -> Result<Effective, sqlx::Error> {
    let global = get(pool, Scope::Global).await?;
    let tenant = get(pool, Scope::Tenant(tenant_id)).await?;
    let domain = get(
        pool,
        Scope::Domain {
            tenant_id,
            domain_id,
        },
    )
    .await?;
    Ok(combine(global.as_ref(), tenant.as_ref(), domain.as_ref()))
}

/// What each of an account's domains gets (three queries, whatever the number of domains).
///
/// # Errors
/// Database errors.
pub async fn for_domains(
    pool: &PgPool,
    tenant_id: Uuid,
    domain_ids: &[Uuid],
) -> Result<std::collections::BTreeMap<Uuid, Effective>, sqlx::Error> {
    let global = get(pool, Scope::Global).await?;
    let tenant = get(pool, Scope::Tenant(tenant_id)).await?;
    let rows = sqlx::query!(
        r#"SELECT domain_id AS "domain_id!", mode, disabled_rules
           FROM waf_policies WHERE scope = 'domain' AND tenant_id = $1"#,
        tenant_id
    )
    .fetch_all(pool)
    .await?;
    let own: std::collections::BTreeMap<Uuid, Policy> = rows
        .into_iter()
        .filter_map(|r| {
            Some((
                r.domain_id,
                Policy {
                    mode: Mode::parse(&r.mode)?,
                    disabled_rules: r.disabled_rules,
                },
            ))
        })
        .collect();
    Ok(domain_ids
        .iter()
        .map(|id| (*id, combine(global.as_ref(), tenant.as_ref(), own.get(id))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(mode: Mode, rules: &[i32]) -> Policy {
        Policy {
            mode,
            disabled_rules: rules.to_vec(),
        }
    }

    #[test]
    fn the_most_specific_mode_wins_and_the_disabled_rules_add_up() {
        let none = combine(None, None, None);
        assert_eq!((none.mode, none.mode_from), (Mode::Detect, "default"));
        let global = p(Mode::Block, &[942100]);
        let g = combine(Some(&global), None, None);
        assert_eq!((g.mode, g.mode_from), (Mode::Block, "global"));
        let tenant = p(Mode::Detect, &[941100, 942100]);
        let t = combine(Some(&global), Some(&tenant), None);
        assert_eq!((t.mode, t.mode_from), (Mode::Detect, "tenant"));
        assert_eq!(t.disabled_rules, [941100, 942100]);
        let domain = p(Mode::Off, &[]);
        let d = combine(Some(&global), Some(&tenant), Some(&domain));
        assert_eq!((d.mode, d.mode_from), (Mode::Off, "domain"));
        assert_eq!(d.disabled_rules, [941100, 942100]);
    }
}
