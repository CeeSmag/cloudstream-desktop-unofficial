//! Settings > Extensions and the Home provider: wiring. Every plugin or repository change runs off
//! the UI thread ([`super::extension_changes`]); installing used to freeze the window.

use std::time::{SystemTime, UNIX_EPOCH};

use slint::SharedString;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_events::show_continue_watching;
use super::extension_catalog::load_extensions_catalog;
use super::extension_changes::{
    add_repository, install_plugin, remove_repository, set_trusted, uninstall,
};
use super::home_expanded::close_expanded_shelf;
use super::home_loading::load_home;
use super::preference_callbacks::saved;

/// The Extensions page's index in the Settings side bar.
const EXTENSIONS_PAGE: i32 = 2;

pub(crate) fn wire_extension_settings(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_provider(move |name| set_provider(&c, name));
    let c = ctx.clone();
    ui.on_settings_page_shown(move |page| {
        if page == EXTENSIONS_PAGE {
            load_extensions_catalog(&c);
        }
    });
    let c = ctx.clone();
    ui.on_set_trusted(move |file, trusted| set_trusted(&c, file.to_string(), trusted));
    let c = ctx.clone();
    ui.on_uninstall_plugin(move |file| uninstall(&c, file.to_string()));
    let c = ctx.clone();
    ui.on_install_repo(move |url| add_repository(&c, url.trim().to_owned()));
    let c = ctx.clone();
    ui.on_remove_repo(move |url| remove_repository(&c, url.to_string()));
    let c = ctx.clone();
    ui.on_install_plugin(move |key| install_plugin(&c, &key));
}

fn set_provider(ctx: &BridgeContext, name: SharedString) {
    close_expanded_shelf(ctx);
    if let Some(ui) = ctx.window() {
        ui.set_selected_provider(name.clone());
        show_continue_watching(ctx, &ui);
    }
    saved(
        "home provider",
        ctx.services.settings.set_home_provider(&name),
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    saved(
        "provider switch time",
        ctx.services.settings.set_sources_valid_from(now),
    );
    let valid_from = ctx.services.title_cache.invalidate_now();
    tracing::info!(
        valid_from,
        "saved sources of the previous provider are no longer used"
    );
    load_home(ctx);
}
