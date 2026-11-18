//! Shows how the opening of a video goes and starts the stream it picked.

use playback_session::{CheckedOpen, OpenProgress};

use crate::playback_watch::SourceCheck;
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
        // Once playing, the playing source stays selected (the viewer or a fallback may have
        // switched away from the checker's first pick).
        if let (false, Some(index)) = (ui.get_player_open(), open.play_index) {
            session.selected = index;
        }
        session.source_checks = open
            .links
            .iter()
            .map(|(_, health)| match health {
                Some(health) if health.is_working() => SourceCheck::Working,
                Some(_) => SourceCheck::Broken,
                None => SourceCheck::Unchecked,
            })
            .collect();
        session.live.publish(open.clone());
    }
    let selected = ctx.session.borrow().selected;
    ui.set_sources(source_rows(&open.links, Some(selected)));
}
