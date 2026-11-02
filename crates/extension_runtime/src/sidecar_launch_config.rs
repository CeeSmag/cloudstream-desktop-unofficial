use std::path::{Path, PathBuf};

use extension_api::GatewayError;

/// Environment variable holding the sidecar install directory.
const SIDECAR_HOME_VARIABLE: &str = "CS_SIDECAR_HOME";
/// Environment variable holding the JDK 21 home the sidecar must run on.
const SIDECAR_JAVA_HOME_VARIABLE: &str = "CS_SIDECAR_JAVA_HOME";

/// How to start the sidecar process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarLaunchConfig {
    /// Executable to start.
    pub program: PathBuf,
    /// Arguments passed to it.
    pub args: Vec<String>,
    /// Extra environment variables for the child process.
    pub environment: Vec<(String, String)>,
}

impl SidecarLaunchConfig {
    /// Builds the config for an installed sidecar (`<home>/bin/cs-sidecar[.bat]`).
    pub fn for_install(home: &Path, java_home: Option<&Path>) -> Self {
        let script = if cfg!(windows) {
            "cs-sidecar.bat"
        } else {
            "cs-sidecar"
        };
        let environment = java_home
            .map(|path| vec![("JAVA_HOME".to_owned(), path.to_string_lossy().into_owned())])
            .unwrap_or_default();
        Self {
            program: home.join("bin").join(script),
            args: Vec::new(),
            environment,
        }
    }

    /// Makes the sidecar keep the files extensions write (their settings and key/value data)
    /// inside `data_dir` instead of in the user's profile.
    pub fn with_data_dir(mut self, data_dir: &Path) -> Self {
        let option = format!("-Dcloudstream.data.dir=\"{}\"", data_dir.display());
        self.environment.push(("JAVA_OPTS".to_owned(), option));
        self
    }

    /// Reads `CS_SIDECAR_HOME` and optionally `CS_SIDECAR_JAVA_HOME`.
    pub fn from_environment() -> Result<Self, GatewayError> {
        let home = std::env::var_os(SIDECAR_HOME_VARIABLE).ok_or_else(|| {
            GatewayError::Unavailable(format!(
                "{SIDECAR_HOME_VARIABLE} is not set; run scripts/setup_sidecar.ps1 first"
            ))
        })?;
        let java_home = std::env::var_os(SIDECAR_JAVA_HOME_VARIABLE).map(PathBuf::from);
        Ok(Self::for_install(Path::new(&home), java_home.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_install_points_at_script_inside_bin() {
        let config = SidecarLaunchConfig::for_install(Path::new("/opt/sidecar"), None);
        assert!(
            config.program.starts_with("/opt/sidecar/bin")
                || config.program.starts_with("/opt/sidecar\\bin")
        );
        assert!(config.environment.is_empty());
    }

    #[test]
    fn with_data_dir_passes_the_folder_as_a_quoted_jvm_property() {
        let config = SidecarLaunchConfig::for_install(Path::new("h"), None)
            .with_data_dir(Path::new("/my data/sidecar"));
        let (name, value) = &config.environment[0];
        assert_eq!(name, "JAVA_OPTS");
        assert!(value.starts_with("-Dcloudstream.data.dir=\"") && value.contains("my data"));
    }

    #[test]
    fn for_install_sets_java_home_when_given() {
        let config = SidecarLaunchConfig::for_install(Path::new("h"), Some(Path::new("/jdk21")));
        assert_eq!(
            config.environment,
            vec![("JAVA_HOME".to_owned(), "/jdk21".to_owned())]
        );
    }
}
