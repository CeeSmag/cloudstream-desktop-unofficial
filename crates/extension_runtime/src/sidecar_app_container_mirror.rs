//! Mirrors JDK / sidecar trees into the data dir when AppContainer ACLs cannot be set.
//!
//! `icacls` needs write-DAC on the target. Installs under Program Files deny that for a
//! normal user, so we copy into `<data>/appcontainer/...` (already granted modify) instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use extension_api::GatewayError;

use crate::sidecar_app_container_acl::grant_read_execute;
use crate::sidecar_app_container_profile::AppContainerSid;

static MIRROR_LOCK: Mutex<()> = Mutex::new(());
static WORK_SEQ: AtomicU64 = AtomicU64::new(0);

const STAMP_NAME: &str = ".cs_mirror_source";

/// Returns `source` when the AppContainer can be granted RX on it; otherwise a mirror under `staging`.
pub(crate) fn ensure_readable(
    source: &Path,
    sid: &AppContainerSid,
    staging: &Path,
) -> Result<PathBuf, GatewayError> {
    ensure_readable_with(source, sid, staging, grant_read_execute)
}

fn ensure_readable_with<F>(
    source: &Path,
    sid: &AppContainerSid,
    staging: &Path,
    grant: F,
) -> Result<PathBuf, GatewayError>
where
    F: FnOnce(&Path, &AppContainerSid) -> Result<(), GatewayError>,
{
    if grant(source, sid).is_ok() {
        return Ok(source.to_path_buf());
    }
    tracing::warn!(
        path = %source.display(),
        staging = %staging.display(),
        "cannot ACL path for AppContainer; mirroring into data dir"
    );
    let _guard = MIRROR_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    mirror_tree(source, staging)?;
    Ok(staging.to_path_buf())
}

fn mirror_tree(source: &Path, staging: &Path) -> Result<(), GatewayError> {
    if !source.is_dir() {
        return Err(GatewayError::Unavailable(format!(
            "cannot mirror AppContainer path; not a directory: {}",
            source.display()
        )));
    }
    let stamp = source_stamp(source)?;
    if stamp_matches(staging, &stamp) {
        return Ok(());
    }
    let parent = staging_parent(staging)?;
    fs::create_dir_all(&parent).map_err(|error| io_error(&parent, error))?;
    let work = work_dir(&parent);
    let _ = fs::remove_dir_all(&work);
    copy_dir(source, &work)?;
    fs::write(work.join(STAMP_NAME), &stamp).map_err(|error| io_error(&work, error))?;
    replace_staging(staging, &work)?;
    tracing::info!(
        source = %source.display(),
        staging = %staging.display(),
        "mirrored tree for AppContainer"
    );
    Ok(())
}

fn staging_parent(staging: &Path) -> Result<PathBuf, GatewayError> {
    staging
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| GatewayError::Unavailable("AppContainer staging path has no parent".into()))
}

fn work_dir(parent: &Path) -> PathBuf {
    let seq = WORK_SEQ.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".mirror_work_{}_{seq}", std::process::id()))
}

fn stamp_matches(staging: &Path, stamp: &str) -> bool {
    fs::read_to_string(staging.join(STAMP_NAME))
        .ok()
        .is_some_and(|existing| existing == stamp)
}

fn replace_staging(staging: &Path, work: &Path) -> Result<(), GatewayError> {
    let _ = fs::remove_dir_all(staging);
    match fs::rename(work, staging) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(work);
            Err(io_error(staging, error))
        }
    }
}

fn source_stamp(source: &Path) -> Result<String, GatewayError> {
    let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let sentinel = {
        let java = source.join("bin").join("java.exe");
        if java.is_file() {
            java
        } else {
            source.to_path_buf()
        }
    };
    let meta = fs::metadata(&sentinel).map_err(|error| io_error(&sentinel, error))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    Ok(format!("{}|{}|{modified}", canonical.display(), meta.len()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), GatewayError> {
    fs::create_dir_all(to).map_err(|error| io_error(to, error))?;
    let entries = fs::read_dir(from).map_err(|error| io_error(from, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error(from, error))?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| io_error(&src, error))?;
        if file_type.is_dir() {
            copy_dir(&src, &dest)?;
        } else if file_type.is_file() {
            fs::copy(&src, &dest).map_err(|error| io_error(&src, error))?;
        }
    }
    Ok(())
}

fn io_error(path: &Path, error: std::io::Error) -> GatewayError {
    GatewayError::Unavailable(format!(
        "AppContainer mirror failed for {}: {error}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar_app_container_profile::get_or_create_sidecar_container;
    use std::sync::Arc;
    use std::thread;

    fn temp_pair(label: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("cs_ac_mirror_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("source");
        let staging = root.join("staging");
        fs::create_dir_all(source.join("bin")).expect("source");
        fs::write(source.join("bin").join("java.exe"), b"fake").expect("java");
        fs::write(source.join("readme.txt"), b"ok").expect("readme");
        (source, staging)
    }

    #[test]
    fn ensure_readable_returns_source_when_grant_succeeds() {
        let sid = get_or_create_sidecar_container().expect("sid");
        let (source, staging) = temp_pair("grant_ok");
        let path = ensure_readable(&source, &sid, &staging).expect("readable");
        assert_eq!(path, source);
        assert!(!staging.exists());
        let _ = fs::remove_dir_all(source.parent().expect("root"));
    }

    #[test]
    fn ensure_readable_mirrors_when_grant_fails() {
        let sid = get_or_create_sidecar_container().expect("sid");
        let (source, staging) = temp_pair("grant_fail");
        let path = ensure_readable_with(&source, &sid, &staging, |_path, _sid| {
            Err(GatewayError::Unavailable("denied".into()))
        })
        .expect("mirror");
        assert_eq!(path, staging);
        assert_eq!(
            fs::read(staging.join("bin").join("java.exe")).expect("java"),
            b"fake"
        );
        let again = ensure_readable_with(&source, &sid, &staging, |_path, _sid| {
            Err(GatewayError::Unavailable("denied".into()))
        })
        .expect("reuse");
        assert_eq!(again, staging);
        let _ = fs::remove_dir_all(source.parent().expect("root"));
    }

    #[test]
    fn mirror_race_keeps_a_complete_tree() {
        let sid = Arc::new(get_or_create_sidecar_container().expect("sid"));
        let (source, staging) = temp_pair("race");
        let source = Arc::new(source);
        let staging = Arc::new(staging);
        let mut handles = Vec::new();
        for _ in 0..8 {
            let sid = Arc::clone(&sid);
            let source = Arc::clone(&source);
            let staging = Arc::clone(&staging);
            handles.push(thread::spawn(move || {
                ensure_readable_with(source.as_ref(), sid.as_ref(), staging.as_ref(), |_, _| {
                    Err(GatewayError::Unavailable("denied".into()))
                })
            }));
        }
        for handle in handles {
            let path = handle.join().expect("thread").expect("mirror");
            assert_eq!(path, *staging);
        }
        assert!(staging.join("bin").join("java.exe").is_file());
        assert!(staging.join("readme.txt").is_file());
        let _ = fs::remove_dir_all(source.parent().expect("root"));
    }
}
