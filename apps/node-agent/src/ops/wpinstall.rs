//! WordPress installs (T12.02): WP-CLI in a container of its own.
//!
//! The install runs in a one-off container made from the pinned `wpcli` image (PHP with
//! mysqli, gd and the rest, so the tenant's own PHP image does not have to have them at that
//! moment). The container runs as the tenant's own user id, on the tenant's network (it
//! reaches the database the way the site will), with the tenant's files mounted and nothing
//! else. The two passwords are not in the command line or the environment: they are two small
//! files in a folder of the agent's state that is mounted read-only for the run and removed
//! afterwards. The subcommands are the fixed ones below; what a caller supplies is checked
//! to be plain text first.

use std::{path::Path, sync::Arc, time::Duration};

use dhpanel_docker_manager::{Network, Tmpfs, VolumeMount};
use dhpanel_domain::TenantUsername;
use dhpanel_node_protocol::{AgentError, ErrorCode, Operation, Output};
use dhpanel_tenant_stack::{Allowlist, Role, files_volume_name, network_name};

use super::{
    db,
    scan::{base_spec, state},
    sqlimport::failed,
    tenant::{TenantContext, docker_error},
};

const LIMIT: Duration = Duration::from_secs(5 * 60);

/// What the container runs. Every value arrives in an environment variable, so nothing is
/// ever put into the script text.
const SCRIPT: &str = r#"
set -eu
W="php -d memory_limit=512M /usr/local/bin/wp --path=$WP_PATH $WP_EXTRA"
if [ -e "$WP_PATH/wp-config.php" ] || [ -e "$WP_PATH/wp-load.php" ]; then
  echo "WordPress is in that folder already" >&2
  exit 3
fi
mkdir -p "$WP_PATH"
set -- core download
[ -z "$WP_VERSION" ] || set -- "$@" --version="$WP_VERSION"
[ -z "$WP_LOCALE" ] || set -- "$@" --locale="$WP_LOCALE"
$W "$@"
$W config create --dbname="$DB_NAME" --dbuser="$DB_USER" --dbhost="$DB_HOST" --dbpass="$(cat /run/dh/dbpass)" --skip-salts
$W config shuffle-salts
$W core install --url="$SITE_URL" --title="$SITE_TITLE" --admin_user="$ADMIN_USER" --admin_password="$(cat /run/dh/adminpass)" --admin_email="$ADMIN_EMAIL" --skip-email
for plugin in $PLUGINS; do
  $W plugin install "$plugin" --activate
done
echo "DH_VERSION=$($W core version)"
"#;

fn invalid(what: &str) -> AgentError {
    AgentError::new(ErrorCode::InvalidArguments, what.to_owned())
}

fn plain(text: &str, max: usize, extra: &str) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// Whether `url` is a plain http(s) address with nothing a shell or WordPress would trip on.
fn plain_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 300
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&()*+,;=%".contains(c))
}

/// A folder of the tenant's files, relative to `www`; empty for `www` itself.
fn plain_folder(folder: &str) -> bool {
    folder.is_empty()
        || (folder.len() <= 200
            && !folder.starts_with('/')
            && folder.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            }))
}

/// `install_wordpress`.
///
/// # Errors
/// [`AgentError`]: bad arguments (not retryable), a folder that has WordPress already,
/// Docker or the image failing (retryable), WP-CLI ending badly (the last of what it said,
/// without the passwords).
pub async fn install_wordpress(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let uid = match &op {
        Operation::InstallWordPress { uid, .. } => *uid,
        _ => return Err(super::wrong_operation()),
    };
    install_as(context, op, (uid, uid)).await
}

