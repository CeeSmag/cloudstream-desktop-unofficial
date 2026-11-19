//! Asks the online skip-time databases about the playing stream (like the old app's
//! `get_skip_segments`), when the viewer left "Skip intro lookup" on and the provider gave ids.

use std::sync::Arc;

use skip_times::SkipQuery;

use crate::background_work::spawn_work;
use crate::request_tickets::RequestKind;
use crate::skip_segments::{from_online, title_kind};
use crate::ui_events::UiEvent;

use super::bridge_context::BridgeContext;

/// Starts the lookup for a stream of `duration_ms`; a newer stream's lookup drops this answer.
pub(crate) fn start_online_lookup(ctx: &BridgeContext, duration_ms: i64) {
    if !ctx.services.settings.skip_lookup().unwrap_or(true) {
        return;
    }
    let Some(query) = query_for(ctx, duration_ms) else {
        return;
    };
    let ticket = ctx.issue_ticket(RequestKind::Skips);
    let lookup = Arc::clone(&ctx.services.skips);
    let sender = ctx.sender();
    spawn_work("skip-times", move || match lookup.find(&query) {
        Ok(found) if !found.is_empty() => {
            sender.send(UiEvent::SkipTimesFound(ticket, from_online(&found)));
        }
        Ok(_) => tracing::debug!("the skip-time databases know nothing about this video"),
        Err(error) => tracing::info!(%error, "online skip times could not be read"),
    });
}

/// The question for the playing title and episode; None without any id to ask by.
fn query_for(ctx: &BridgeContext, duration_ms: i64) -> Option<SkipQuery> {
    let session = ctx.session.borrow();
    let ids = session.details.skip_ids.clone();
    if ids.is_empty() {
        return None;
    }
    let item = session.target.as_ref()?;
    let number = |n: Option<i32>| n.and_then(|n| u32::try_from(n).ok());
    Some(SkipQuery {
        ids,
        kind: title_kind(&session.details.media_type, session.episodes.is_empty()),
        season: number(item.season),
        episode: number(item.episode),
        duration_ms: u64::try_from(duration_ms).unwrap_or(0),
    })
}
