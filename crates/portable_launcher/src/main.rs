//! Portable single-executable launcher for CloudStream.
//! Unpacks runtime binaries (app, sidecar, JRE, libmpv) to AppData on first launch
//! and transparently manages execution while preserving all user data.
#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::Command;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(has_payload)]
const PAYLOAD: &[u8] = include_bytes!(env!("CS_PAYLOAD_ARCHIVE"));
#[cfg(not(has_payload))]
const PAYLOAD: &[u8] = &[];

fn get_runtime_dir() -> anyhow::Result<PathBuf> {
    let local_data = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."))
    };

    Ok(local_data
        .join("CloudStreamRs")
        .join(format!("runtime-{APP_VERSION}")))
}

fn extract_payload_if_needed(runtime_dir: &Path) -> anyhow::Result<()> {
    let main_binary_name = if cfg!(windows) {
        "desktop_tauri.exe"
    } else {
        "desktop_tauri"
    };

    if runtime_dir.join(main_binary_name).exists() {
        return Ok(());
    }

    if PAYLOAD.is_empty() {
        anyhow::bail!("This launcher was built without an embedded payload archive.");
    }

    std::fs::create_dir_all(runtime_dir)?;

    let decoder = flate2::read::GzDecoder::new(PAYLOAD);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(runtime_dir)?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let runtime_dir = get_runtime_dir()?;
    extract_payload_if_needed(&runtime_dir)?;

    let main_binary_name = if cfg!(windows) {
        "desktop_tauri.exe"
    } else {
        "desktop_tauri"
    };
    let app_exe = runtime_dir.join(main_binary_name);

    if !app_exe.exists() {
        anyhow::bail!("Application executable not found at {}", app_exe.display());
    }

    let current_exe = std::env::current_exe()?;

    let mut cmd = Command::new(&app_exe);
    cmd.args(std::env::args().skip(1));
    cmd.env("CS_PORTABLE_EXE", &current_exe);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn()?;
    let status = child.wait()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