/// [`install_wordpress`] running as the given user (a test passes another one).
///
/// # Errors
/// As [`install_wordpress`].
pub async fn install_as(
    context: Arc<TenantContext>,
    op: Operation,
    run_as: (u32, u32),
) -> Result<Output, AgentError> {
    use std::os::unix::fs::PermissionsExt as _;
    let Operation::InstallWordPress {
        app_id,
        tenant_id,
        username,
        uid: _,
        path,
        database,
        database_password,
        site,
        admin_password,
        version,
        locale,
        plugins,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let (Some(database_password), Some(admin_password)) = (database_password, admin_password)
    else {
        return Err(invalid("the passwords are missing"));
    };
    let username = TenantUsername::parse(&username).map_err(|e| invalid(&e.to_string()))?;
    if !plain_folder(&path) {
        return Err(invalid("the folder must stay inside the tenant's files"));
    }
    if !plain(&database.name, 64, "_")
        || !plain(&database.user, 64, "_")
        || database_password.0.is_empty()
        || database_password.0.len() > 128
        || database_password.0.contains(['\0', '\n', '\r'])
        || admin_password.0.len() < 12
        || admin_password.0.len() > 128
        || admin_password.0.contains(['\0', '\n', '\r'])
    {
        return Err(invalid("a database name, user or password is not valid"));
    }
    if !plain_url(&site.url)
        || site.title.is_empty()
        || site.title.chars().count() > 100
        || site.title.chars().any(char::is_control)
        || !plain(&site.admin_user, 60, "_.-")
        || site.admin_user.len() < 3
        || !site.admin_email.contains('@')
        || site.admin_email.len() > 200
        || !site
            .admin_email
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c))
    {
        return Err(invalid(
            "the site's address, title or administrator is not valid",
        ));
    }
    if version
        .as_deref()
        .is_some_and(|v| !(plain(v, 20, ".") && v.starts_with(|c: char| c.is_ascii_digit())))
        || locale.as_deref().is_some_and(|l| !plain(l, 10, "_"))
        || plugins.len() > 10
        || plugins.iter().any(|p| !plain(p, 100, "-"))
    {
        return Err(invalid("the version, locale or a plugin name is not valid"));
    }
    let allowlist = Allowlist::embedded().map_err(|e| failed(&format!("the image list: {e}")))?;
    let image = allowlist
        .current(Role::Wpcli)
        .map_err(|e| failed(&e.to_string()))?
        .clone();
    context
        .docker
        .pull_image(&image)
        .await
        .map_err(|e| docker_error(&e))?;
    // The two passwords, as files for this run only.
    let short = app_id.simple().to_string();
    let short = &short[..12];
    let state_dir = state(&context)?;
    let secrets = state_dir.join("wpcli").join(short);
    let cleanup = |dir: &Path| {
        let _ = std::fs::remove_dir_all(dir);
    };
    cleanup(&secrets);
    std::fs::create_dir_all(&secrets)
        .map_err(|e| failed(&format!("cannot make the folder: {e}")))?;
    let written = (|| -> std::io::Result<()> {
        std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o755))?;
        for (name, text) in [
            ("dbpass", &database_password.0),
            ("adminpass", &admin_password.0),
        ] {
            let file = secrets.join(name);
            std::fs::write(&file, text)?;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        cleanup(&secrets);
        return Err(failed(&format!("cannot write the secrets: {}", e.kind())));
    }
    let outcome = run(
        &context,
        &username,
        tenant_id,
        &secrets,
        short,
        image,
        run_as,
        &Settings {
            path: &path,
            database: &database,
            site: &site,
            version: version.as_deref().unwrap_or(""),
            locale: locale.as_deref().unwrap_or(""),
            plugins: &plugins.join(" "),
        },
    )
    .await;
    cleanup(&secrets);
    let _ = context
        .docker
        .remove_volume(&db::name(&format!("dh-wpi-{short}"))?)
        .await;
    let done = outcome?;
    let scrub = |text: &str| {
        let mut out = text.to_owned();
        for secret in [&database_password.0, &admin_password.0] {
            out = out.replace(secret.as_str(), "***");
        }
        out
    };
    if done.exit_code != 0 {
        let tail: String = scrub(&done.output)
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ")
            .chars()
            .filter(|c| !c.is_control())
            .take(400)
            .collect();
        let code = if done.exit_code == 3 {
            ErrorCode::InvalidArguments
        } else {
            ErrorCode::Internal
        };
        return Err(AgentError::new(
            code,
            format!("WP-CLI ended with {}: {tail}", done.exit_code),
        ));
    }
    let version = done
        .output
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("DH_VERSION="))
        .unwrap_or("")
        .trim()
        .to_owned();
    Ok(Output::WordPressInstalled { version })
}

