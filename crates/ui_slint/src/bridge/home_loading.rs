//! Loads the Home shelves of the chosen provider in the background.

use app_services::UiServices;
use core_model::{MainPageEntry, ProviderInfo, SearchResult};

use crate::background_work::spawn_work;
use crate::request_tickets::RequestKind;
use crate::ui_events::UiEvent;

use super::bridge_context::BridgeContext;

/// One provider shelf shown on Home, together with the metadata needed to page it when expanded.
#[derive(Debug, Clone)]
pub(crate) struct HomeShelf {
    pub(crate) title: String,
    pub(crate) items: Vec<SearchResult>,
    pub(crate) provider: String,
    pub(crate) entry: MainPageEntry,
    pub(crate) section_index: usize,
    pub(crate) has_next: bool,
}

/// Loads every main-page shelf of the selected provider (or the first one).
pub(crate) fn load_home(ctx: &BridgeContext) {
    let ticket = ctx.issue_ticket(RequestKind::Home);
    let services = ctx.owned_services();
    let sender = ctx.sender();
    let provider = ctx
        .window()
        .map(|ui| {
            ui.set_home_loading(true);
            ui.get_selected_provider().to_string()
        })
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
        sender.send(UiEvent::HomeLoaded(ticket, sections));
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
        Ok(Some(page)) => {
            let has_next = page.has_next;
            page.items
                .into_iter()
                .enumerate()
                .map(|(section_index, section)| {
                    let title = if section.name.is_empty() {
                        entry.name.clone()
                    } else {
                        section.name
                    };
                    HomeShelf {
                        title,
                        items: section.list,
                        provider: info.name.clone(),
                        entry: entry.clone(),
                        section_index,
                        has_next,
                    }
                })
                .collect()
        }
        Ok(None) => Vec::new(),
        Err(error) => {
            tracing::warn!(provider = %info.name, shelf = %entry.name, %error, "a Home shelf could not be loaded");
            Vec::new()
        }
    }
}
