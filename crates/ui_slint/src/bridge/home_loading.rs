//! Loads the Home shelves of the chosen provider in the background.

use app_services::UiServices;
use core_model::{MainPageEntry, ProviderInfo, SearchResult};

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;

use super::bridge_context::BridgeContext;

/// A shelf title and its titles.
pub(crate) type HomeShelf = (String, Vec<SearchResult>);

/// Loads every main-page shelf of the selected provider (or the first one).
pub(crate) fn load_home(ctx: &BridgeContext) {
    let services = ctx.owned_services();
    let sender = ctx.sender();
    let provider = ctx
        .window()
        .map(|ui| ui.get_selected_provider().to_string())
        .unwrap_or_default();
    spawn_work("home", move || {
        let providers = services.extensions.providers();
        let chosen = providers
            .iter()
            .find(|info| info.name == provider)
            .or_else(|| providers.first());
        let sections = chosen
            .map(|info| provider_shelves(&services, info))
            .unwrap_or_default();
        sender.send(UiEvent::HomeLoaded(sections));
    });
}

fn provider_shelves(services: &UiServices, info: &ProviderInfo) -> Vec<HomeShelf> {
    info.main_page
        .iter()
        .flat_map(|entry| entry_shelves(services, info, entry))
        .collect()
}

fn entry_shelves(
    services: &UiServices,
    info: &ProviderInfo,
    entry: &MainPageEntry,
) -> Vec<HomeShelf> {
    match services.catalog.home(&info.name, entry) {
        Ok(Some(page)) => page
            .items
            .into_iter()
            .map(|section| {
                let title = if section.name.is_empty() {
                    entry.name.clone()
                } else {
                    section.name
                };
                (title, section.list)
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(error) => {
            tracing::warn!(provider = %info.name, shelf = %entry.name, %error, "a Home shelf could not be loaded");
            Vec::new()
        }
    }
}
