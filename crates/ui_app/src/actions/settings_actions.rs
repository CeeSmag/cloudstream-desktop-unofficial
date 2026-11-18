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
            model.notify(Notice::info("Extension uninstalled."));
        }
        UiAction::SetTrusted { extension, trusted } => {
            let outcome = services.extensions.set_trusted(&extension, trusted);
            report(model, "Cannot change trust", outcome);
            model.notify(Notice::info(if trusted {
                "Extension marked trusted and reloaded with wider access."
            } else {
                "Extension returned to the sandbox and reloaded."
            }));
        }
        UiAction::SetProviderEnabled { provider, enabled } => {
            let outcome = services.settings.set_provider_enabled(&provider, enabled);
            report(model, "Cannot save the setting", outcome);
        }
        UiAction::SetDebugLogging(enabled) => {
            report(
                model,
                "Cannot save debug logging",
                services.settings.set_debug_logging_enabled(enabled),
            );
            let _ = services.extensions.set_debug_logging(enabled);
        }
        UiAction::SetStreamProxy(enabled) => {
            report(
                model,
                "Cannot save stream proxy",
                services.settings.set_stream_proxy(enabled),
            );
        }
        UiAction::SetSkipLookup(enabled) => {
            report(
                model,
                "Cannot save skip lookup",
                services.settings.set_skip_lookup(enabled),
            );
        }
        UiAction::SetAutoplayNext(enabled) => {
            report(
                model,
                "Cannot save autoplay",
                services.settings.set_autoplay_next(enabled),
            );
        }
        UiAction::SetPluginWorkerLimit(limit) => {
            let value = if limit == 0 {
                None
            } else {
                Some(limit as usize)
            };
            report(
                model,
                "Cannot save plugin worker limit",
                services.settings.set_plugin_worker_limit(value),
            );
            services.extensions.set_resident_worker_limit(value);
        }
        UiAction::CheckForUpdate => {
            model.settings.checking_update = true;
            model.settings.update_status = Some("Checking…".to_owned());
            let current = env!("CARGO_PKG_VERSION").to_owned();
            dispatcher.runner.spawn("check-update", move || {
                UiEvent::UpdateChecked(crate::update_check::check_for_update(&current))
            });
            return;
        }
        UiAction::OpenUpdateUrl => {
            if let Some(url) = &model.settings.update_url {
                if let Err(error) = open::that(url) {
                    model.notify(Notice::error(format!("Cannot open update URL: {error}")));
                }
            }
            return;
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
    if let Ok(value) = services.settings.debug_logging_enabled() {
        model.settings.debug_logging = value;
    }
    if let Ok(value) = services.settings.stream_proxy() {
        model.settings.stream_proxy = value;
    }
    if let Ok(value) = services.settings.skip_lookup() {
        model.settings.skip_lookup = value;
    }
    if let Ok(value) = services.settings.autoplay_next() {
        model.settings.autoplay_next = value;
    }
    if let Ok(limit) = services.settings.plugin_worker_limit() {
        model.settings.plugin_worker_limit = limit.unwrap_or(0) as u32;
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
