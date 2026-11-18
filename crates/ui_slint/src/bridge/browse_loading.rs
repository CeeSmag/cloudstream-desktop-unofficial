//! Loads the Browse screen's TMDB titles in the background.

use app_services::{BrowseFilterRequest, BrowseService};

use crate::background_work::spawn_work;
use crate::request_tickets::RequestKind;
use crate::ui_events::{BrowseRow, UiEvent};

use super::bridge_context::BridgeContext;

/// Loads the most popular titles of `kind` ("movie" or "tv").
pub(crate) fn load_browse(ctx: &BridgeContext, kind: String) {
    let ticket = ctx.issue_ticket(RequestKind::Browse);
    let sender = ctx.sender();
    spawn_work("browse", move || match fetch_popular(&kind) {
        Ok(rows) => sender.send(UiEvent::BrowseLoaded(ticket, rows)),
        Err(error) => {
            tracing::warn!(%kind, %error, "Browse could not load");
            sender.send(UiEvent::BrowseLoaded(ticket, Vec::new()));
        }
    });
}

fn fetch_popular(kind: &str) -> Result<Vec<BrowseRow>, String> {
    let request = BrowseFilterRequest {
        media_type: kind.to_owned(),
        year: None,
        sort_by: "popular".into(),
        rating: None,
        genres: Vec::new(),
        genre: None,
        page: 1,
    };
    let page = BrowseService::default().fetch_browse_page(&request)?;
    Ok(page
        .items
        .into_iter()
        .map(|item| BrowseRow {
            title: item.name,
            subtitle: item.media_type,
            poster: item.poster_url.unwrap_or_default(),
            url: item.id,
        })
        .collect())
}
