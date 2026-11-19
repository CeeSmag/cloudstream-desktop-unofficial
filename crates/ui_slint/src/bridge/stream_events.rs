//! Shows how the opening of a video goes and starts the stream it picked.

use playback_session::{CheckedOpen, OpenProgress};

use crate::check_text::{can_skip_check, check_detail, check_fraction, check_title};
use crate::playback_watch::SourceCheck;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::source_picker::show_sources;
use super::stream_opening::start_selected;

pub(crate) fn show_open_progress(ui: &CloudStream, progress: OpenProgress) {
    let detail = check_detail(&progress);
    ui.set_open_title(check_title(&progress).into());
    ui.set_open_status(detail.clone().into());
    ui.set_open_fraction(check_fraction(&progress));
    ui.set_can_skip_checks(can_skip_check(&progress));
    let line = if detail.is_empty() {
        check_title(&progress).to_owned()
    } else {
        detail
    };
    ui.set_status_line(line.into());
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
    show_sources(ctx);
}
