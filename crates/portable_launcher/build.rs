//! Build script for portable_launcher: gives the exe the CloudStream icon and passes on the payload.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_payload)");
    println!("cargo:rerun-if-env-changed=CS_PAYLOAD_ARCHIVE");
    if let Ok(archive) = std::env::var("CS_PAYLOAD_ARCHIVE") {
        if std::path::Path::new(&archive).exists() {
            println!("cargo:rustc-cfg=has_payload");
            println!("cargo:rerun-if-changed={archive}");
        }
    }
    embed_icon();
}

/// Embeds the app icon into the Windows exe (file explorer and taskbar show it).
#[cfg(windows)]
fn embed_icon() {
    let icon = "../desktop_tauri/icons/icon.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon);
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the CloudStream icon: {error}");
    }
}

#[cfg(not(windows))]
fn embed_icon() {}
