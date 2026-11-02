use core_model::{PlayableLink, SubtitleTrack};

use crate::app_model::{AppModel, PendingStreams, PlayTarget};
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
    let Some(link) = pending.links.get(index).cloned() else {
        return;
    };
    let start = PlaybackStart {
        target: pending.target,
        link,
        subtitles: pending.subtitles,
    };
    begin(dispatcher, model, start);
}

/// Everything needed to start one stream.
pub(super) struct PlaybackStart {
    pub(super) target: PlayTarget,
    pub(super) link: PlayableLink,
    pub(super) subtitles: Vec<SubtitleTrack>,
}

/// Starts playback, reporting failures as notices.
pub(super) fn begin(dispatcher: &Dispatcher, model: &mut AppModel, start: PlaybackStart) {
    let Some(playback) = &dispatcher.services.playback else {
        return;
    };
    model.now_playing_title = Some(start.target.item.title.clone());
    let embedded = playback.is_embedded();
    if let Err(error) = playback.start(start.target.item, &start.link, &start.subtitles) {
        model.notify(Notice::error(format!("Cannot start playback: {error}")));
        model.now_playing_title = None;
    } else if embedded {
        model.playback = player_api::PlaybackStatus {
            state: player_api::PlaybackState::Loading,
            ..player_api::PlaybackStatus::default()
        };
        model.player = crate::app_model::PlayerView {
            open: true,
            just_opened: true,
            ..crate::app_model::PlayerView::default()
        };
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
            [only] => {
                let start = PlaybackStart {
                    target,
                    link: only.clone(),
                    subtitles: found.subtitles.clone(),
                };
                begin(dispatcher, model, start);
            }
            links if same_source(links) => {
                let start = PlaybackStart {
                    target,
                    link: links.first().cloned().unwrap_or_default(),
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
