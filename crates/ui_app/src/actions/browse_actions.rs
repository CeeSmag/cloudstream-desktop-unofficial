use core_model::{Bookmark, HomePageSection};

use crate::app_model::{AppModel, DetailsView, Loadable};
use crate::navigation::{Tab, TitleRef};
use crate::notice::Notice;
use crate::ui_event::UiEvent;

use super::{Dispatcher, UiAction};

pub(super) fn perform(dispatcher: &Dispatcher, model: &mut AppModel, action: UiAction) {
    match action {
        UiAction::SwitchTab(tab) => switch_tab(dispatcher, model, tab),
        UiAction::SelectProvider(name) => {
            model.selected_provider = Some(name);
            refresh_home(dispatcher, model);
        }
        UiAction::RefreshHome => refresh_home(dispatcher, model),
        UiAction::Search(query) => search(dispatcher, model, query),
        UiAction::OpenTitle(title) => open_title(dispatcher, model, title),
        UiAction::CloseTitle => {
            model.details_stack.pop();
        }
        UiAction::ToggleBookmark => toggle_bookmark(dispatcher, model),
        _ => {}
    }
}

fn switch_tab(dispatcher: &Dispatcher, model: &mut AppModel, tab: Tab) {
    model.tab = tab;
    model.details_stack.clear();
    if tab == Tab::Library {
        refresh_library(dispatcher, model);
    }
}

pub(super) fn refresh_library(dispatcher: &Dispatcher, model: &mut AppModel) {
    let library = &dispatcher.services.library;
    match library.bookmarks() {
        Ok(bookmarks) => model.bookmarks = bookmarks,
        Err(error) => model.notify(Notice::error(format!("Cannot read your library: {error}"))),
    }
    match library.continue_watching() {
        Ok(items) => model.continue_watching = items,
        Err(error) => model.notify(Notice::error(format!("Cannot read watch history: {error}"))),
    }
}

pub(super) fn refresh_home(dispatcher: &Dispatcher, model: &mut AppModel) {
    let Some(provider) = model.selected_provider_info().cloned() else {
        model.home = Loadable::Idle;
        return;
    };
    if !provider.has_main_page {
        model.home = Loadable::Ready(Vec::new());
        return;
    }
    model.home = Loadable::Loading;
    let catalog = dispatcher.services.catalog.clone();
    dispatcher.runner.spawn("home", move || {
        let mut rows: Vec<HomePageSection> = Vec::new();
        let mut last_error = None;
        for entry in &provider.main_page {
            match catalog.home(&provider.name, entry) {
                Ok(Some(page)) => {
                    rows.extend(page.items.into_iter().filter(|row| !row.list.is_empty()))
                }
                Ok(None) => {}
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        let rows = match (rows.is_empty(), last_error) {
            (true, Some(error)) => Err(error),
            _ => Ok(rows),
        };
        UiEvent::HomeLoaded {
            provider: provider.name,
            rows,
        }
    });
}

fn search(dispatcher: &Dispatcher, model: &mut AppModel, query: String) {
    let query = query.trim().to_owned();
    if query.is_empty() {
        return;
    }
    model.search.loading = true;
    model.search.last_query.clone_from(&query);
    let providers = model.enabled_providers();
    let catalog = dispatcher.services.catalog.clone();
    dispatcher.runner.spawn("search", move || {
        let outcomes = catalog.search(&providers, &query);
        UiEvent::SearchFinished { query, outcomes }
    });
}

fn open_title(dispatcher: &Dispatcher, model: &mut AppModel, title: TitleRef) {
    let bookmarked = dispatcher
        .services
        .library
        .is_bookmarked(&title.provider, &title.url)
        .unwrap_or(false);
    model.details_stack.push(DetailsView {
        title_ref: title.clone(),
        details: Loadable::Loading,
        bookmarked,
    });
    let catalog = dispatcher.services.catalog.clone();
    dispatcher.runner.spawn("details", move || {
        let result = catalog
            .details(&title.provider, &title.url)
            .map_err(|error| error.to_string());
        UiEvent::DetailsLoaded {
            url: title.url,
            result,
        }
    });
}

fn toggle_bookmark(dispatcher: &Dispatcher, model: &mut AppModel) {
    let Some(view) = model.details_stack.last_mut() else {
        return;
    };
    let Loadable::Ready(details) = &view.details else {
        return;
    };
    let library = &dispatcher.services.library;
    let provider = view.title_ref.provider.clone();
    let url = view.title_ref.url.clone();
    let outcome = if view.bookmarked {
        library.remove_bookmark(&provider, &url)
    } else {
        library.add_bookmark(&Bookmark {
            provider,
            url,
            name: details.name.clone(),
            poster_url: details.poster_url.clone(),
            media_type: details.media_type.clone(),
            added_at_ms: dispatcher.services.clock.now_ms(),
        })
    };
    match outcome {
        Ok(()) => view.bookmarked = !view.bookmarked,
        Err(error) => model.notify(Notice::error(format!(
            "Cannot update your library: {error}"
        ))),
    }
}
