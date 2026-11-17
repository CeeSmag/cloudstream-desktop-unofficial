//! The Refresh button of the player's Sources menu: the provider searches the playing video's
//! streams again and they are checked again, as if the video was opened for the first time, while
//! the stream that plays goes on playing.
//!
//! What was saved for the title is removed first, the list is started over with only the stream
//! that plays, and the old search and checks stop (their late results and saves are refused by
//! generation and by the saved sources' ticket). The viewer then picks from the new list as it
//! fills. A refresh is refused while no stream plays (the video is starting or switching) and when
//! another video started opening meanwhile.

use std::sync::Arc;
use std::thread::JoinHandle;

use app_services::link_rank;
use tauri::{AppHandle, Manager, State};
use title_cache::{CachedLink, LinkHealth};

use crate::app_state::AppState;
use crate::check_gate::CheckGate;
use crate::link_ledger::LinkLedger;
use crate::source_manager::{emit, CheckUpdate, SavedLinks, SourceManager, Tell};
use crate::video_opening::{search_request, services_for_playing, OpeningServices};

/// Message when there is nothing to refresh.
pub(crate) const NOTHING_PLAYS: &str = "The sources can be refreshed once the video plays.";
/// Message when the video changed while the refresh started.
pub(crate) const VIDEO_CHANGED: &str = "The video changed while its sources were being refreshed.";

/// Searches the playing video's sources again; returns once the new search has started.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn refresh_sources(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let services = services_for_playing(&state)?;
    let events = app.clone();
    let tell: Tell = Arc::new(move |update| emit(&events, update));
    // Starting the search waits for the extension runtime, so it runs off the async workers.
    let started = tauri::async_runtime::spawn_blocking(move || {
        let sources = app.state::<SourceManager>();
        refresh_playing(&sources, &services, tell)
    })
    .await
    .map_err(|error| error.to_string())?;
    // The search and the checks go on in the background until done or the video closes.
    started.map(|_background| ())
}

/// Starts the fresh search of the playing video and returns the threads doing the work.
pub(crate) fn refresh_playing(
    sources: &SourceManager,
    services: &OpeningServices,
    tell: Tell,
) -> Result<Vec<JoinHandle<()>>, String> {
    let playing = sources
        .session()
        .snapshot()
        .filter(|active| active.playing)
        .ok_or_else(|| NOTHING_PLAYS.to_owned())?;
    let search = services
        .streams
        .open_search(&playing.item.provider, &playing.item.media_key)
        .map_err(|error| error.to_string())?;
    let Some(opened) = sources.restart_list(playing.generation) else {
        search.cancel();
        tracing::info!("the video changed before its sources could be refreshed");
        return Err(VIDEO_CHANGED.to_owned());
    };
    tracing::info!("refreshing the sources of the playing video");
    let mut request = search_request(services, &playing.item, opened, search);
    request.saved = SavedLinks::Forget;
    tell(CheckUpdate::SourcesChanged);
    Ok(sources.discover_with(tell, request))
}

impl SourceManager {
    /// Gives the playing video of list `generation` a list with only the stream that plays and a
    /// new wait, and stops the old wait. `None`, and nothing changed, when another video started
    /// opening since that list was opened, or no stream plays. The check and the change happen
    /// under the opening's lock, as in [`SourceManager::begin`] and [`SourceManager::open`].
    pub(crate) fn restart_list(&self, generation: u64) -> Option<(Arc<CheckGate>, u64)> {
        let mut opening = self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if opening.generation != Some(generation) {
            return None;
        }
        let fresh = self.session().restart_playing(generation)?;
        let gate = Arc::new(CheckGate::new(self.limits));
        if let Some(old) = opening.gate.replace(Arc::clone(&gate)) {
            old.cancel();
        }
        opening.generation = Some(fresh);
        Some((gate, fresh))
    }

    /// Puts the streams already in list `generation` in `ledger` and `gate` as working, so the
    /// search does not list them twice. Only a refreshed list has any (the stream that plays); a
    /// new video's list is empty here.
    pub(crate) fn plant_playing(&self, gate: &CheckGate, generation: u64, ledger: &LinkLedger) {
        let Some(active) = self
            .session()
            .snapshot()
            .filter(|active| active.generation == generation)
        else {
            return;
        };
        if active.links.is_empty() {
            return;
        }
        let playing: Vec<CachedLink> = active
            .links
            .iter()
            .map(|link| CachedLink {
                link: link.clone(),
                health: LinkHealth::Working,
            })
            .collect();
        ledger.plant(&playing);
        let ranks: Vec<i32> = active.links.iter().map(link_rank).collect();
        gate.announce_saved(&ranks);
        for index in 0..ranks.len() {
            gate.record(index, true);
        }
    }
}

#[cfg(test)]
#[path = "source_refresh_tests.rs"]
mod tests;
