//! Player controls: pause, seek, volume, sources, leave and close.

use player_api::PlaybackState;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::stream_opening::{refresh_playing, switch_source};

/// How far "Skip" jumps when no intro times are known.
const SKIP_JUMP_MS: i64 = 85_000;

pub(crate) fn wire_player(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_toggle_pause(move || toggle_pause(&c));
    let c = ctx.clone();
    ui.on_seek_to(move |ms| {
        if let Some(playback) = &c.services.playback {
            report("seek", playback.seek_to(ms as i64));
        }
    });
    let c = ctx.clone();
    ui.on_set_volume(move |volume| {
        if let Some(playback) = &c.services.playback {
            report(
                "volume",
                playback.set_volume(volume.clamp(0.0, 130.0) as u8),
            );
        }
    });
    let c = ctx.clone();
    ui.on_skip_segment(move || {
        if let Some(playback) = &c.services.playback {
            let position = playback.tick().position_ms;
            report("skip", playback.seek_to(position + SKIP_JUMP_MS));
        }
    });
    let c = ctx.clone();
    ui.on_close_player(move || close_player(&c));
    let c = ctx.clone();
    ui.on_leave_player(move || leave_player(&c));
    wire_player_panels(ctx, ui);
}

fn wire_player_panels(ctx: &BridgeContext, ui: &CloudStream) {
    let weak = ctx.ui.clone();
    ui.on_toggle_sources(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_sources(!ui.get_show_sources());
        }
    });
    let c = ctx.clone();
    ui.on_refresh_sources(move || refresh_playing(&c));
    let c = ctx.clone();
    ui.on_select_source(move |index| {
        if let Ok(index) = usize::try_from(index) {
            switch_source(&c, index);
        }
    });
    let weak = ctx.ui.clone();
    ui.on_reveal_player_chrome(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_player_chrome_visible(true);
        }
    });
}

fn toggle_pause(ctx: &BridgeContext) {
    if let Some(playback) = &ctx.services.playback {
        let paused = playback.tick().state == PlaybackState::Paused;
        report("pause", playback.set_paused(!paused));
    }
}

fn close_player(ctx: &BridgeContext) {
    if let Some(playback) = &ctx.services.playback {
        report("stop", playback.stop());
    }
    if let Some(ui) = ctx.window() {
        ui.set_player_open(false);
    }
}

fn leave_player(ctx: &BridgeContext) {
    if let Some(playback) = &ctx.services.playback {
        let keep_ready = ctx.services.settings.keep_video_ready().unwrap_or(true);
        match ctx.services.settings.video_cache() {
            Ok(cache) if keep_ready => report("leave", playback.leave(cache.while_away())),
            _ => report("stop", playback.stop()),
        }
    }
    if let Some(ui) = ctx.window() {
        ui.set_player_open(false);
    }
}

/// Logs a player command that failed; the screen keeps showing the player's real state.
pub(crate) fn report<E: std::fmt::Display>(command: &str, result: Result<(), E>) {
    if let Err(error) = result {
        tracing::warn!(command, %error, "a player command failed");
    }
}
