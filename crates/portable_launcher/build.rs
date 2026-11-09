//! Build script for portable_launcher: gives the exe the CloudStream icon and passes on the payload.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_payload)");
    println!("cargo:rerun-if-env-changed=CS_PAYLOAD_ARCHIVE");
    let mut payload_id = "none".to_owned();
    if let Ok(archive) = std::env::var("CS_PAYLOAD_ARCHIVE") {
        if std::path::Path::new(&archive).exists() {
            println!("cargo:rustc-cfg=has_payload");
            println!("cargo:rerun-if-changed={archive}");
            payload_id = fingerprint(&archive);
        }
    }
    // The unpacked program folder is named after this, so a new build never reuses an old one.
    println!("cargo:rustc-env=CS_PAYLOAD_ID={payload_id}");
    embed_icon();
}

/// A short fingerprint of the packed program (FNV-1a over its bytes).
fn fingerprint(archive: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let Ok(bytes) = std::fs::read(archive) else {
        return "unreadable".to_owned();
    };
    let hash = bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    });
    format!("{hash:016x}").chars().take(10).collect()
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
