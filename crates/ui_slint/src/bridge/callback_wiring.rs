//! Connects every Slint callback to its handler.

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::browse_filter_callbacks::wire_browse_filters;
use super::catalog_callbacks::wire_catalog;
use super::cloudflare_callbacks::wire_cloudflare;
use super::debug_logging::wire_debug_logging;
use super::details_callbacks::wire_details;
use super::extension_settings_callbacks::wire_extension_settings;
use super::focus_navigation::wire_navigation;
use super::home_callbacks::wire_home;
use super::library_actions::wire_library_actions;
use super::player_callbacks::wire_player;
use super::player_timeline::wire_timeline;
use super::player_track_callbacks::wire_player_tracks;
use super::plugin_list::wire_plugin_list;
use super::preference_callbacks::wire_preferences;
use super::source_picker::wire_source_picker;
use super::storage_settings_callbacks::wire_storage_settings;
use super::update_callbacks::wire_updates;

/// Wires every callback of the window.
pub(crate) fn wire_callbacks(ctx: &BridgeContext, ui: &CloudStream) {
    wire_home(ctx, ui);
    wire_library_actions(ctx, ui);
    wire_catalog(ctx, ui);
    wire_browse_filters(ctx, ui);
    wire_details(ctx, ui);
    wire_player(ctx, ui);
    wire_player_tracks(ctx, ui);
    wire_timeline(ctx, ui);
    wire_source_picker(ctx, ui);
    wire_preferences(ctx, ui);
    wire_debug_logging(ctx, ui);
    wire_storage_settings(ctx, ui);
    wire_extension_settings(ctx, ui);
    wire_plugin_list(ctx, ui);
    wire_updates(ctx, ui);
    wire_cloudflare(ctx, ui);
    wire_navigation(ctx, ui);
}
