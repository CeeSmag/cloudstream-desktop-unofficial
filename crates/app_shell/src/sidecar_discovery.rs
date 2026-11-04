use std::path::{Path, PathBuf};

use extension_api::GatewayError;
use extension_runtime::SidecarLaunchConfig;

/// Where `scripts/setup_sidecar.ps1` puts the build by default.
const DEFAULT_INSTALL: &str = "C:\\cs3d\\repo\\cs-sidecar\\build\\install\\cs-sidecar";

/// Folders that may hold a JDK 21 (the sidecar's sandbox needs the Security Manager, gone in JDK 24+).
const JDK_PARENT_FOLDERS: &[&str] = &[
    "C:\\Program Files\\Java",
    "C:\\Program Files\\Eclipse Adoptium",
    "C:\\Program Files\\Microsoft",
];

/// Finds the sidecar: environment first, then the default install location.
pub(crate) fn discover() -> Result<SidecarLaunchConfig, GatewayError> {
    if std::env::var_os("CS_SIDECAR_HOME").is_some() {
        return SidecarLaunchConfig::from_environment();
    }
    if let Some(config) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(bundled_install))
    {
        return Ok(config);
    }
    let home = PathBuf::from(DEFAULT_INSTALL);
    if !home.join("bin").is_dir() {
        return Err(GatewayError::Unavailable(format!(
            "the extension runtime is not installed. Run scripts/setup_sidecar.ps1 or set CS_SIDECAR_HOME (looked in {DEFAULT_INSTALL})"
        )));
    }
    let java_home = std::env::var_os("CS_SIDECAR_JAVA_HOME")
        .map(PathBuf::from)
        .or_else(find_jdk21);
    Ok(SidecarLaunchConfig::for_install(
        &home,
        java_home.as_deref(),
    ))
}

/// The sidecar and Java that ship with the app: `sidecar` and `jre` folders next to the program,
/// as the installer and the portable zip lay them out. None when the app has no `sidecar` folder.
fn bundled_install(program_folder: &Path) -> Option<SidecarLaunchConfig> {
    let home = program_folder.join("sidecar");
    if !home.join("bin").is_dir() {
        return None;
    }
    let jre = program_folder.join("jre");
    let java_home = jre.join("bin").is_dir().then_some(jre).or_else(find_jdk21);
    Some(SidecarLaunchConfig::for_install(
        &home,
        java_home.as_deref(),
    ))
}

fn find_jdk21() -> Option<PathBuf> {
    JDK_PARENT_FOLDERS
        .iter()
        .find_map(|parent| jdk21_in(Path::new(parent)))
}

fn jdk21_in(parent: &Path) -> Option<PathBuf> {
    std::fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            is_jdk21_folder_name(
                &path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default(),
            )
        })
}

/// True for folder names like `jdk-21.0.2`, `jbr-21.0.10` or `jdk21`.
fn is_jdk21_folder_name(name: &str) -> bool {
    name == "jdk-21"
        || ["jdk-21.", "jbr-21", "jdk21"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jdk21_folder_names_are_recognised() {
        for good in ["jdk-21.0.2", "jbr-21.0.10", "jdk21", "jdk-21"] {
            assert!(is_jdk21_folder_name(good), "{good}");
        }
    }

    #[test]
    fn the_sidecar_and_java_next_to_the_program_are_used() {
        let folder = std::env::temp_dir().join(format!("cs_bundled_{}", std::process::id()));
        std::fs::create_dir_all(folder.join("sidecar").join("bin")).expect("sidecar");
        std::fs::create_dir_all(folder.join("jre").join("bin")).expect("jre");
        let config = bundled_install(&folder).expect("found");
        assert!(config
            .program
            .starts_with(folder.join("sidecar").join("bin")));
        let java_home = config
            .environment
            .iter()
            .find(|(name, _)| name == "JAVA_HOME");
        assert_eq!(
            java_home.map(|(_, value)| value.as_str()),
            folder.join("jre").to_str()
        );
        std::fs::remove_dir_all(folder.join("sidecar")).expect("remove");
        assert!(
            bundled_install(&folder).is_none(),
            "no sidecar folder, nothing bundled"
        );
        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn other_java_versions_are_rejected() {
        for bad in ["jdk-25", "jdk-11", "jre-1.8", "jdk-210"] {
            assert!(!is_jdk21_folder_name(bad), "{bad}");
        }
    }
}
