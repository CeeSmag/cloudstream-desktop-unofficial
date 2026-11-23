//! Plugin and repository changes, run off the UI thread with a busy line.

use app_services::{PluginHit, SecurityBlock, ServiceError, UiServices};
use core_model::InstalledExtension;

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::extension_catalog::{
    load_extensions_catalog, plugin_key, refresh_plugins, refresh_providers,
};
use super::home_loading::load_home;
use super::repository_discovery::watch_discovered_repositories;
use super::security_dialog::open_security_blocked;

enum ChangeOutcome {
    Done(String),
    Failed(String),
    SecurityBlocked {
        hit: Option<Box<PluginHit>>,
        block: Box<SecurityBlock>,
    },
}

type ExtensionChange = Box<dyn FnOnce(&UiServices) -> ChangeOutcome + Send>;

/// Installs the catalog plugin named `key` sandboxed (`trusted = false`).
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
        true,
        Box::new(move |services| {
            match services
                .extensions
                .install(&hit.plugin, &hit.repository_url)
            {
                Ok(providers) => ChangeOutcome::Done(format!(
                    "Installed {} ({} providers)",
                    hit.plugin.name,
                    providers.len()
                )),
                Err(ServiceError::SecurityBlocked(block)) => ChangeOutcome::SecurityBlocked {
                    hit: Some(Box::new(hit)),
                    block,
                },
                Err(error) => ChangeOutcome::Failed(format!(
                    "{} could not be installed: {error}",
                    hit.plugin.name
                )),
            }
        }),
    );
}

/// Installs `hit` explicitly trusted after the user confirms the security warning modal.
pub(crate) fn install_plugin_trusted(ctx: &BridgeContext, hit: PluginHit) {
    let busy = format!(
        "Installing {} as trusted… (this can take a minute)",
        hit.plugin.name
    );
    run_change(
        ctx,
        &busy,
        true,
        Box::new(move |services| {
            match services
                .extensions
                .install_trusted(&hit.plugin, &hit.repository_url)
            {
                Ok(providers) => ChangeOutcome::Done(format!(
                    "Installed {} as trusted ({} providers)",
                    hit.plugin.name,
                    providers.len()
                )),
                Err(ServiceError::SecurityBlocked(block)) => {
                    ChangeOutcome::SecurityBlocked { hit: None, block }
                }
                Err(error) => ChangeOutcome::Failed(format!(
                    "{} could not be installed: {error}",
                    hit.plugin.name
                )),
            }
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
        false,
        Box::new(move |services| match services.repositories.add(&url) {
            Ok(entry) => ChangeOutcome::Done(format!("Added repository {}", entry.name)),
            Err(error) => ChangeOutcome::Failed(format!("Repository failed: {error}")),
        }),
    );
}

pub(crate) fn remove_repository(ctx: &BridgeContext, url: String) {
    run_change(
        ctx,
        "Removing the repository…",
        false,
        Box::new(move |services| match services.repositories.remove(&url) {
            Ok(()) => ChangeOutcome::Done("Repository removed".to_owned()),
            Err(error) => ChangeOutcome::Failed(error.to_string()),
        }),
    );
}

pub(crate) fn apply_set_trusted(ctx: &BridgeContext, file: String, trusted: bool) {
    let busy = if trusted {
        "Trusting the plugin (reloading it)…"
    } else {
        "Sandboxing the plugin (reloading it)…"
    };
    run_change(
        ctx,
        busy,
        true,
        Box::new(move |services| {
            let extension = match installed_by_file(services, &file) {
                Ok(ext) => ext,
                Err(error) => return ChangeOutcome::Failed(error),
            };
            match services.extensions.set_trusted(&extension, trusted) {
                Ok(()) => {
                    let mode = if trusted { "trusted" } else { "sandboxed" };
                    ChangeOutcome::Done(format!("{} is {mode} now", extension.internal_name))
                }
                Err(ServiceError::SecurityBlocked(block)) => {
                    ChangeOutcome::SecurityBlocked { hit: None, block }
                }
                Err(error) => ChangeOutcome::Failed(error.to_string()),
            }
        }),
    );
}

pub(crate) fn uninstall(ctx: &BridgeContext, file: String) {
    run_change(
        ctx,
        "Removing the plugin…",
        false,
        Box::new(move |services| {
            let extension = match installed_by_file(services, &file) {
                Ok(ext) => ext,
                Err(error) => return ChangeOutcome::Failed(error),
            };
            match services.extensions.uninstall(&extension) {
                Ok(()) => ChangeOutcome::Done(format!("Removed {}", extension.internal_name)),
                Err(error) => ChangeOutcome::Failed(error.to_string()),
            }
        }),
    );
}

/// Runs `change` in the background with `busy` on screen; [`show_change_done`] or
/// [`show_security_blocked`] follows.
fn run_change(ctx: &BridgeContext, busy: &str, watch_on_success: bool, change: ExtensionChange) {
    if let Some(ui) = ctx.window() {
        ui.set_extensions_busy(busy.into());
    }
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("extension-change", move || match change(&services) {
        ChangeOutcome::Done(message) => sender.send(UiEvent::ExtensionChanged {
            message,
            watch_repositories: watch_on_success,
        }),
        ChangeOutcome::Failed(error) => {
            tracing::warn!(%error, "an extension change failed");
            sender.send(UiEvent::ExtensionChanged {
                message: error,
                watch_repositories: false,
            });
        }
        ChangeOutcome::SecurityBlocked { hit, block } => {
            tracing::warn!(
                plugin = %block.plugin,
                can_trust = block.can_trust,
                summary = %block.short_summary(),
                "plugin blocked by security policy"
            );
            sender.send(UiEvent::ExtensionSecurityBlocked { hit, block });
        }
    });
}

/// Shows the structured security warning modal after a security refusal.
pub(crate) fn show_security_blocked(
    ctx: &BridgeContext,
    ui: &CloudStream,
    hit: Option<PluginHit>,
    block: SecurityBlock,
) {
    ui.set_extensions_busy("".into());
    refresh_plugins(ctx, ui);
    refresh_providers(ctx, ui);
    open_security_blocked(ctx, ui, hit, block);
}

/// After a change: its message, then every list that depends on the plugins.
pub(crate) fn show_change_done(
    ctx: &BridgeContext,
    ui: &CloudStream,
    message: String,
    watch_repositories: bool,
) {
    ui.set_extensions_busy("".into());
    ui.set_repo_url("".into());
    ui.set_notice(message.into());
    refresh_plugins(ctx, ui);
    refresh_providers(ctx, ui);
    load_extensions_catalog(ctx);
    load_home(ctx);
    if watch_repositories {
        watch_discovered_repositories(ctx);
    }
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
