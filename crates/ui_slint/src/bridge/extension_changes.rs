//! Plugin and repository changes, run off the UI thread with a busy line.

use app_services::UiServices;
use core_model::InstalledExtension;

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::extension_catalog::{
    load_extensions_catalog, plugin_key, refresh_plugins, refresh_providers,
};
use super::home_loading::load_home;

type ExtensionChange = Box<dyn FnOnce(&UiServices) -> Result<String, String> + Send>;

/// Installs the catalog plugin named `key`.
pub(crate) fn install_plugin(ctx: &BridgeContext, key: &str) {
    let found = ctx
        .session
        .borrow()
        .available
        .iter()
        .find(|hit| plugin_key(hit) == key)
        .cloned();
    let Some(hit) = found else {
        return;
    };
    let busy = format!("Installing {}… (this can take a minute)", hit.plugin.name);
    run_change(
        ctx,
        &busy,
        Box::new(move |services| {
            let providers = services
                .extensions
                .install(&hit.plugin, &hit.repository_url)
                .map_err(|error| format!("{} could not be installed: {error}", hit.plugin.name))?;
            Ok(format!(
                "Installed {} ({} providers)",
                hit.plugin.name,
                providers.len()
            ))
        }),
    );
}

pub(crate) fn add_repository(ctx: &BridgeContext, url: String) {
    if url.is_empty() {
        return;
    }
    run_change(
        ctx,
        "Adding the repository…",
        Box::new(move |services| {
            let entry = services
                .repositories
                .add(&url)
                .map_err(|error| format!("Repository failed: {error}"))?;
            Ok(format!("Added repository {}", entry.name))
        }),
    );
}

pub(crate) fn remove_repository(ctx: &BridgeContext, url: String) {
    run_change(
        ctx,
        "Removing the repository…",
        Box::new(move |services| {
            services
                .repositories
                .remove(&url)
                .map_err(|error| error.to_string())?;
            Ok("Repository removed".to_owned())
        }),
    );
}

pub(crate) fn set_trusted(ctx: &BridgeContext, file: String, trusted: bool) {
    let busy = if trusted {
        "Trusting the plugin (reloading it)…"
    } else {
        "Sandboxing the plugin (reloading it)…"
    };
    run_change(
        ctx,
        busy,
        Box::new(move |services| {
            let extension = installed_by_file(services, &file)?;
            services
                .extensions
                .set_trusted(&extension, trusted)
                .map_err(|error| error.to_string())?;
            let mode = if trusted { "trusted" } else { "sandboxed" };
            Ok(format!("{} is {mode} now", extension.internal_name))
        }),
    );
}

pub(crate) fn uninstall(ctx: &BridgeContext, file: String) {
    run_change(
        ctx,
        "Removing the plugin…",
        Box::new(move |services| {
            let extension = installed_by_file(services, &file)?;
            services
                .extensions
                .uninstall(&extension)
                .map_err(|error| error.to_string())?;
            Ok(format!("Removed {}", extension.internal_name))
        }),
    );
}

/// Runs `change` in the background with `busy` on screen; [`show_change_done`] follows.
fn run_change(ctx: &BridgeContext, busy: &str, change: ExtensionChange) {
    if let Some(ui) = ctx.window() {
        ui.set_extensions_busy(busy.into());
    }
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("extension-change", move || {
        let message = change(&services).unwrap_or_else(|error| {
            tracing::warn!(%error, "an extension change failed");
            error
        });
        sender.send(UiEvent::ExtensionChanged(message));
    });
}

/// After a change: its message, then every list that depends on the plugins.
pub(crate) fn show_change_done(ctx: &BridgeContext, ui: &CloudStream, message: String) {
    ui.set_extensions_busy("".into());
    ui.set_repo_url("".into());
    ui.set_notice(message.into());
    refresh_plugins(ctx, ui);
    refresh_providers(ctx, ui);
    load_extensions_catalog(ctx);
    load_home(ctx);
}

fn installed_by_file(services: &UiServices, file: &str) -> Result<InstalledExtension, String> {
    services
        .extensions
        .installed()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|known| known.file_name == file)
        .ok_or_else(|| "that plugin is no longer installed".to_owned())
}