struct Settings<'a> {
    path: &'a str,
    database: &'a dhpanel_node_protocol::WpDatabase,
    site: &'a dhpanel_node_protocol::WpSite,
    version: &'a str,
    locale: &'a str,
    plugins: &'a str,
}

#[allow(clippy::too_many_arguments)]
async fn run(
    context: &TenantContext,
    username: &TenantUsername,
    tenant_id: uuid::Uuid,
    secrets: &Path,
    short: &str,
    image: dhpanel_docker_manager::ImageRef,
    run_as: (u32, u32),
    s: &Settings<'_>,
) -> Result<dhpanel_docker_manager::CommandResult, AgentError> {
    let _ = tenant_id;
    let state_dir = state(context)?;
    let secrets_volume = db::name(&format!("dh-wpi-{short}"))?;
    context
        .docker
        .create_bind_volume(&secrets_volume, secrets, &state_dir)
        .await
        .map_err(|e| docker_error(&e))?;
    let files = files_volume_name(username).map_err(|e| invalid(&e.to_string()))?;
    let network = network_name(username).map_err(|e| invalid(&e.to_string()))?;
    let mut spec = base_spec(
        db::name(&format!("dh-wpi-{short}"))?,
        image,
        &["sh", "-c", SCRIPT],
    );
    spec.cap_add = Vec::new();
    spec.user = Some(run_as);
    spec.memory_bytes = 1024 * 1024 * 1024;
    spec.pids_limit = 128;
    spec.network = Network::Named(network);
    spec.tmpfs = vec![Tmpfs {
        target: "/tmp".into(),
        size_bytes: 256 * 1024 * 1024,
    }];
    spec.volumes = vec![
        VolumeMount {
            volume: files,
            target: "/var/www/html".into(),
            read_only: false,
        },
        VolumeMount {
            volume: secrets_volume,
            target: "/run/dh".into(),
            read_only: true,
        },
    ];
    let folder = if s.path.is_empty() {
        "/var/www/html".to_owned()
    } else {
        format!("/var/www/html/{}", s.path)
    };
    let env = [
        ("HOME", "/tmp"),
        ("WP_CLI_CACHE_DIR", "/tmp/wp-cache"),
        ("WP_PATH", folder.as_str()),
        ("WP_EXTRA", if run_as.0 == 0 { "--allow-root" } else { "" }),
        ("WP_VERSION", s.version),
        ("WP_LOCALE", s.locale),
        ("DB_NAME", s.database.name.as_str()),
        ("DB_USER", s.database.user.as_str()),
        ("DB_HOST", "db"),
        ("SITE_URL", s.site.url.as_str()),
        ("SITE_TITLE", s.site.title.as_str()),
        ("ADMIN_USER", s.site.admin_user.as_str()),
        ("ADMIN_EMAIL", s.site.admin_email.as_str()),
        ("PLUGINS", s.plugins),
    ];
    spec.env = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let _ = context.docker.remove(&spec.name, true).await;
    context
        .docker
        .run_to_end(&spec, LIMIT)
        .await
        .map_err(|e| docker_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_values_pass() {
        assert!(plain_folder("") && plain_folder("blog") && plain_folder("a/b-c_d.e"));
        for bad in ["/x", "../x", "a/../b", "a//b", "a b", "a;b"] {
            assert!(!plain_folder(bad), "{bad}");
        }
        assert!(plain_url("https://example.com/blog"));
        for bad in [
            "ftp://x",
            "https://a b",
            "https://x/\"y\"",
            "https://x/`y`",
            "javascript:alert(1)",
        ] {
            assert!(!plain_url(bad), "{bad}");
        }
    }
}
