//! Builds the `java.exe` command line used inside the AppContainer.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use extension_api::GatewayError;

use crate::SidecarLaunchConfig;

/// Resolves `java.exe` from the launch config or process `JAVA_HOME`.
pub(crate) fn java_exe(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let java_home = launch_env(config, "JAVA_HOME")
        .map(PathBuf::from)
        .ok_or_else(|| {
            GatewayError::Unavailable(
                "JAVA_HOME is required to start the sidecar inside an AppContainer".into(),
            )
        })?;
    let exe = java_home.join("bin").join("java.exe");
    if !exe.is_file() {
        return Err(GatewayError::Unavailable(format!(
            "java.exe not found at {}",
            exe.display()
        )));
    }
    Ok(exe)
}

/// The child environment takes precedence over the inherited process environment.
fn launch_env(config: &SidecarLaunchConfig, key: &str) -> Option<std::ffi::OsString> {
    config
        .environment
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| std::ffi::OsString::from(value))
        .or_else(|| std::env::var_os(key))
}

/// `JAVA_HOME` folder that contains `bin/java.exe`.
pub(crate) fn java_home_dir(java: &Path) -> Result<PathBuf, GatewayError> {
    let bin = java.parent().ok_or_else(|| {
        GatewayError::Unavailable(format!("java.exe has no parent folder: {}", java.display()))
    })?;
    bin.parent().map(Path::to_path_buf).ok_or_else(|| {
        GatewayError::Unavailable(format!("java bin has no JAVA_HOME: {}", bin.display()))
    })
}

/// Install home (`.../cs-sidecar`) from `bin/cs-sidecar.bat`.
pub(crate) fn sidecar_home(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let bin = config
        .program
        .parent()
        .ok_or_else(|| GatewayError::Unavailable("sidecar program has no parent folder".into()))?;
    Ok(bin
        .parent()
        .ok_or_else(|| GatewayError::Unavailable("sidecar bin has no install home".into()))?
        .to_path_buf())
}

/// Reads `-Dcloudstream.data.dir="..."` from `JAVA_OPTS` and ensures the folder exists.
pub(crate) fn data_dir_from_config(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let opts = java_opts(config);
    let marker = "-Dcloudstream.data.dir=\"";
    let start = opts.find(marker).ok_or_else(|| {
        GatewayError::Unavailable("sidecar data dir is missing from JAVA_OPTS".into())
    })? + marker.len();
    let rest = opts.get(start..).unwrap_or("");
    let end = rest.find('"').ok_or_else(|| {
        GatewayError::Unavailable("sidecar data dir in JAVA_OPTS is not quoted".into())
    })?;
    let path = PathBuf::from(rest.get(..end).unwrap_or(""));
    if path.as_os_str().is_empty() {
        return Err(GatewayError::Unavailable(
            "sidecar data dir is empty".into(),
        ));
    }
    std::fs::create_dir_all(&path).map_err(|source| {
        GatewayError::Unavailable(format!("cannot create sidecar data dir: {source}"))
    })?;
    Ok(path)
}

/// `java.exe [JAVA_OPTS] -classpath "jars" sidecar.SidecarMainKt`
pub(crate) fn java_command_line(
    java: &Path,
    app_home: &Path,
    config: &SidecarLaunchConfig,
) -> Result<String, GatewayError> {
    Ok(format!(
        "\"{}\" {} -classpath \"{}\" {MAIN_CLASS}",
        java.display(),
        java_opts(config),
        classpath_for(app_home, app_home)?
    ))
}

const MAIN_CLASS: &str = "sidecar.SidecarMainKt";

/// Builds the direct process launch for an installed worker or a custom test executable.
pub(crate) fn plain_command(
    config: &SidecarLaunchConfig,
) -> Result<std::process::Command, GatewayError> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let mut command = if config.program.file_name() == Some(OsStr::new("cs-sidecar.bat")) {
        let executable = launch_env(config, "JAVA_HOME").map_or_else(
            || PathBuf::from("java.exe"),
            |home| PathBuf::from(home).join("bin").join("java.exe"),
        );
        let mut java = Command::new(executable);
        for key in ["JAVA_OPTS", "CS_SIDECAR_OPTS"] {
            if let Some(options) = launch_env(config, key) {
                java.raw_arg(options);
            }
        }
        let home = sidecar_home(config)?;
        java.current_dir(&home);
        java.args([
            "-classpath",
            &classpath_for(&home, Path::new(""))?,
            MAIN_CLASS,
        ]);
        java
    } else {
        Command::new(&config.program)
    };
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

fn java_opts(config: &SidecarLaunchConfig) -> &str {
    config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_OPTS")
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn classpath_for(app_home: &Path, prefix: &Path) -> Result<String, GatewayError> {
    let lib = app_home.join("lib");
    let mut jars = std::fs::read_dir(&lib)
        .map_err(|source| {
            GatewayError::Unavailable(format!(
                "cannot read sidecar lib {}: {source}",
                lib.display()
            ))
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| prefix.join("lib").join(entry.file_name()))
        .filter(|path| {
            path.extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        })
        .collect::<Vec<_>>();
    jars.sort();
    if jars.is_empty() {
        return Err(GatewayError::Unavailable(format!(
            "no jars under {}",
            lib.display()
        )));
    }
    Ok(jars
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn java_home_dir_walks_up_from_java_exe() {
        let java = PathBuf::from(r"C:\jre\bin\java.exe");
        assert_eq!(
            java_home_dir(&java).expect("home"),
            PathBuf::from(r"C:\jre")
        );
    }

    #[test]
    fn data_dir_from_config_reads_quoted_path() {
        let dir = std::env::temp_dir().join(format!("cs_ac_data_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut config = SidecarLaunchConfig::for_install(Path::new("h"), None);
        config.environment.push((
            "JAVA_OPTS".into(),
            format!("-Dcloudstream.data.dir=\"{}\"", dir.display()),
        ));
        let parsed = data_dir_from_config(&config).expect("data dir");
        assert_eq!(parsed, dir);
        assert!(dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }
}
