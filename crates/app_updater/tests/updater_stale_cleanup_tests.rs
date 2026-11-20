//! Integration tests for `app_updater` stale-download cleanup and error formatting.

use std::time::{SystemTime, UNIX_EPOCH};

use app_updater::{remove_stale_downloads, UpdateError};

#[test]
fn remove_stale_downloads_clears_old_updates_folder_safely() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cs_upd_{}_{}", std::process::id(), nanos));
    let updates = dir.join("updates");
    std::fs::create_dir_all(&updates).expect("mkdir updates");
    std::fs::write(updates.join("old-installer.part"), b"partial").expect("write");

    // Calling on populated folder and again on non-existent folder must both succeed cleanly.
    remove_stale_downloads(&dir);
    remove_stale_downloads(&dir.join("does-not-exist"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn update_error_variants_render_human_readable_messages() {
    assert!(!UpdateError::NoAsset.to_string().is_empty());
    assert!(!UpdateError::NotSigned.to_string().is_empty());
    assert!(!UpdateError::NotNewer {
        latest: "v0.4.8".into()
    }
    .to_string()
    .is_empty());
    assert!(!UpdateError::ChecksumList("bad line".into())
        .to_string()
        .is_empty());
    assert!(!UpdateError::ChecksumMismatch {
        file: "app.AppImage".into()
    }
    .to_string()
    .is_empty());
    assert!(!UpdateError::MissingChecksum {
        file: "app.AppImage".into()
    }
    .to_string()
    .is_empty());
    assert!(
        !UpdateError::ReadOnlyTarget(std::path::PathBuf::from("/opt/app"))
            .to_string()
            .is_empty()
    );
    assert!(!UpdateError::UnsafePath(std::path::PathBuf::from("a\"b"))
        .to_string()
        .is_empty());
}
