//! Live CreateProcess smoke for the AppContainer launcher.
//!
//! Ignored in normal gates: needs a built sidecar install and JDK 21.
//! Run: `CS_SIDECAR_HOME=... CS_SIDECAR_JAVA_HOME=... cargo test -p extension_runtime --lib -- --ignored`

use std::path::PathBuf;

use crate::sidecar_app_container_launch::start_in_app_container;
use crate::SidecarLaunchConfig;

#[test]
#[ignore = "needs CS_SIDECAR_HOME and a JDK 21 home"]
fn app_container_createprocess_starts_sidecar_jvm() {
    let java = std::env::var_os("CS_SIDECAR_JAVA_HOME")
        .or_else(|| std::env::var_os("JAVA_HOME"))
        .map(PathBuf::from)
        .filter(|path| path.join("bin").join("java.exe").is_file())
        .expect("JAVA_HOME or CS_SIDECAR_JAVA_HOME with java.exe");
    let home = std::env::var_os("CS_SIDECAR_HOME")
        .map(PathBuf::from)
        .filter(|path| path.join("bin").join("cs-sidecar.bat").is_file())
        .unwrap_or_else(|| PathBuf::from(r"C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar"));
    assert!(
        home.join("lib").is_dir(),
        "sidecar install missing at {}",
        home.display()
    );
    let data = std::env::temp_dir().join(format!("cs_ac_launch_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let config = SidecarLaunchConfig::for_install(&home, Some(&java)).with_data_dir(&data);
    let started = start_in_app_container(&config);
    let _ = std::fs::remove_dir_all(&data);
    started.expect("CreateProcess inside AppContainer");
}
