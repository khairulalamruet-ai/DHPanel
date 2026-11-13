//! Tenant directories (T04.07): the one thing about tenants that needs root.
//!
//! Containers run in the agent's rootless user namespace, where tenant id `N`
//! (10000 and up) is the host id `subuid_start + N - 1` of the agent user's
//! range in `/etc/subuid`. A tenant's files therefore have to be owned by that
//! host id. Only root can give files to such an id, so the agent asks the helper.
//!
//! What this module does, and nothing else: create `<root>/<tenant id>` and
//! `<root>/<tenant id>/www` (mode 0750, owner the tenant's host id, group the
//! agent). The path comes only from a validated UUID, so no request can point it
//! anywhere else; a symlink or anything that is not a directory in the way is a
//! failure, never followed. Doing it twice is fine; a directory that exists with
//! another owner is a failure (it would mean two tenants share an id).

use std::{
    fs, io,
    os::unix::fs::{MetadataExt as _, PermissionsExt as _},
    path::PathBuf,
};

use crate::protocol::ErrorCode;

/// First and one-past-last tenant id (kept in step with `dhpanel_domain::UidRange::DEFAULT`).
pub const UID_START: u32 = 10_000;
/// See [`UID_START`].
pub const UID_END: u32 = 60_000;

/// Where tenants live and how ids map to the host.
#[derive(Debug, Clone)]
pub struct TenantEnv {
    /// The directory holding every tenant's directory.
    pub root: PathBuf,
    /// The agent user's name (to find its range in `subuid_file`).
    pub agent_name: String,
    /// `/etc/subuid`.
    pub subuid_file: PathBuf,
    /// The group tenant directories get (the agent's).
    pub agent_gid: u32,
}

/// What is wrong, in a form that goes back to the agent and into the log.
pub type Failure = (ErrorCode, &'static str);

/// Whether `id` is a lower-case UUID, the only shape allowed in a path.
#[must_use]
pub fn valid_tenant_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

/// The agent's range in a `subuid` file: `(start, count)`.
#[must_use]
pub fn subuid_range(text: &str, agent: &str) -> Option<(u32, u32)> {
    text.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == agent).then_some(())?;
        Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
    })
}

/// The host id of tenant id `uid`.
///
/// # Errors
/// The id is outside the tenant range, or the agent's subordinate range does
/// not reach that far.
pub fn host_uid(range: (u32, u32), uid: u32) -> Result<u32, Failure> {
    if !(UID_START..UID_END).contains(&uid) {
        return Err((
            ErrorCode::InvalidArguments,
            "the user id is outside the tenant range",
        ));
    }
    let (start, count) = range;
    let offset = uid - 1;
    if offset >= count {
        return Err((
            ErrorCode::Failed,
            "the agent's subordinate id range is too small",
        ));
    }
    start
        .checked_add(offset)
        .ok_or((ErrorCode::Failed, "the subordinate id range is invalid"))
}

/// Creates the tenant's directories.
///
/// # Errors
/// [`Failure`] with a fixed message.
pub fn prepare(env: &TenantEnv, tenant_id: &str, uid: u32) -> Result<(), Failure> {
    if !valid_tenant_id(tenant_id) {
        return Err((ErrorCode::InvalidArguments, "the tenant id is not a UUID"));
    }
    // Checked before any file is read: a bad request costs nothing.
    if !(UID_START..UID_END).contains(&uid) {
        return Err((
            ErrorCode::InvalidArguments,
            "the user id is outside the tenant range",
        ));
    }
    let subuid = fs::read_to_string(&env.subuid_file)
        .map_err(|_| (ErrorCode::Failed, "cannot read the subordinate id file"))?;
    let range = subuid_range(&subuid, &env.agent_name)
        .ok_or((ErrorCode::Failed, "the agent has no subordinate id range"))?;
    let owner = host_uid(range, uid)?;
    prepare_as(env, tenant_id, owner)
}

