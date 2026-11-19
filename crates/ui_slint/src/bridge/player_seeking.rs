//! Jumps in the video: a click on the bar goes at once, the arrow keys and ±10 s are spaced out
//! ([`crate::seek_scheduler`]) and the bar holds the jump target while the player catches up.

use std::time::Instant;

use crate::feedback_text::seek_text;
use crate::seek_bar::format_time;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;
use super::player_feedback::show_feedback;

pub(crate) fn wire_seeking(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_seek_to(move |ms| jump_to(&c, ms as i64));
    let c = ctx.clone();
    ui.on_seek_by(move |delta| seek_by(&c, delta as i64));
}

/// A click or drag on the bar: goes at once, holds the target briefly, and forgets queued keys.
fn jump_to(ctx: &BridgeContext, target_ms: i64) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    ctx.session
        .borrow_mut()
        .seeks
        .send_now(target_ms, Instant::now());
    report("seek", playback.seek_to(target_ms));
    if let Some(ui) = ctx.window() {
        show_position(&ui, target_ms);
        show_feedback(
            &ui,
            "jump",
            format!("Jump to {}", format_time(target_ms)),
            None,
        );
    }
}

/// An arrow key or ±10 s: counted from where the last jump goes, sent now or a moment later.
fn seek_by(ctx: &BridgeContext, delta_ms: i64) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    let now = Instant::now();
    let status = playback.tick();
    let base = ctx
        .session
        .borrow()
        .seeks
        .seek_base(status.position_ms, now);
    let target = (base + delta_ms).clamp(0, status.duration_ms.max(0));
    let send = ctx.session.borrow_mut().seeks.press(target, now);
    if let Some(target) = send {
        report("seek", playback.seek_to(target));
    }
    if let Some(ui) = ctx.window() {
        show_position(&ui, target);
        let icon = if delta_ms < 0 { "back" } else { "forward" };
        show_feedback(&ui, icon, seek_text(delta_ms), None);
    }
}

/// Sends the waiting key jump once its time has come (called by the frame timer).
pub(crate) fn send_due_seek(ctx: &BridgeContext) {
    let due = ctx.session.borrow_mut().seeks.due(Instant::now());
    if let (Some(target), Some(playback)) = (due, &ctx.services.playback) {
        report("seek", playback.seek_to(target));
    }
}

/// Where the bar and the time say the video is: the jump's target while settling, else `playing_ms`.
pub(crate) fn shown_position(ctx: &BridgeContext, playing_ms: i64) -> i64 {
    ctx.session
        .borrow_mut()
        .seeks
        .shown_position(playing_ms, Instant::now())
}

fn show_position(ui: &CloudStream, position_ms: i64) {
    ui.set_position_ms(position_ms as f32);
    ui.set_position_text(format_time(position_ms).into());
}
