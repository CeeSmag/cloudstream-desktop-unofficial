//! Loads a title page in the background.

use crate::background_work::spawn_work;
use crate::posters::{empty_episodes, string_model};
use crate::request_tickets::RequestKind;
use crate::ui_events::{EpisodeRow, UiEvent};
use crate::PosterItem;

use super::bridge_context::BridgeContext;
use super::details_page::details_page;
use super::focus_navigation::focus_details_start;
use super::session::DetailsTarget;

/// Opens the title page of `item`.
pub(crate) fn load_details(ctx: &BridgeContext, item: &PosterItem) {
    let target = DetailsTarget {
        provider: item.provider.to_string(),
        url: item.url.to_string(),
        data: item.url.to_string(),
        media_key: item.media_key.to_string(),
        poster_url: Some(item.poster.to_string()).filter(|poster| !poster.is_empty()),
        resume_media_key: None,
        ..DetailsTarget::default()
    };
    ctx.session.borrow_mut().details = target.clone();
    let ticket = ctx.issue_ticket(RequestKind::Details);
    let fallback_title = item.title.to_string();
    show_details_pending(ctx, &fallback_title, "");
    let services = ctx.owned_services();
    let sender = ctx.sender();
    spawn_work("details", move || {
        match services.catalog.details(&target.provider, &target.url) {
            Ok(details) => {
                let page = details_page(&services, target, details, fallback_title);
                sender.send(UiEvent::DetailsLoaded(ticket, Box::new(page)));
            }
            Err(error) => {
                tracing::warn!(provider = %target.provider, %error, "title page failed to load");
                sender.send(UiEvent::DetailsFailed(ticket, error.to_string()));
            }
        }
    });
}

/// Shows the title page at once, empty, with a spinner and `status` ("" = "Loading the title…").
pub(crate) fn show_details_pending(ctx: &BridgeContext, title: &str, status: &str) {
    let Some(ui) = ctx.window() else {
        return;
    };
    ui.set_details_title(title.into());
    ui.set_details_plot(status.into());
    ui.set_details_meta("".into());
    ui.set_details_tags(empty_strings());
    ui.set_details_cast("".into());
    ui.set_play_label("Play".into());
    ui.set_resume_label("".into());
    ui.set_season_labels(empty_strings());
    ui.set_episodes(empty_episodes());
    ui.set_details_loading(true);
    focus_details_start(ctx, &ui);
}

/// Closes the pending title page and says why it could not be shown.
pub(crate) fn show_details_failed(ctx: &BridgeContext, message: &str) {
    let Some(ui) = ctx.window() else {
        return;
    };
    ui.set_details_loading(false);
    ui.set_details_title("".into());
    ui.set_notice(message.into());
}

/// "All" plus one label per known season, in order.
pub(crate) fn season_labels(episodes: &[EpisodeRow]) -> Vec<String> {
    let seasons: std::collections::BTreeSet<i32> = episodes
        .iter()
        .map(|row| row.season)
        .filter(|season| *season > 0)
        .collect();
    std::iter::once("All".to_owned())
        .chain(seasons.into_iter().map(|season| format!("Season {season}")))
        .collect()
}

/// The season number of a label from [`season_labels`]; -1 for "All".
pub(crate) fn parse_season_label(label: &str) -> i32 {
    label
        .strip_prefix("Season ")
        .and_then(|number| number.parse().ok())
        .unwrap_or(-1)
}

fn empty_strings() -> slint::ModelRc<slint::SharedString> {
    string_model(std::iter::empty())
}
