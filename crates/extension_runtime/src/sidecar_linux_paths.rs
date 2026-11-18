//! Resolves the filesystem allow-list roots for a Linux-sandboxed sidecar.

use std::path::{Path, PathBuf};

use extension_api::GatewayError;

use crate::SidecarLaunchConfig;

/// Paths the worker may touch under Landlock (plus fixed `/tmp`, `/proc`, `/dev`).
#[derive(Debug, Clone)]
pub(crate) struct SandboxPaths {
    /// JDK / JRE home (`JAVA_HOME`).
    pub(crate) java_home: PathBuf,
    /// Sidecar install home (`…/cs-sidecar`).
    pub(crate) sidecar_home: PathBuf,
    /// `<data>/sidecar` (`cloudstream.data.dir`) — read/write.
    pub(crate) data_dir: PathBuf,
    /// Sibling `<data>/extensions` — read-only `.cs3` load path.
    pub(crate) extensions_dir: PathBuf,
}

impl SandboxPaths {
    /// Builds the allow-list from a production launch config; creates data/extensions dirs.
    pub(crate) fn from_config(config: &SidecarLaunchConfig) -> Result<Self, GatewayError> {
        let java_home = java_home_from_config(config)?;
        let sidecar_home = sidecar_home_from_config(config)?;
        let data_dir = data_dir_from_config(config)?;
        let extensions_dir = extensions_dir_beside(&data_dir)?;
        Ok(Self {
            java_home,
            sidecar_home,
            data_dir,
            extensions_dir,
        })
    }
}

fn java_home_from_config(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let home = config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_HOME")
        .map(|(_, value)| PathBuf::from(value))
        .or_else(|| std::env::var_os("JAVA_HOME").map(PathBuf::from))
        .ok_or_else(|| {
            GatewayError::Unavailable(
                "JAVA_HOME is required to start a sandboxed Linux sidecar".into(),
            )
        })?;
    let java = home.join("bin").join("java");
    if !java.is_file() {
        return Err(GatewayError::Unavailable(format!(
            "java not found at {}",
            java.display()
        )));
    }
    Ok(home)
}

fn sidecar_home_from_config(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let bin = config
        .program
        .parent()
        .ok_or_else(|| GatewayError::Unavailable("sidecar program has no parent folder".into()))?;
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| GatewayError::Unavailable("sidecar bin has no install home".into()))
}

fn data_dir_from_config(config: &SidecarLaunchConfig) -> Result<PathBuf, GatewayError> {
    let opts = java_opts(config);
    let marker = "-Dcloudstream.data.dir=\"";
    let start = opts.find(marker).ok_or_else(|| {
        GatewayError::Unavailable("sidecar data dir is missing from JAVA_OPTS".into())
    })?;
    let after = start + marker.len();
    let rest = opts.get(after..).unwrap_or("");
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

fn extensions_dir_beside(data_dir: &Path) -> Result<PathBuf, GatewayError> {
    let parent = data_dir.parent().ok_or_else(|| {
        GatewayError::Unavailable("sidecar data dir has no parent for extensions".into())
    })?;
    let extensions = parent.join("extensions");
    std::fs::create_dir_all(&extensions).map_err(|source| {
        GatewayError::Unavailable(format!("cannot create extensions dir: {source}"))
    })?;
    Ok(extensions)
}

fn java_opts(config: &SidecarLaunchConfig) -> &str {
    config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_OPTS")
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn from_config_reads_data_dir_and_extensions_sibling() {
        let root = std::env::temp_dir().join(format!("cs_linux_paths_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("sidecar");
        let home = root.join("cs-sidecar");
        let jre = root.join("jre");
        fs::create_dir_all(home.join("bin")).expect("bin");
        fs::create_dir_all(jre.join("bin")).expect("jre bin");
        fs::write(jre.join("bin").join("java"), b"#!/bin/sh\n").expect("java");
        let mut config = SidecarLaunchConfig::for_install(&home, Some(&jre));
        config = config.with_data_dir(&data);
        let paths = SandboxPaths::from_config(&config).expect("paths");
        assert_eq!(paths.data_dir, data);
        assert_eq!(paths.extensions_dir, root.join("extensions"));
        assert_eq!(paths.java_home, jre);
        assert_eq!(paths.sidecar_home, home);
        let _ = fs::remove_dir_all(&root);
    }
}
