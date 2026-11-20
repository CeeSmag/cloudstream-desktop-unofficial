//! Build script for `app_shell`: embeds the CloudStream icon into `cloudstream.exe` on Windows.

fn main() {
    embed_icon();
}

/// Embeds the multi-resolution `.ico` resource into the Windows executable so Explorer,
/// shortcuts, Task Manager, and the Windows taskbar always display the CloudStream icon.
#[cfg(windows)]
fn embed_icon() {
    let icon = "icons/icon.ico";
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
