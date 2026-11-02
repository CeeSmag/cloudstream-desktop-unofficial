use app_services::StreamSelection;
use core_model::{PlayableLink, SubtitleTrack};
use player_api::TrackKind;

use crate::app_model::{AppModel, PendingStreams, PlayTarget, PlayerMenu, PlayerView, SourceSet};
use crate::notice::Notice;
use crate::ui_event::UiEvent;

use super::{Dispatcher, UiAction};

pub(super) fn perform(dispatcher: &Dispatcher, model: &mut AppModel, action: UiAction) {
    match action {
        UiAction::Play(target) => resolve_streams(dispatcher, model, target),
        UiAction::PlayLink(index) => choose_link(dispatcher, model, index),
        UiAction::DismissStreamChoice => model.pending_streams = None,
        UiAction::SetPaused(paused) => control(model, |p| p.set_paused(paused), dispatcher),
        UiAction::SeekBy(delta_ms) => {
            let target = (model.playback.position_ms + delta_ms).max(0);
            control(model, |p| p.seek_to(target), dispatcher);
        }
        UiAction::StopPlayback => {
            control(model, |p| p.stop(), dispatcher);
            model.player = crate::app_model::PlayerView::default();
        }
        UiAction::SeekTo(position_ms) => {
            control(model, |p| p.seek_to(position_ms.max(0)), dispatcher)
        }
        UiAction::TogglePause => {
            let paused = model.playback.state == player_api::PlaybackState::Paused;
            control(model, |p| p.set_paused(!paused), dispatcher);
        }
        UiAction::OpenPlayer => open_player(dispatcher, model),
        UiAction::TogglePlayerMenu(menu) => toggle_menu(model, menu),
        UiAction::SelectAudio(id) => select_track(dispatcher, model, TrackKind::Audio, id),
        UiAction::SelectSubtitle(id) => select_track(dispatcher, model, TrackKind::Subtitle, id),
        UiAction::SwitchSource(index) => switch_source(dispatcher, model, index),
        UiAction::ClosePlayer => {
            model.player.open = false;
            model.player.fullscreen = false;
        }
        UiAction::ToggleFullscreen => model.player.fullscreen = !model.player.fullscreen,
        _ => {}
    }
}

fn control(
    model: &mut AppModel,
    call: impl FnOnce(&app_services::PlaybackService) -> Result<(), app_services::ServiceError>,
    dispatcher: &Dispatcher,
) {
    let Some(playback) = &dispatcher.services.playback else {
        return;
    };
    if let Err(error) = call(playback) {
        model.notify(Notice::error(format!("Player: {error}")));
    }
}

fn resolve_streams(dispatcher: &Dispatcher, model: &mut AppModel, target: PlayTarget) {
    if dispatcher.services.playback.is_none() {
        let reason = dispatcher
            .services
            .player_problem
            .clone()
            .unwrap_or_else(|| "no player".to_owned());
        model.notify(Notice::error(format!("Cannot play: {reason}")));
        return;
    }
    model.resolving_streams = true;
    let streams = std::sync::Arc::clone(&dispatcher.services.streams);
    dispatcher.runner.spawn("resolve-streams", move || {
        let result = streams
            .resolve(&target.item.provider, &target.data)
            .map_err(|error| error.to_string());
        UiEvent::StreamsResolved { target, result }
    });
}

fn choose_link(dispatcher: &Dispatcher, model: &mut AppModel, index: usize) {
    let Some(pending) = model.pending_streams.take() else {
        return;
    };
    if index >= pending.links.len() {
        return;
    }
    let start = PlaybackStart {
        target: pending.target,
        links: pending.links,
        selected: index,
        subtitles: pending.subtitles,
    };
    begin(dispatcher, model, start);
}

/// Everything needed to start one stream.
pub(super) struct PlaybackStart {
    pub(super) target: PlayTarget,
    /// Every stream found, best first.
    pub(super) links: Vec<PlayableLink>,
    /// Index into `links` of the one to play.
    pub(super) selected: usize,
    pub(super) subtitles: Vec<SubtitleTrack>,
}

