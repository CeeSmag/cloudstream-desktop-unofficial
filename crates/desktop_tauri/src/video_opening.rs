//! The steps of opening a video that the `play_media` command strings together: starting the
//! search, waiting for the first good stream, and starting it.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use app_services::{LinkSearch, PlaybackItem, PlaybackService, StreamResolver};
use tauri::State;

use crate::app_state::AppState;
use crate::check_gate::{CheckGate, StartDecision};
use crate::dto::PlayRequestDto;
use crate::source_manager::{SearchRequest, SourceManager, Tell};

/// Message of the error `play_media` returns when the video is closed before it could start.
pub(crate) const CLOSED_BEFORE_START: &str = "The video was closed before it started.";

pub(crate) fn item_from(request: PlayRequestDto) -> PlaybackItem {
    PlaybackItem {
        provider: request.provider,
        media_key: request.media_key,
        title: request.title,
        detail_url: request.detail_url.unwrap_or_default(),
        poster_url: request.poster_url,
    }
}

/// The services opening a video needs, taken out so the lock is not held while waiting.
pub(crate) struct OpeningServices {
    pub(crate) streams: Arc<StreamResolver>,
    pub(crate) playback: Arc<PlaybackService>,
}

pub(crate) fn services_for_playing(state: &State<'_, AppState>) -> Result<OpeningServices, String> {
    let services = state.read();
    let playback = services
        .playback
        .clone()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    Ok(OpeningServices {
        streams: Arc::clone(&services.streams),
        playback,
    })
}

/// Opens a video: starts the provider's search, waits until the best working stream may play
/// (never longer than the gate's limit), and plays it. Returns how that ended and the threads that
/// go on searching and checking in the background; they end when the search and the checks are over
/// or the video is closed.
pub(crate) fn open_video(
    sources: &SourceManager,
    services: &OpeningServices,
    item: PlaybackItem,
    tell: Tell,
) -> (Result<(), String>, Vec<JoinHandle<()>>) {
    let gate = sources.begin();
    let opened_at = Instant::now();
    let (generation, search) = match start_search(sources, &gate, &services.streams, &item) {
        Ok(started) => started,
        Err(error) => return (Err(error), Vec::new()),
    };
    let late_subtitles = Arc::clone(&services.playback);
    let background = sources.discover_with(
        tell,
        SearchRequest {
            gate: Arc::clone(&gate),
            generation,
            search,
            give_subtitles: Arc::new(move |subtitles| late_subtitles.add_subtitles(subtitles)),
        },
    );
    let outcome = wait_for_start(&gate).and_then(|start| {
        tracing::info!(
            start,
            elapsed_ms = opened_at.elapsed().as_millis(),
            "starting playback"
        );
        start_stream(
            sources,
            &services.playback,
            StreamStart { item, start, gate },
        )
    });
    (outcome, background)
}

/// Starts the provider's search and the list of streams for `item`. Fails when the viewer already
/// left or opened another video, and then the search just started is stopped again.
pub(crate) fn start_search(
    sources: &SourceManager,
    gate: &Arc<CheckGate>,
    streams: &StreamResolver,
    item: &PlaybackItem,
) -> Result<(u64, LinkSearch), String> {
    let search = streams
        .open_search(&item.provider, &item.media_key)
        .map_err(|error| error.to_string())?;
    match sources.open(gate, item.clone()) {
        Some(generation) => Ok((generation, search)),
        None => {
            search.cancel();
            tracing::info!("the video was replaced or closed while its search started");
            Err(CLOSED_BEFORE_START.to_owned())
        }
    }
}

/// Waits until playback may begin and says which stream to play.
fn wait_for_start(gate: &CheckGate) -> Result<usize, String> {
    match gate.wait_for_start() {
        StartDecision::Start(index) => Ok(index),
        StartDecision::Cancelled => Err(CLOSED_BEFORE_START.to_owned()),
        StartDecision::Failed(reason) => Err(reason),
    }
}

/// What starting the stream needs besides the services: the item, which stream, and the wait.
pub(crate) struct StreamStart {
    pub(crate) item: PlaybackItem,
    pub(crate) start: usize,
    pub(crate) gate: Arc<CheckGate>,
}

/// Plays the stream at `start` of the open video and hands the player the subtitles found so far.
/// If the viewer closed the video while it was starting, the stream is stopped again.
pub(crate) fn start_stream(
    sources: &SourceManager,
    playback: &PlaybackService,
    request: StreamStart,
) -> Result<(), String> {
    let StreamStart { item, start, gate } = request;
    let active = sources
        .session()
        .start_snapshot()
        .ok_or_else(|| CLOSED_BEFORE_START.to_owned())?;
    let link = active
        .links
        .get(start)
        .ok_or_else(|| "No playable stream links found".to_string())?;
    tracing::info!(found = active.links.len(), url = %link.url, "playing stream");
    playback
        .start(item, link, &active.subtitles)
        .map_err(|error| error.to_string())?;
    if gate.is_cancelled() {
        tracing::info!("the video was closed while its stream started; stopping it");
        if let Err(error) = playback.stop() {
            tracing::warn!(%error, "could not stop the stream of a closed video");
        }
        return Err(CLOSED_BEFORE_START.to_owned());
    }
    sources.session().mark_selected(start);
    let late = sources.session().unsent_subtitles(active.generation);
    if !late.is_empty() {
        playback.add_subtitles(&late);
    }
    Ok(())
}
