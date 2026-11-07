//! Build script for portable_launcher.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_payload)");
    println!("cargo:rerun-if-env-changed=CS_PAYLOAD_ARCHIVE");
    if let Ok(archive) = std::env::var("CS_PAYLOAD_ARCHIVE") {
        if std::path::Path::new(&archive).exists() {
            println!("cargo:rustc-cfg=has_payload");
            println!("cargo:rerun-if-changed={archive}");
        }
    }
}
