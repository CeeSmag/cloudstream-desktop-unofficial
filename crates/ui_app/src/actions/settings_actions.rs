use std::sync::Arc;

use crate::app_model::{AppModel, Loadable};
use crate::notice::Notice;
use crate::ui_event::UiEvent;

use super::{browse_actions, Dispatcher, UiAction};

pub(super) fn perform(dispatcher: &Dispatcher, model: &mut AppModel, action: UiAction) {
    match action {
        UiAction::AddRepository(url) => add_repository(dispatcher, model, url),
        UiAction::RemoveRepository(url) => {
            report(
                model,
                "Cannot remove the repository",
                dispatcher.services.repositories.remove(&url),
            );
            refresh_local_state(dispatcher, model);
        }
        UiAction::LoadRepositoryPlugins(url) => load_plugins(dispatcher, model, url),
        UiAction::InstallPlugin {
            repository_url,
            listing,
        } => install(dispatcher, model, repository_url, listing),
        other => perform_extension_change(dispatcher, model, other),
    }
}

fn perform_extension_change(dispatcher: &Dispatcher, model: &mut AppModel, action: UiAction) {
    let services = &dispatcher.services;
    match action {
        UiAction::UninstallExtension(extension) => {
            report(
                model,
                "Cannot uninstall",
                services.extensions.uninstall(&extension),
            );
            model.notify(Notice::info(
                "Restart the app to fully unload an uninstalled extension.",
            ));
        }
        UiAction::SetTrusted { extension, trusted } => {
            let outcome = services.extensions.set_trusted(&extension, trusted);
            report(model, "Cannot change trust", outcome);
            model.notify(Notice::info(
                "Trust changes apply the next time the app starts.",
            ));
        }
        UiAction::SetProviderEnabled { provider, enabled } => {
            let outcome = services.settings.set_provider_enabled(&provider, enabled);
            report(model, "Cannot save the setting", outcome);
        }
        _ => return,
    }
    refresh_local_state(dispatcher, model);
}

fn report<T>(model: &mut AppModel, context: &str, outcome: Result<T, app_services::ServiceError>) {
    if let Err(error) = outcome {
        model.notify(Notice::error(format!("{context}: {error}")));
    }
}

/// Reloads everything that lives in the local database.
pub(super) fn refresh_local_state(dispatcher: &Dispatcher, model: &mut AppModel) {
    let services = &dispatcher.services;
    match services.repositories.list() {
        Ok(list) => model.settings.repositories = list,
        Err(error) => model.notify(Notice::error(format!("Cannot read repositories: {error}"))),
    }
    match services.extensions.installed() {
        Ok(list) => model.settings.installed = list,
        Err(error) => model.notify(Notice::error(format!(
            "Cannot read installed extensions: {error}"
        ))),
    }
    match services.settings.disabled_providers() {
        Ok(list) => model.settings.disabled_providers = list,
        Err(error) => model.notify(Notice::error(format!("Cannot read settings: {error}"))),
    }
    model.providers = services.extensions.providers();
    browse_actions::refresh_library(dispatcher, model);
}

pub(super) fn load_installed_extensions(dispatcher: &Dispatcher, model: &mut AppModel) {
    model.extensions_loading = true;
    let extensions = dispatcher.services.extensions.clone();
    dispatcher.runner.spawn("load-extensions", move || {
        UiEvent::ExtensionsLoaded(
            extensions
                .load_installed()
                .map_err(|error| error.to_string()),
        )
    });
}

fn add_repository(dispatcher: &Dispatcher, model: &mut AppModel, url: String) {
    model.settings.adding_repository = true;
    let repositories = dispatcher.services.repositories.clone();
    dispatcher.runner.spawn("add-repository", move || {
        UiEvent::RepositoryAdded(repositories.add(&url).map_err(|error| error.to_string()))
    });
}

fn load_plugins(dispatcher: &Dispatcher, model: &mut AppModel, url: String) {
    model
        .settings
        .plugin_lists
        .insert(url.clone(), Loadable::Loading);
    let repositories = dispatcher.services.repositories.clone();
    dispatcher.runner.spawn("repository-plugins", move || {
        let result = repositories
            .plugins(&url)
            .map_err(|error| error.to_string());
        UiEvent::RepositoryPlugins { url, result }
    });
}

fn install(
    dispatcher: &Dispatcher,
    model: &mut AppModel,
    repository_url: String,
    listing: core_model::PluginListing,
) {
    model
        .settings
        .busy_plugins
        .insert(listing.internal_name.clone());
    let extensions = Arc::new(dispatcher.services.extensions.clone());
    dispatcher.runner.spawn("install-plugin", move || {
        let name = listing.internal_name.clone();
        let result = extensions
            .install(&listing, &repository_url)
            .map_err(|error| error.to_string());
        UiEvent::PluginInstalled { name, result }
    });
}
