//! Landlock allow-list proof tests (separate file so the FFI module stays ≤300 lines).
#![allow(unsafe_code)]
#![allow(clippy::expect_used, clippy::print_stderr, clippy::too_many_lines)]

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::sidecar_linux_landlock::{abi_version, restrict_in_child};
use crate::sidecar_linux_paths::SandboxPaths;

fn make_paths(root: &Path) -> SandboxPaths {
    let java_home = root.join("jre");
    let sidecar_home = root.join("cs-sidecar");
    let data_dir = root.join("data").join("sidecar");
    let extensions_dir = root.join("data").join("extensions");
    for dir in [
        java_home.join("bin"),
        sidecar_home.join("bin"),
        data_dir.clone(),
        extensions_dir.clone(),
    ] {
        fs::create_dir_all(dir).expect("mkdir");
    }
    fs::write(java_home.join("bin").join("java"), b"x").expect("java");
    fs::write(data_dir.join("allowed.txt"), b"ok").expect("allowed");
    SandboxPaths {
        java_home,
        sidecar_home,
        data_dir,
        extensions_dir,
    }
}

fn secret_file() -> (PathBuf, PathBuf) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home"));
    let secret_dir = home.join(format!("cs_landlock_secret_{}", std::process::id()));
    let _ = fs::remove_dir_all(&secret_dir);
    fs::create_dir_all(&secret_dir).expect("secret dir");
    let secret = secret_dir.join("id_rsa");
    fs::write(&secret, b"secret").expect("key");
    (secret_dir, secret)
}

#[test]
fn landlock_denies_outside_allow_list_and_allows_data_dir() {
    if abi_version().is_err() {
        return;
    }
    let root = PathBuf::from(format!("/tmp/cs_landlock_deny_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let paths = make_paths(&root);
    let (secret_dir, secret) = secret_file();
    let allowed = paths.data_dir.join("allowed.txt");
    let paths_for_child = paths.clone();
    let script = format!(
        "if cat '{allowed}' >/dev/null 2>&1; then echo ALLOWED_OK; else echo ALLOWED_FAIL; fi; \
         if cat '{secret}' >/dev/null 2>&1; then echo SECRET_OK; else echo SECRET_DENY; fi",
        allowed = allowed.display(),
        secret = secret.display()
    );
    let output = unsafe {
        Command::new("sh")
            .arg("-c")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .pre_exec(move || restrict_in_child(&paths_for_child))
            .output()
            .expect("run sandboxed sh")
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&secret_dir);
    assert!(
        text.contains("ALLOWED_OK"),
        "data dir must stay readable: {text}"
    );
    assert!(
        text.contains("SECRET_DENY"),
        "paths outside allow-list must be denied: {text}"
    );
}
