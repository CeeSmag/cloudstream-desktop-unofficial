//! Shows how the opening of a video goes, delivers ready subtitles, and starts the picked stream.

use playback_session::{CheckedOpen, OpenProgress};

use crate::check_text::{
    can_skip_check, check_detail, check_fraction, check_title, sources_loading_text,
};
use crate::playback_watch::SourceCheck;
use crate::ui_events::ReadySubtitleEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_polling::sync_subtitles_indicator;
use super::session::resolve_selected_index;
use super::source_picker::show_sources;
use super::stream_opening::start_selected;

pub(crate) fn show_open_progress(ctx: &BridgeContext, ui: &CloudStream, progress: OpenProgress) {
    let refreshing = {
        let mut session = ctx.session.borrow_mut();
        session.sources_searching = !progress.search_done;
        session.sources_refreshing
    };
    let detail = check_detail(&progress);
    ui.set_open_title(check_title(&progress).into());
    ui.set_open_status(detail.clone().into());
    ui.set_open_fraction(check_fraction(&progress));
    ui.set_can_skip_checks(can_skip_check(&progress));
    ui.set_sources_status(sources_loading_text(&progress, refreshing).into());
    let line = if detail.is_empty() {
        check_title(&progress).to_owned()
    } else {
        detail
    };
    ui.set_status_line(line.into());
    sync_subtitles_indicator(ctx, ui);
}

pub(crate) fn start_ready_stream(ctx: &BridgeContext, ui: &CloudStream, index: usize) {
    ctx.session.borrow_mut().selected = index;
    ui.set_player_open(true);
    ui.set_open_busy(false);
    start_selected(ctx, ui, index);
    sync_subtitles_indicator(ctx, ui);
}

pub(crate) fn show_open_update(ctx: &BridgeContext, ui: &CloudStream, open: CheckedOpen) {
    {
        let mut session = ctx.session.borrow_mut();
        let playing_url = session
            .links
            .get(session.selected)
            .map(|link| link.url.clone());
        session.links = open.links.iter().map(|(link, _)| link.clone()).collect();
        merge_provider_subtitles(&mut session.provider_subtitles, &open.subtitles);
        if let Some(delivery) = &session.subtitle_delivery {
            delivery.submit(&open.subtitles);
        }
        // Once playing, keep the playing source selected by URL (even when Refresh reorders links).
        if ui.get_player_open() {
            session.selected =
                resolve_selected_index(&session.links, playing_url.as_deref(), open.play_index);
        } else if let Some(index) = open.play_index {
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
        let mut published = open;
        published.subtitles.clone_from(&session.provider_subtitles);
        session.live.publish(published);
    }
    show_sources(ctx);
    sync_subtitles_indicator(ctx, ui);
}

/// Attaches a newly downloaded/normalized local subtitle file to the active stream (or keeps it
/// ready for when playback starts / switches source).
pub(crate) fn on_subtitle_ready(ctx: &BridgeContext, ui: &CloudStream, ready: ReadySubtitleEvent) {
    let added = {
        let mut session = ctx.session.borrow_mut();
        session.delivered_subtitle_urls.insert(ready.remote_url);
        let duplicate = session
            .ready_subtitles
            .iter()
            .any(|known| known.url == ready.track.url);
        if duplicate {
            None
        } else {
            session.ready_subtitles.push(ready.track.clone());
            Some(ready.track)
        }
    };
    if let Some(track) = added {
        if ui.get_player_open() {
            if let Some(playback) = &ctx.services.playback {
                playback.add_subtitles(&[track]);
            }
        }
    }
    sync_subtitles_indicator(ctx, ui);
}

/// Marks the background open or refresh pass complete and closes subtitle discovery.
pub(crate) fn on_open_finished(ctx: &BridgeContext, ui: &CloudStream) {
    {
        let mut session = ctx.session.borrow_mut();
        session.sources_loading = false;
        session.sources_searching = false;
        session.sources_refreshing = false;
        if let Some(delivery) = &session.subtitle_delivery {
            delivery.finish_discovery();
        }
    }
    ui.set_sources_loading(false);
    ui.set_sources_status("".into());
    sync_subtitles_indicator(ctx, ui);
}

fn merge_provider_subtitles(
    known: &mut Vec<core_model::SubtitleTrack>,
    incoming: &[core_model::SubtitleTrack],
) {
    for track in incoming {
        if !known.iter().any(|existing| existing.url == track.url) {
            known.push(track.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use core_model::{PlayableLink, SubtitleTrack};

    use super::*;

    fn link(url: &str) -> PlayableLink {
        PlayableLink {
            url: url.to_owned(),
            ..PlayableLink::default()
        }
    }

    fn sub(lang: &str, url: &str) -> SubtitleTrack {
        SubtitleTrack {
            lang: lang.to_owned(),
            url: url.to_owned(),
            headers: None,
        }
    }

    #[test]
    fn resolve_selected_index_follows_playing_url_across_reordered_refresh() {
        let links = [link("https://a"), link("https://b"), link("https://c")];
        assert_eq!(
            resolve_selected_index(&links, Some("https://b"), Some(0)),
            1
        );
        assert_eq!(
            resolve_selected_index(&links, Some("https://gone"), Some(2)),
            2
        );
        assert_eq!(
            resolve_selected_index(&links, Some("https://gone"), Some(99)),
            0
        );
    }

    #[test]
    fn merge_provider_subtitles_preserves_existing_and_deduplicates_on_refresh() {
        let mut known = vec![sub("English", "https://s/en.vtt")];
        merge_provider_subtitles(&mut known, &[]);
        assert_eq!(
            known.len(),
            1,
            "empty refresh batch must not wipe known subs"
        );
        merge_provider_subtitles(
            &mut known,
            &[
                sub("English", "https://s/en.vtt"),
                sub("Arabic", "https://s/ar.vtt"),
            ],
        );
        let urls: Vec<&str> = known.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, ["https://s/en.vtt", "https://s/ar.vtt"]);
    }
}
