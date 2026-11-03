//! Brings the provider's subtitle files to the open video. The provider only names them; here they
//! are downloaded in the background (the viewer's languages first), saved as clean local files,
//! and handed to the video the moment each is ready. What a title was given before is offered at
//! once, so a title that once had Arabic subtitles keeps them when the provider's service is slow.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use core_model::SubtitleTrack;
use subtitle_store::{
    KeepGoing, LanguageRank, PrefetchConfig, ReadySubtitle, SubtitleLoader, SubtitlePrefetch,
};

use crate::source_discovery::SubtitleSink;
use crate::source_manager::{CheckUpdate, Tell};
use crate::source_session::SourceSession;

/// How many subtitle files are downloaded at the same time; the time goes to waiting on the host.
const DOWNLOADS_AT_ONCE: usize = 8;

/// What the opening of a video says about its subtitles.
#[derive(Clone)]
pub struct SubtitleWork {
    /// Identifies the title (provider and episode) in the saved lists.
    pub title_key: String,
    /// Which languages come first.
    pub rank_of: LanguageRank,
}

/// Everything [`Delivery`] needs.
pub(crate) struct DeliveryParts {
    pub(crate) session: Arc<SourceSession>,
    pub(crate) generation: u64,
    pub(crate) loader: Arc<SubtitleLoader>,
    pub(crate) work: SubtitleWork,
    pub(crate) tell: Tell,
    pub(crate) give_subtitles: SubtitleSink,
}

/// Puts ready subtitle files into the open video and, once it plays, into the player.
pub(crate) struct Delivery {
    parts: DeliveryParts,
    delivered: Mutex<HashSet<String>>,
}

impl Delivery {
    pub(crate) fn new(parts: DeliveryParts) -> Arc<Self> {
        Arc::new(Self {
            parts,
            delivered: Mutex::default(),
        })
    }

    /// Hands one ready file to the video; a file that was handed over before is ignored.
    /// `remember` saves it in the title's list for next time.
    pub(crate) fn deliver(&self, ready: &ReadySubtitle, remember: bool) {
        let parts = &self.parts;
        let new = self
            .delivered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(ready.url.clone());
        if !new {
            return;
        }
        let track = SubtitleTrack {
            lang: ready.language.clone(),
            url: ready.path.to_string_lossy().replace('\\', "/"),
            headers: None,
        };
        if parts
            .session
            .append(parts.generation, Vec::new(), vec![track])
            .is_none()
        {
            return;
        }
        if remember {
            let saved = parts
                .loader
                .cache()
                .remember_for_title(&parts.work.title_key, ready);
            if let Err(error) = saved {
                tracing::warn!(%error, "could not add the subtitle to the title's saved list");
            }
        }
        (parts.tell)(CheckUpdate::SourcesChanged);
        let fresh = parts.session.unsent_subtitles(parts.generation);
        if !fresh.is_empty() {
            (parts.give_subtitles)(&fresh);
        }
    }

    /// Offers what this title was given before, before the provider has said anything.
    pub(crate) fn offer_saved(&self) {
        let saved = self
            .parts
            .loader
            .cache()
            .cached_for_title(&self.parts.work.title_key);
        if !saved.is_empty() {
            tracing::info!(
                files = saved.len(),
                "subtitles this title was given before are offered at once"
            );
        }
        for ready in &saved {
            self.deliver(ready, false);
        }
    }

    /// Starts the background downloads; each file that becomes ready is delivered.
    pub(crate) fn start_downloads(
        self: &Arc<Self>,
        wanted: KeepGoing,
    ) -> (SubtitlePrefetch, Vec<JoinHandle<()>>) {
        let delivery = Arc::clone(self);
        let config = PrefetchConfig {
            workers: DOWNLOADS_AT_ONCE,
            rank_of: Arc::clone(&self.parts.work.rank_of),
        };
        SubtitlePrefetch::start(
            Arc::clone(&self.parts.loader),
            &config,
            Arc::new(move |ready| delivery.deliver(&ready, true)),
            wanted,
        )
    }
}
