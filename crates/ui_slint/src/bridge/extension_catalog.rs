//! The Extensions page's lists: repositories, the plugins they offer, the installed plugins.

use app_services::PluginHit;
use slint::{ModelRc, VecModel};

use crate::background_work::spawn_work;
use crate::posters::{plugin_rows, string_model};
use crate::ui_events::UiEvent;
use crate::{AvailablePlugin, CloudStream, RepositoryItem};

use super::bridge_context::BridgeContext;

/// What the Extensions page lists.
#[derive(Debug, Default)]
pub(crate) struct ExtensionsCatalog {
    pub(crate) repositories: Vec<core_model::RepositoryEntry>,
    pub(crate) plugins: Vec<PluginHit>,
    pub(crate) installed_names: Vec<String>,
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
        catalog.installed_names = services
            .extensions
            .installed()
            .map(|list| list.into_iter().map(|ext| ext.internal_name).collect())
            .unwrap_or_default();
        sender.send(UiEvent::ExtensionsLoaded(Box::new(catalog)));
    });
}

/// Shows the catalog on the Extensions page.
pub(crate) fn show_catalog(ctx: &BridgeContext, ui: &CloudStream, catalog: ExtensionsCatalog) {
    ui.set_plugins_loading(false);
    if let Some(error) = &catalog.error {
        tracing::warn!(%error, "the plugin catalog is incomplete");
        ui.set_notice(format!("Some repositories could not be read: {error}").into());
    }
    let repositories: Vec<RepositoryItem> = catalog
        .repositories
        .iter()
        .map(|repo| RepositoryItem {
            name: repo.name.clone().into(),
            url: repo.url.clone().into(),
        })
        .collect();
    ui.set_repositories(ModelRc::new(VecModel::from(repositories)));
    let available: Vec<AvailablePlugin> = catalog
        .plugins
        .iter()
        .map(|hit| available_row(hit, &catalog.installed_names))
        .collect();
    ui.set_available_plugins(ModelRc::new(VecModel::from(available)));
    ctx.session.borrow_mut().available = catalog.plugins;
}

fn available_row(hit: &PluginHit, installed: &[String]) -> AvailablePlugin {
    let plugin = &hit.plugin;
    let mut detail = format!("v{} · {}", plugin.version, hit.repository_name);
    if let Some(language) = plugin
        .language
        .as_deref()
        .filter(|language| !language.is_empty())
    {
        detail = format!("{detail} · {language}");
    }
    AvailablePlugin {
        key: plugin_key(hit).into(),
        name: plugin.name.clone().into(),
        detail: detail.into(),
        description: plugin.description.clone().unwrap_or_default().into(),
        installed: installed.contains(&plugin.internal_name),
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
