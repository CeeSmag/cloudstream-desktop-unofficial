//! Player controls: pause, seek, volume, sources, leave and close.

use player_api::PlaybackState;

use slint::ComponentHandle;

use crate::CloudStream;

use crate::feedback_text::{volume_text, wheel_volume_delta};

use super::bridge_context::BridgeContext;
use super::catalog_events::show_continue_watching;
use super::player_feedback::show_feedback;
use super::player_polling::clear_video_frame;
use super::player_seeking::wire_seeking;
use super::source_persist::persist_playing_sources;
use super::stream_opening::switch_source;
use super::stream_refresh::refresh_playing;

pub(crate) fn wire_player(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_toggle_pause(move || toggle_pause(&c));
    wire_seeking(ctx, ui);
    wire_volume(ctx, ui);
    let c = ctx.clone();
    ui.on_leave_player(move || leave_player(&c));
    wire_player_panels(ctx, ui);
}

/// The volume: the bar, the keys and the mouse wheel over the video.
fn wire_volume(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_volume(move |volume| {
        let volume = volume.clamp(0.0, 130.0);
        if let Some(playback) = &c.services.playback {
            report("volume", playback.set_volume(volume as u8));
        }
        if let Some(ui) = c.window() {
            ui.set_volume(volume);
            let icon = if volume <= 0.0 { "mute" } else { "volume" };
            show_feedback(&ui, icon, volume_text(volume), Some(volume / 100.0));
        }
    });
    let c = ctx.clone();
    ui.on_wheel_volume(move |delta_y| {
        let Some(ui) = c.window() else {
            return;
        };
        let carry = c.session.borrow().wheel_carry;
        let (delta, carry) = wheel_volume_delta(delta_y, carry);
        c.session.borrow_mut().wheel_carry = carry;
        if delta != 0.0 {
            ui.invoke_set_volume((ui.get_volume() + delta).clamp(0.0, 130.0));
        }
    });
}

fn wire_player_panels(ctx: &BridgeContext, ui: &CloudStream) {
    let weak = ctx.ui.clone();
    ui.on_toggle_sources(move || {
        if let Some(ui) = weak.upgrade() {
            let open = !ui.get_show_sources();
            if open {
                ui.set_player_open_panel(0);
            }
            ui.set_show_sources(open);
        }
    });
    let c = ctx.clone();
    ui.on_refresh_sources(move || {
        refresh_playing(&c);
        if let Some(ui) = c.window() {
            show_feedback(
                &ui,
                "source",
                "Searching for sources again".to_owned(),
                None,
            );
        }
    });
    let c = ctx.clone();
    ui.on_select_source(move |index| {
        if let Ok(index) = usize::try_from(index) {
            switch_source(&c, index);
        }
    });
    let weak = ctx.ui.clone();
    ui.on_toggle_fullscreen(move || {
        if let Some(ui) = weak.upgrade() {
            toggle_fullscreen(&ui);
        }
    });
}

fn toggle_fullscreen(ui: &CloudStream) {
    let fullscreen = !ui.window().is_fullscreen();
    tracing::debug!(fullscreen, "player fullscreen toggled");
    ui.window().set_fullscreen(fullscreen);
    ui.set_fullscreen(fullscreen);
    let (icon, text) = if fullscreen {
        ("fullscreen", "Fullscreen")
    } else {
        ("windowed", "Windowed")
    };
    show_feedback(ui, icon, text.to_owned(), None);
}

fn toggle_pause(ctx: &BridgeContext) {
    if let Some(playback) = &ctx.services.playback {
        let paused = playback.tick().state == PlaybackState::Paused;
        tracing::debug!(next_paused = !paused, "player pause toggled");
        report("pause", playback.set_paused(!paused));
        if let Some(ui) = ctx.window() {
            let (icon, text) = if paused {
                ("play", "Playing")
            } else {
                ("pause", "Paused")
            };
            show_feedback(&ui, icon, text.to_owned(), None);
        }
    }
}

fn leave_player(ctx: &BridgeContext) {
    // Drop any in-flight open so a late OpenReady cannot reopen the player.
    ctx.session.borrow_mut().abandon_open();
    persist_playing_sources(ctx);
    if let Some(playback) = &ctx.services.playback {
        report("stop", playback.stop());
    }
    if let Some(ui) = ctx.window() {
        reset_player_ui(ctx, &ui);
        show_continue_watching(ctx, &ui);
    }
}

fn reset_player_ui(ctx: &BridgeContext, ui: &CloudStream) {
    clear_video_frame(ctx, ui);
    ui.set_show_sources(false);
    ui.set_sources_loading(false);
    ui.set_sources_status("".into());
    ui.set_subtitles_loading(false);
    ui.set_subtitles_status("".into());
    ui.set_player_open_panel(0);
    ui.set_player_open(false);
}

/// Logs a player command that failed; the screen keeps showing the player's real state.
pub(crate) fn report<E: std::fmt::Display>(command: &str, result: Result<(), E>) {
    if let Err(error) = result {
        tracing::warn!(command, %error, "a player command failed");
    }
}
