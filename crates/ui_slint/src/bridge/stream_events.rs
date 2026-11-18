//! Shows how the opening of a video goes and starts the stream it picked.

use playback_session::{CheckedOpen, OpenProgress};

use crate::posters::source_rows;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::stream_opening::start_selected;

pub(crate) fn show_open_progress(ui: &CloudStream, progress: OpenProgress) {
    let done = if progress.search_done {
        " · search done"
    } else {
        ""
    };
    let status = format!(
        "Streams {} · checked {} · working {}{done}",
        progress.found, progress.checked, progress.working
    );
    ui.set_open_status(status.clone().into());
    ui.set_status_line(status.into());
}

pub(crate) fn start_ready_stream(ctx: &BridgeContext, ui: &CloudStream, index: usize) {
    ctx.session.borrow_mut().selected = index;
    ui.set_player_open(true);
    ui.set_open_busy(false);
    start_selected(ctx, ui, index);
}

pub(crate) fn show_open_update(ctx: &BridgeContext, ui: &CloudStream, open: CheckedOpen) {
    {
        let mut session = ctx.session.borrow_mut();
        session.links = open.links.iter().map(|(link, _)| link.clone()).collect();
        session.subtitles.clone_from(&open.subtitles);
        if let Some(index) = open.play_index {
            session.selected = index;
        }
        session.live.publish(open.clone());
    }
    ui.set_sources(source_rows(&open.links, open.play_index));
}
