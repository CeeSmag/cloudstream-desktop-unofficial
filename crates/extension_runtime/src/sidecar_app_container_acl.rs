//! Grants the AppContainer SID access to the folders the sidecar must read or write.

use std::path::Path;
use std::process::{Command, Stdio};

use extension_api::GatewayError;

use crate::sidecar_app_container_profile::AppContainerSid;

/// Windows flag that stops a console window from flashing for `icacls`.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Grants modify rights on `path` (and children) to the AppContainer.
pub(crate) fn grant_modify(path: &Path, sid: &AppContainerSid) -> Result<(), GatewayError> {
    grant(path, sid, "(OI)(CI)(M)")
}

/// Grants read+execute rights on `path` (and children) to the AppContainer.
pub(crate) fn grant_read_execute(path: &Path, sid: &AppContainerSid) -> Result<(), GatewayError> {
    grant(path, sid, "(OI)(CI)(RX)")
}

fn grant(path: &Path, sid: &AppContainerSid, rights: &str) -> Result<(), GatewayError> {
    if !path.exists() {
        return Err(GatewayError::Unavailable(format!(
            "cannot grant AppContainer access; path missing: {}",
            path.display()
        )));
    }
    let key =
        crate::acl_session_cache::grant_key(&path.display().to_string(), sid.as_str(), rights);
    if crate::acl_session_cache::already_granted(&key) {
        tracing::debug!(
            path = %path.display(),
            rights,
            "AppContainer ACL already granted this session"
        );
        return Ok(());
    }
    let grant_spec = format!("*{}:{rights}", sid.as_str());
    let mut command = Command::new("icacls");
    command
        .arg(path)
        .args(["/grant", &grant_spec])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().map_err(|source| {
        GatewayError::Unavailable(format!("cannot run icacls for AppContainer ACL: {source}"))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GatewayError::Unavailable(format!(
            "icacls failed for {}: {stderr}",
            path.display()
        )));
    }
    crate::acl_session_cache::note_granted(key);
    tracing::info!(path = %path.display(), rights, "granted AppContainer ACL");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar_app_container_profile::get_or_create_sidecar_container;
    use std::fs;

    #[test]
    fn grant_modify_accepts_a_temp_directory() {
        let sid = get_or_create_sidecar_container().expect("sid");
        let dir = std::env::temp_dir().join(format!("cs_ac_acl_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        grant_modify(&dir, &sid).expect("grant");
        let _ = fs::remove_dir_all(&dir);
    }
}