/// Starts playback, reporting failures as notices.
pub(super) fn begin(dispatcher: &Dispatcher, model: &mut AppModel, start: PlaybackStart) {
    let Some(playback) = &dispatcher.services.playback else {
        return;
    };
    let Some(link) = start.links.get(start.selected).cloned() else {
        return;
    };
    model.now_playing_title = Some(start.target.item.title.clone());
    let embedded = playback.is_embedded();
    if let Err(error) = playback.start(start.target.item.clone(), &link, &start.subtitles) {
        model.notify(Notice::error(format!("Cannot start playback: {error}")));
        model.now_playing_title = None;
        return;
    }
    model.sources = SourceSet {
        target: Some(start.target),
        links: start.links,
        subtitles: start.subtitles,
        selected: start.selected,
    };
    model.tracks.clear();
    if embedded {
        model.playback = loading_status();
        model.player = PlayerView {
            open: true,
            just_opened: true,
            ..PlayerView::default()
        };
    }
}

fn loading_status() -> player_api::PlaybackStatus {
    player_api::PlaybackStatus {
        state: player_api::PlaybackState::Loading,
        ..player_api::PlaybackStatus::default()
    }
}

/// Decides between starting right away and asking the user to pick a stream.
pub(crate) fn handle_resolved(
    dispatcher: &Dispatcher,
    model: &mut AppModel,
    target: PlayTarget,
    result: Result<core_model::PlayableLinks, String>,
) {
    model.resolving_streams = false;
    match result {
        Err(error) => model.notify(Notice::error(format!("Could not load streams: {error}"))),
        Ok(found) if found.links.is_empty() => {
            model.notify(Notice::error(
                "No playable streams were found for this item.",
            ));
        }
        Ok(found) => match found.links.as_slice() {
            links if links.len() == 1 || same_source(links) => {
                let start = PlaybackStart {
                    target,
                    links: found.links.clone(),
                    selected: 0,
                    subtitles: found.subtitles.clone(),
                };
                begin(dispatcher, model, start);
            }
            _ => {
                model.pending_streams = Some(PendingStreams {
                    target,
                    links: found.links,
                    subtitles: found.subtitles,
                });
            }
        },
    }
}

fn open_player(dispatcher: &Dispatcher, model: &mut AppModel) {
    let embedded = dispatcher
        .services
        .playback
        .as_ref()
        .is_some_and(|playback| playback.is_embedded());
    if embedded && model.now_playing_title.is_some() {
        model.player.open = true;
        model.player.just_opened = true;
    }
}

/// True when every link comes from the same source, so the best one can simply be played
/// (a picker is only useful when different hosters offer different streams).
fn same_source(links: &[core_model::PlayableLink]) -> bool {
    links
        .windows(2)
        .all(|pair| pair.first().map(|a| &a.source) == pair.get(1).map(|b| &b.source))
}

fn toggle_menu(model: &mut AppModel, menu: PlayerMenu) {
    model.player.menu = if model.player.menu == Some(menu) {
        None
    } else {
        Some(menu)
    };
}

fn select_track(dispatcher: &Dispatcher, model: &mut AppModel, kind: TrackKind, id: Option<i64>) {
    model.player.menu = None;
    control(
        model,
        |playback| playback.select_track(kind, id),
        dispatcher,
    );
}

fn switch_source(dispatcher: &Dispatcher, model: &mut AppModel, index: usize) {
    model.player.menu = None;
    let Some(playback) = &dispatcher.services.playback else {
        return;
    };
    let Some(target) = model.sources.target.clone() else {
        return;
    };
    let Some(link) = model.sources.links.get(index).cloned() else {
        return;
    };
    let position = model.playback.position_ms;
    let stream = StreamSelection {
        link: &link,
        subtitles: &model.sources.subtitles,
        start_position_ms: position,
    };
    match playback.start_at(target.item, &stream) {
        Ok(()) => {
            model.sources.selected = index;
            model.tracks.clear();
            model.playback = loading_status();
        }
        Err(error) => model.notify(Notice::error(format!("Cannot switch source: {error}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(source: &str) -> core_model::PlayableLink {
        core_model::PlayableLink {
            source: source.into(),
            url: "u".into(),
            ..core_model::PlayableLink::default()
        }
    }

    #[test]
    fn same_source_is_true_for_one_hoster_only() {
        assert!(same_source(&[link("A"), link("A"), link("A")]));
        assert!(!same_source(&[link("A"), link("B")]));
        assert!(same_source(&[]));
    }
}
