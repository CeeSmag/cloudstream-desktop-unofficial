//! Connects every Slint callback to its handler.

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::catalog_callbacks::wire_catalog;
use super::cloudflare_callbacks::wire_cloudflare;
use super::details_callbacks::wire_details;
use super::extension_settings_callbacks::wire_extension_settings;
use super::home_callbacks::wire_home;
use super::player_callbacks::wire_player;
use super::player_track_callbacks::wire_player_tracks;
use super::preference_callbacks::wire_preferences;
use super::storage_settings_callbacks::wire_storage_settings;
use super::update_callbacks::wire_updates;

/// Wires every callback of the window.
pub(crate) fn wire_callbacks(ctx: &BridgeContext, ui: &CloudStream) {
    wire_home(ctx, ui);
    wire_catalog(ctx, ui);
    wire_details(ctx, ui);
    wire_player(ctx, ui);
    wire_player_tracks(ctx, ui);
    wire_preferences(ctx, ui);
    wire_storage_settings(ctx, ui);
    wire_extension_settings(ctx, ui);
    wire_updates(ctx, ui);
    wire_cloudflare(ctx, ui);
}
