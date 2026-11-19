//! The Extensions page's lists: repositories, the plugins they offer, the installed plugins.

use std::collections::HashMap;

use app_services::PluginHit;

use crate::background_work::spawn_work;
use crate::plugin_filter::format_file_size;
use crate::posters::{plugin_rows, string_model};
use crate::ui_events::UiEvent;
use crate::{AvailablePlugin, CloudStream};

use super::bridge_context::BridgeContext;
use super::plugin_list::show_new_catalog;
use super::repo_list::{show_repositories, sync_open_repo_detail};

/// What the Extensions page lists.
#[derive(Debug, Default)]
pub(crate) struct ExtensionsCatalog {
    pub(crate) repositories: Vec<core_model::RepositoryEntry>,
    pub(crate) plugins: Vec<PluginHit>,
    pub(crate) installed_times: HashMap<String, i64>,
    pub(crate) error: Option<String>,
}

/// Loads the repositories and the plugins they offer (downloads every repository once).
pub(crate) fn load_extensions_catalog(ctx: &BridgeContext) {
    if let Some(ui) = ctx.window() {
        ui.set_plugins_loading(true);
    }
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("extensions-catalog", move || {
        let mut catalog = ExtensionsCatalog::default();
        match services.repositories.list() {
            Ok(list) => catalog.repositories = list,
            Err(error) => catalog.error = Some(error.to_string()),
        }
        match services.repositories.all_plugins() {
            Ok(hits) => catalog.plugins = hits,
            Err(error) => catalog.error = Some(error.to_string()),
        }
        catalog.installed_times = services
            .extensions
            .installed()
            .map(|list| {
                list.into_iter()
                    .map(|ext| (ext.internal_name, ext.installed_at_ms))
                    .collect()
            })
            .unwrap_or_default();
        sender.send(UiEvent::ExtensionsLoaded(Box::new(catalog)));
    });
}

/// Shows the catalog on the Extensions page (Repositories tab, open RepoDetail, and Plugins tab).
pub(crate) fn show_catalog(ctx: &BridgeContext, ui: &CloudStream, catalog: ExtensionsCatalog) {
    ui.set_plugins_loading(false);
    if let Some(error) = &catalog.error {
        tracing::warn!(%error, "the plugin catalog is incomplete");
        ui.set_notice(format!("Some repositories could not be read: {error}").into());
    }
    {
        let mut session = ctx.session.borrow_mut();
        session.repositories = catalog.repositories;
        session.available = catalog.plugins;
        session.installed_times = catalog.installed_times;
    }
    show_repositories(ctx);
    show_new_catalog(ctx, ui);
    sync_open_repo_detail(ctx);
}

pub(crate) fn available_row(
    hit: &PluginHit,
    installed: &HashMap<String, i64>,
    show_repository: bool,
) -> AvailablePlugin {
    let plugin = &hit.plugin;
    let mut parts = vec![format!("v{}", plugin.version)];
    if show_repository && !hit.repository_name.is_empty() {
        parts.push(hit.repository_name.clone());
    }
    if let Some(language) = plugin
        .language
        .as_deref()
        .filter(|language| !language.is_empty())
    {
        parts.push(language.to_owned());
    }
    if let Some(size) = format_file_size(plugin.file_size) {
        parts.push(size);
    }
    AvailablePlugin {
        key: plugin_key(hit).into(),
        name: plugin.name.clone().into(),
        detail: parts.join(" · ").into(),
        description: plugin.description.clone().unwrap_or_default().into(),
        installed: installed.contains_key(&plugin.internal_name),
    }
}

/// Names a plugin by its repository and internal name.
pub(crate) fn plugin_key(hit: &PluginHit) -> String {
    format!("{}\n{}", hit.repository_url, hit.plugin.internal_name)
}

/// Shows the installed plugins in Settings.
pub(crate) fn refresh_plugins(ctx: &BridgeContext, ui: &CloudStream) {
    match ctx.services.extensions.installed() {
        Ok(list) => ui.set_plugins(plugin_rows(&list)),
        Err(error) => tracing::warn!(%error, "could not list the installed plugins"),
    }
}

/// The provider drop-downs; the chosen provider stays unless it is gone.
pub(crate) fn refresh_providers(ctx: &BridgeContext, ui: &CloudStream) {
    let names: Vec<String> = ctx
        .services
        .extensions
        .providers()
        .into_iter()
        .map(|info| info.name)
        .collect();
    let chosen = ui.get_selected_provider().to_string();
    if !names.contains(&chosen) {
        ui.set_selected_provider(names.first().cloned().unwrap_or_default().into());
    }
    ui.set_providers(string_model(names));
}
