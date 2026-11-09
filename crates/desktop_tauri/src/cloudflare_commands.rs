//! The Home page's "Verify" button: the viewer allows a site to open its Cloudflare verification window.

use std::sync::Arc;

use tauri::State;

use crate::cloudflare_gate::Gate;

/// Allows the verification window for `host` (and its subdomains) from now on.
#[tauri::command]
#[tracing::instrument(level = "info", skip_all, fields(host = %host))]
pub fn allow_cloudflare_host(host: String, gate: State<'_, Arc<Gate>>) {
    tracing::info!(%host, "the viewer allowed Cloudflare verification for this site");
    gate.allow(&host);
}

/// The page tells how an image from the web fared: the first few per site are logged with what the app's
/// own browser holds for that address, which is what tells a blocked image from a broken one.
#[tauri::command]
pub fn report_image(url: String, loaded: bool, app: tauri::AppHandle, gate: State<'_, Arc<Gate>>) {
    use tauri::Manager;
    let Ok(address) = url.parse::<tauri::Url>() else {
        tracing::warn!(
            loaded,
            "an image address that is not a web address was reported"
        );
        return;
    };
    let host = address.host_str().unwrap_or("?").to_owned();
    let cookies: Vec<String> = app
        .get_webview_window("main")
        .and_then(|view| view.cookies_for_url(address.clone()).ok())
        .unwrap_or_default()
        .iter()
        .map(|cookie| format!("{}@{}", cookie.name(), cookie.domain().unwrap_or("-")))
        .collect();
    let image = format!("{host}{}", address.path());
    if loaded {
        tracing::info!(%host, %image, ?cookies, "an image loaded");
    } else {
        tracing::warn!(%host, %image, ?cookies, check_allowed = gate.is_allowed(&host), "an image did NOT load; the cookies are what the app's browser sends for it");
    }
}