/// The filesystem part, with the owner already worked out.
///
/// The helper holds only `CAP_CHOWN`: once a directory belongs to the tenant,
/// the helper (not the owner, not in the group) can no longer look inside it.
/// So everything inside is made first, the inner directory is handed over
/// before the outer one, and "the tenant directory belongs to the tenant" means
/// "this was finished". A directory still owned by the helper's own user is an
/// earlier attempt that stopped half way, and is finished now.
///
/// # Errors
/// [`Failure`].
pub fn prepare_as(env: &TenantEnv, tenant_id: &str, owner: u32) -> Result<(), Failure> {
    let failed = |_: io::Error| (ErrorCode::Failed, "cannot create the tenant directory");
    let clash = (
        ErrorCode::Failed,
        "the tenant directory exists with a different owner or type",
    );
    // The parent exists and is the helper's own: created here if missing, never
    // inside a directory anybody else controls.
    match fs::symlink_metadata(&env.root) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => return Err(clash),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(&env.root).map_err(failed)?;
            fs::set_permissions(&env.root, fs::Permissions::from_mode(0o755)).map_err(failed)?;
        }
        Err(err) => return Err(failed(err)),
    }
    let me = rustix::process::geteuid().as_raw();
    let tenant = env.root.join(tenant_id);
    let www = tenant.join("www");
    match fs::symlink_metadata(&tenant) {
        // Finished before: owned by the tenant (and so, by construction, complete).
        Ok(meta)
            if meta.file_type().is_dir() && meta.uid() == owner && meta.gid() == env.agent_gid =>
        {
            return Ok(());
        }
        // Started before and stopped: still ours. Finish it below.
        Ok(meta) if meta.file_type().is_dir() && meta.uid() == me => {}
        Ok(_) => return Err(clash),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&tenant).map_err(failed)?;
        }
        Err(err) => return Err(failed(err)),
    }
    // Inside first, while the helper can still reach it.
    match fs::symlink_metadata(&www) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => return Err(clash),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&www).map_err(failed)?
        }
        Err(err) => return Err(failed(err)),
    }
    for dir in [&www, &tenant] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o750)).map_err(failed)?;
    }
    // Hand over: inner first, outer last (the outer one is the "finished" mark).
    for dir in [&www, &tenant] {
        std::os::unix::fs::chown(dir, Some(owner), Some(env.agent_gid)).map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const ID: &str = "0190b6a2-0000-7000-8000-000000000001";

    #[test]
    fn ids_are_uuids_and_nothing_else() {
        assert!(valid_tenant_id(ID));
        for bad in [
            "",
            "..",
            "../etc",
            "0190B6A2-0000-7000-8000-000000000001",
            "0190b6a2-0000-7000-8000-00000000000",
            "0190b6a2_0000_7000_8000_000000000001",
            "0190b6a2-0000-7000-8000-00000000000g",
            "0190b6a2-0000-7000-8000-000000000001/x",
            "0190b6a2-0000-7000-8000-000000000001\n",
        ] {
            assert!(!valid_tenant_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn tenant_ids_map_into_the_agents_subordinate_range() {
        let text = "dev:100000:65536\ndhpanel-agent:165536:65536\n";
        let range = subuid_range(text, "dhpanel-agent").unwrap();
        assert_eq!(range, (165_536, 65_536));
        assert_eq!(host_uid(range, 10_000).unwrap(), 165_536 + 9_999);
        assert_eq!(host_uid(range, 59_999).unwrap(), 165_536 + 59_998);
        for bad in [0, 1, 9_999, 60_000, 65_534] {
            assert_eq!(
                host_uid(range, bad).unwrap_err().0,
                ErrorCode::InvalidArguments,
                "{bad}"
            );
        }
        // A range that is too small, or none at all.
        assert_eq!(
            host_uid((165_536, 20_000), 30_000).unwrap_err().0,
            ErrorCode::Failed
        );
        assert_eq!(subuid_range(text, "nobody"), None);
        assert_eq!(subuid_range("dhpanel-agent:x:y", "dhpanel-agent"), None);
        assert_eq!(
            subuid_range("dhpanel-agent-two:1:2", "dhpanel-agent"),
            None,
            "whole names only"
        );
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dhph-tenants-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn me() -> (u32, u32) {
        let meta = fs::metadata(std::env::temp_dir()).unwrap();
        let probe = std::env::temp_dir().join(format!("dhph-who-{}", std::process::id()));
        fs::write(&probe, b"").unwrap();
        let m = fs::metadata(&probe).unwrap();
        let _ = fs::remove_file(&probe);
        let _ = meta;
        (m.uid(), m.gid())
    }

    #[test]
    fn directories_are_created_private_owned_and_the_second_time_is_fine() {
        let root = scratch("create");
        let (uid, gid) = me();
        let env = TenantEnv {
            root: root.clone(),
            agent_name: "agent".into(),
            subuid_file: "/nonexistent".into(),
            agent_gid: gid,
        };
        prepare_as(&env, ID, uid).unwrap();
        for path in [root.join(ID), root.join(ID).join("www")] {
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(meta.is_dir());
            assert_eq!(meta.permissions().mode() & 0o777, 0o750, "{path:?}");
            assert_eq!((meta.uid(), meta.gid()), (uid, gid));
        }
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o755
        );
        prepare_as(&env, ID, uid).unwrap(); // again: nothing to do
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_symlink_a_file_or_a_foreign_owner_in_the_way_is_a_failure() {
        let root = scratch("blocked");
        let (uid, gid) = me();
        let env = TenantEnv {
            root: root.clone(),
            agent_name: "agent".into(),
            subuid_file: "/nonexistent".into(),
            agent_gid: gid,
        };
        fs::create_dir_all(&root).unwrap();
        // A symlink where the tenant directory should be: never followed.
        let target = scratch("elsewhere");
        fs::create_dir_all(&target).unwrap();
        std::os::unix::fs::symlink(&target, root.join(ID)).unwrap();
        let err = prepare_as(&env, ID, uid).unwrap_err();
        assert_eq!(err.0, ErrorCode::Failed);
        assert!(
            fs::read_dir(&target).unwrap().next().is_none(),
            "nothing was created behind the link"
        );
        fs::remove_file(root.join(ID)).unwrap();
        // A plain file.
        fs::write(root.join(ID), b"x").unwrap();
        assert_eq!(prepare_as(&env, ID, uid).unwrap_err().0, ErrorCode::Failed);
        fs::remove_file(root.join(ID)).unwrap();
        // The right directory but owned by someone else (when we are not that someone).
        prepare_as(&env, ID, uid).unwrap();
        let other = if uid == 0 { 1 } else { uid.wrapping_add(1) };
        assert!(
            prepare_as(&env, ID, other).is_err(),
            "a directory owned by another id is not accepted"
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    fn requests_with_bad_arguments_never_touch_the_disk() {
        let root = scratch("bad");
        let env = TenantEnv {
            root: root.clone(),
            agent_name: "dhpanel-agent".into(),
            subuid_file: "/nonexistent".into(),
            agent_gid: 0,
        };
        assert_eq!(
            prepare(&env, "../../etc", 10_000).unwrap_err().0,
            ErrorCode::InvalidArguments
        );
        assert_eq!(
            prepare(&env, ID, 0).unwrap_err().0,
            ErrorCode::InvalidArguments
        );
        assert!(!root.exists());
        // A good request that cannot read the subuid file fails clearly.
        assert_eq!(prepare(&env, ID, 10_000).unwrap_err().0, ErrorCode::Failed);
        assert!(!root.exists());
    }
}
