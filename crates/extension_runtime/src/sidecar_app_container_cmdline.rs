//! Builds the `java.exe` command line used inside the AppContainer.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use extension_api::GatewayError;

use crate::SidecarLaunchConfig;

/// Resolves `java.exe` from the launch config or process `JAVA_HOME`.
pub(crate) fn java_exe(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let java_home = config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_HOME")
        .map(|(_, value)| PathBuf::from(value))
        .or_else(|| std::env::var_os("JAVA_HOME").map(PathBuf::from))
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
    let classpath = classpath_for(app_home)?;
    Ok(format!(
        "\"{}\" {} -classpath \"{}\" sidecar.SidecarMainKt",
        java.display(),
        java_opts(config),
        classpath
    ))
}

fn java_opts(config: &SidecarLaunchConfig) -> &str {
    config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_OPTS")
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn classpath_for(app_home: &Path) -> Result<String, GatewayError> {
    let lib = app_home.join("lib");
    let mut jars = std::fs::read_dir(&lib)
        .map_err(|source| {
            GatewayError::Unavailable(format!(
                "cannot read sidecar lib {}: {source}",
                lib.display()
            ))
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
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
