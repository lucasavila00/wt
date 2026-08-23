use crate::CodexPaths;
use nix::unistd::{Gid, Group, Uid, User};
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub const SERVER_USER: &str = wt_host_world::WT_IDENTITY.user;
pub const SERVER_GROUP: &str = wt_host_world::WT_IDENTITY.group;
pub const SERVER_UID: u32 = wt_host_world::WT_IDENTITY.uid;
pub const SERVER_GID: u32 = wt_host_world::WT_IDENTITY.gid;
pub const SERVER_HOME: &str = wt_host_world::WT_IDENTITY.home;

const RECOVERY: &str = "rebootstrap the WT server account before installing or starting WT";

pub fn validate_process_identity() -> Result<(), String> {
    let uid = Uid::effective();
    let gid = Gid::effective();
    let user = User::from_uid(uid)
        .map_err(|error| format!("look up process user for uid={uid}: {error}"))?;
    let group = Group::from_gid(gid)
        .map_err(|error| format!("look up process group for gid={gid}: {error}"))?;
    validate_identity(Identity {
        user: user.as_ref().map(|user| user.name.as_str()),
        group: group.as_ref().map(|group| group.name.as_str()),
        uid: uid.as_raw(),
        gid: gid.as_raw(),
        primary_gid: user.as_ref().map(|user| user.gid.as_raw()),
        home: user.as_ref().map(|user| user.dir.as_path()),
    })
}

pub fn validate_shared_roots(codex: CodexPaths) -> Result<(), String> {
    for path in [
        Path::new(SERVER_HOME),
        Path::new("/home/wt/.codex"),
        Path::new(codex.sessions),
        Path::new(codex.auth_share),
        Path::new(crate::SSH_AUTHORIZED_KEYS_SHARE_DIR),
    ] {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            format!(
                "inspect WT shared root {}: {error}; expected a non-symlink directory owned by {SERVER_USER}:{SERVER_GROUP} (uid/gid={SERVER_UID}:{SERVER_GID}); {RECOVERY}",
                path.display()
            )
        })?;
        validate_shared_root(path, &metadata)?;
    }
    Ok(())
}

struct Identity<'a> {
    user: Option<&'a str>,
    group: Option<&'a str>,
    uid: u32,
    gid: u32,
    primary_gid: Option<u32>,
    home: Option<&'a Path>,
}

fn validate_identity(actual: Identity<'_>) -> Result<(), String> {
    if actual.user == Some(SERVER_USER)
        && actual.group == Some(SERVER_GROUP)
        && actual.uid == SERVER_UID
        && actual.gid == SERVER_GID
        && actual.primary_gid == Some(SERVER_GID)
        && actual.home == Some(Path::new(SERVER_HOME))
    {
        return Ok(());
    }
    Err(format!(
        "WT server identity mismatch: expected user/group={SERVER_USER}:{SERVER_GROUP} uid/gid={SERVER_UID}:{SERVER_GID} home={SERVER_HOME}; actual user/group={}:{} effective uid/gid={}:{} account primary gid={} home={}; {RECOVERY}",
        actual.user.unwrap_or("<unknown>"),
        actual.group.unwrap_or("<unknown>"),
        actual.uid,
        actual.gid,
        actual
            .primary_gid
            .map_or_else(|| "<unknown>".to_owned(), |gid| gid.to_string()),
        actual
            .home
            .map_or_else(|| "<unknown>".to_owned(), |home| home.display().to_string()),
    ))
}

fn validate_shared_root(path: &Path, metadata: &Metadata) -> Result<(), String> {
    validate_shared_root_details(
        path,
        metadata_kind(metadata),
        metadata.uid(),
        metadata.gid(),
        metadata.mode() & 0o7777,
    )
}

fn metadata_kind(metadata: &Metadata) -> &'static str {
    if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    }
}

