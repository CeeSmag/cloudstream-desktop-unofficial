//! Settings > Extensions and the Home provider.

use std::time::{SystemTime, UNIX_EPOCH};

use slint::SharedString;

use crate::posters::plugin_rows;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::home_loading::load_home;
use super::preference_callbacks::saved;

pub(crate) fn wire_extension_settings(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_provider(move |name| set_provider(&c, name));
    let c = ctx.clone();
    ui.on_set_trusted(move |file, trusted| {
        change_extension(&c, &file, |services, extension| {
            services
                .extensions
                .set_trusted(extension, trusted)
                .map(|_| ())
        });
    });
    let c = ctx.clone();
    ui.on_uninstall_plugin(move |file| {
        change_extension(&c, &file, |services, extension| {
            services.extensions.uninstall(extension)
        });
    });
    let c = ctx.clone();
    ui.on_install_repo(move |url| add_repository(&c, url.to_string()));
}

fn set_provider(ctx: &BridgeContext, name: SharedString) {
    if let Some(ui) = ctx.window() {
        ui.set_selected_provider(name);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    saved(
        "provider switch time",
        ctx.services.settings.set_sources_valid_from(now),
    );
    let valid_from = ctx.services.title_cache.invalidate_now();
    tracing::info!(
        valid_from,
        "saved sources of the previous provider are no longer used"
    );
    load_home(ctx);
}

fn change_extension(
    ctx: &BridgeContext,
    file: &str,
    change: impl Fn(
        &app_services::UiServices,
        &core_model::InstalledExtension,
    ) -> Result<(), app_services::ServiceError>,
) {
    match ctx.services.extensions.installed() {
        Ok(list) => {
            if let Some(extension) = list.iter().find(|known| known.file_name == file) {
                if let Err(error) = change(&ctx.services, extension) {
                    tracing::warn!(%file, %error, "the plugin change failed");
                    ctx.sender()
                        .notice(format!("Plugin change failed: {error}"));
                }
            }
        }
        Err(error) => tracing::warn!(%error, "could not list the installed plugins"),
    }
    if let Some(ui) = ctx.window() {
        refresh_plugins(ctx, &ui);
    }
}

fn add_repository(ctx: &BridgeContext, url: String) {
    if url.is_empty() {
        ctx.sender().notice("Enter a repository URL");
        return;
    }
    match ctx.services.repositories.add(&url) {
        Ok(entry) => ctx
            .sender()
            .notice(format!("Added repository {}", entry.name)),
        Err(error) => ctx.sender().notice(format!("Repository failed: {error}")),
    }
}

/// Shows the installed plugins in Settings.
pub(crate) fn refresh_plugins(ctx: &BridgeContext, ui: &CloudStream) {
    match ctx.services.extensions.installed() {
        Ok(list) => ui.set_plugins(plugin_rows(&list)),
        Err(error) => tracing::warn!(%error, "could not list the installed plugins"),
    }
}
