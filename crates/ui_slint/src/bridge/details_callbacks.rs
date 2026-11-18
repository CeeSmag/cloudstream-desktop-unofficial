//! The title page: play, resume, episodes, seasons, close.

use app_services::PlaybackItem;
use slint::Model;

use crate::posters::empty_episodes;
use crate::request_tickets::RequestKind;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::details_loading::parse_season_label;
use super::home_callbacks::row;
use super::stream_opening::begin_open;

pub(crate) fn wire_details(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_play_details(move || play_details(&c));
    let weak = ctx.ui.clone();
    ui.on_resume_details(move || {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_play_details();
        }
    });
    let c = ctx.clone();
    ui.on_play_episode(move |index| play_episode(&c, index));
    let c = ctx.clone();
    ui.on_close_details(move || close_details(&c));
    let c = ctx.clone();
    ui.on_skip_checks(move || c.session.borrow().control.skip_checks());
    let weak = ctx.ui.clone();
    ui.on_season_chosen(move |label| {
        if let Some(ui) = weak.upgrade() {
            ui.set_season_filter(parse_season_label(&label));
        }
    });
}

/// Closes the title page; its late answer and a video still opening from it are dropped.
fn close_details(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    {
        let mut session = ctx.session.borrow_mut();
        session.tickets.cancel(RequestKind::Details);
        if ui.get_open_busy() {
            session.control.cancel_open();
            session.tickets.cancel(RequestKind::Open);
        }
    }
    ui.set_open_busy(false);
    ui.set_open_status("".into());
    ui.set_details_title("".into());
    ui.set_episodes(empty_episodes());
}

fn play_details(ctx: &BridgeContext) {
    let title = ctx
        .window()
        .map(|ui| ui.get_details_title().to_string())
        .unwrap_or_default();
    let details = ctx.session.borrow().details.clone();
    let item = PlaybackItem {
        provider: details.provider,
        media_key: details.media_key,
        title,
        detail_url: details.url,
        poster_url: None,
        season: None,
        episode: None,
        next_episode: None,
    };
    begin_open(ctx, item, details.data);
}

fn play_episode(ctx: &BridgeContext, index: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some(episode) = ui.get_episodes().row_data(row(index)) else {
        return;
    };
    let details = ctx.session.borrow().details.clone();
    let item = PlaybackItem {
        provider: details.provider,
        media_key: episode.data.to_string(),
        title: format!("{} — {}", ui.get_details_title(), episode.name),
        detail_url: details.url,
        poster_url: None,
        season: None,
        episode: None,
        next_episode: None,
    };
    begin_open(ctx, item, episode.data.to_string());
}
