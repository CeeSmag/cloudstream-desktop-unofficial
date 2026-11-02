use crate::actions::{Dispatcher, UiAction};
use crate::app_model::{AppModel, Loadable};
use crate::notice::Notice;
use crate::ui_event::UiEvent;

/// Applies a finished background job to the model.
pub(crate) fn apply(model: &mut AppModel, dispatcher: &Dispatcher, event: UiEvent) {
    match event {
        UiEvent::ExtensionsLoaded(result) => extensions_loaded(model, dispatcher, result),
        UiEvent::HomeLoaded { provider, rows } => {
            if model.selected_provider.as_deref() == Some(provider.as_str()) {
                model.home = rows.map_or_else(Loadable::Failed, Loadable::Ready);
            }
        }
        UiEvent::SearchFinished { query, outcomes } => {
            if query == model.search.last_query {
                model.search.loading = false;
                model.search.outcomes = outcomes;
            }
        }
        UiEvent::DetailsLoaded { url, result } => details_loaded(model, &url, result),
        UiEvent::StreamsResolved { target, result } => {
            crate::actions::handle_resolved(dispatcher, model, target, result);
        }
        UiEvent::RepositoryAdded(result) => repository_added(model, dispatcher, result),
        UiEvent::RepositoryPlugins { url, result } => {
            let state = result.map_or_else(Loadable::Failed, Loadable::Ready);
            model.settings.plugin_lists.insert(url, state);
        }
        UiEvent::PluginInstalled { name, result } => {
            plugin_installed(model, dispatcher, &name, result)
        }
        UiEvent::Notice(notice) => model.notify(notice),
    }
}

fn extensions_loaded(
    model: &mut AppModel,
    dispatcher: &Dispatcher,
    result: Result<app_services::ExtensionLoadReport, String>,
) {
    model.extensions_loading = false;
    match result {
        Ok(report) => {
            for (file, reason) in &report.failures {
                model.notify(Notice::error(format!(
                    "Extension {file} failed to load: {reason}"
                )));
            }
        }
        Err(error) => model.notify(Notice::error(format!("Could not load extensions: {error}"))),
    }
    model.providers = dispatcher.providers();
    select_default_provider(model, dispatcher);
}

fn select_default_provider(model: &mut AppModel, dispatcher: &Dispatcher) {
    let still_available = model.selected_provider_info().is_some();
    if still_available {
        return;
    }
    let first = model
        .providers
        .first()
        .map(|provider| provider.name.clone());
    if let Some(name) = first {
        dispatcher.perform(model, UiAction::SelectProvider(name));
    }
}

fn details_loaded(
    model: &mut AppModel,
    url: &str,
    result: Result<core_model::LoadDetails, String>,
) {
    for view in model
        .details_stack
        .iter_mut()
        .filter(|view| view.title_ref.url == url)
    {
        view.details = result
            .clone()
            .map_or_else(Loadable::Failed, Loadable::Ready);
    }
}

fn repository_added(
    model: &mut AppModel,
    dispatcher: &Dispatcher,
    result: Result<core_model::RepositoryEntry, String>,
) {
    model.settings.adding_repository = false;
    match result {
        Ok(entry) => {
            model.notify(Notice::info(format!("Added repository {}", entry.name)));
            model.settings.repository_input.clear();
            dispatcher.refresh_local_state(model);
            dispatcher.perform(model, UiAction::LoadRepositoryPlugins(entry.url));
        }
        Err(error) => model.notify(Notice::error(format!("Cannot add repository: {error}"))),
    }
}

fn plugin_installed(
    model: &mut AppModel,
    dispatcher: &Dispatcher,
    name: &str,
    result: Result<Vec<core_model::ProviderInfo>, String>,
) {
    model.settings.busy_plugins.remove(name);
    match result {
        Ok(providers) => {
            model.notify(Notice::info(format!(
                "Installed {name} ({} provider(s))",
                providers.len()
            )));
            dispatcher.refresh_local_state(model);
            select_default_provider(model, dispatcher);
        }
        Err(error) => model.notify(Notice::error(format!("Install of {name} failed: {error}"))),
    }
}