fn validate_shared_root_details(
    path: &Path,
    kind: &str,
    uid: u32,
    gid: u32,
    mode: u32,
) -> Result<(), String> {
    if kind != "directory" || uid != SERVER_UID || gid != SERVER_GID {
        return Err(format!(
            "WT shared root identity mismatch at {}: expected non-symlink directory owned by {SERVER_USER}:{SERVER_GROUP} (uid/gid={SERVER_UID}:{SERVER_GID}); actual type={kind} uid/gid={}:{}; {RECOVERY}",
            path.display(),
            uid,
            gid,
        ));
    }
    if shared_root_mode(path).is_some_and(|expected| mode != expected) {
        return Err(format!(
            "WT shared root mode mismatch at {}: expected mode=0700; actual mode={mode:04o}; {RECOVERY}",
            path.display(),
        ));
    }
    Ok(())
}

fn shared_root_mode(path: &Path) -> Option<u32> {
    path.file_name()
        .is_some_and(|name| name == "sessions" || name == "auth-share")
        .then_some(0o700)
        .or_else(|| {
            (path == Path::new(crate::CODEX_AUTH_SHARE_DIR)
                || path == Path::new(crate::SSH_AUTHORIZED_KEYS_SHARE_DIR))
            .then_some(0o700)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_canonical_identity() {
        assert_eq!(
            validate_identity(Identity {
                user: Some("wt"),
                group: Some("wt"),
                uid: 1001,
                gid: 1001,
                primary_gid: Some(1001),
                home: Some(Path::new("/home/wt")),
            }),
            Ok(())
        );
    }

    #[test]
    fn identity_mismatch_reports_expected_and_actual_values() {
        insta::assert_snapshot!(
            validate_identity(Identity {
                user: Some("wt"),
                group: Some("wt"),
                uid: 1000,
                gid: 1000,
                primary_gid: Some(1000),
                home: Some(Path::new("/srv/wt")),
            })
            .unwrap_err(),
            @"WT server identity mismatch: expected user/group=wt:wt uid/gid=1001:1001 home=/home/wt; actual user/group=wt:wt effective uid/gid=1000:1000 account primary gid=1000 home=/srv/wt; rebootstrap the WT server account before installing or starting WT"
        );
    }

    #[test]
    fn account_name_mismatch_reports_expected_and_actual_values() {
        insta::assert_snapshot!(
            validate_identity(Identity {
                user: Some("builder"),
                group: Some("builders"),
                uid: 1001,
                gid: 1001,
                primary_gid: Some(1001),
                home: Some(Path::new("/home/wt")),
            })
            .unwrap_err(),
            @"WT server identity mismatch: expected user/group=wt:wt uid/gid=1001:1001 home=/home/wt; actual user/group=builder:builders effective uid/gid=1001:1001 account primary gid=1001 home=/home/wt; rebootstrap the WT server account before installing or starting WT"
        );
    }

    #[test]
    fn shared_root_mismatch_reports_expected_and_actual_values() {
        insta::assert_snapshot!(
            validate_shared_root_details(
                Path::new("/home/wt/.codex/sessions"),
                "directory",
                1000,
                1000,
                0o700,
            )
            .unwrap_err(),
            @"WT shared root identity mismatch at /home/wt/.codex/sessions: expected non-symlink directory owned by wt:wt (uid/gid=1001:1001); actual type=directory uid/gid=1000:1000; rebootstrap the WT server account before installing or starting WT"
        );
    }

    #[test]
    fn private_shared_root_mode_is_enforced() {
        insta::assert_snapshot!(
            validate_shared_root_details(
                Path::new("/home/wt/.codex/sessions"),
                "directory",
                1001,
                1001,
                0o750,
            )
            .unwrap_err(),
            @"WT shared root mode mismatch at /home/wt/.codex/sessions: expected mode=0700; actual mode=0750; rebootstrap the WT server account before installing or starting WT"
        );
    }

    #[test]
    fn shared_roots_cover_the_host_guest_owner_boundary() {
        assert_eq!(
            [
                SERVER_HOME,
                "/home/wt/.codex",
                crate::CODEX_SESSIONS_PATH,
                crate::CODEX_AUTH_SHARE_DIR,
                crate::SSH_AUTHORIZED_KEYS_SHARE_DIR,
            ],
            [
                "/home/wt",
                "/home/wt/.codex",
                "/home/wt/.codex/sessions",
                "/home/wt/.codex/.wt-auth",
                "/home/wt/.ssh/.wt-authorized-keys",
            ]
        );
    }
}
