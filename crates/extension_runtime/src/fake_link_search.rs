//! A scripted stand-in for the sidecar's streaming link search, for tests: it follows the same
//! rules as the real one (links and subtitles are reported as they are found, a caller asks with
//! cursors and may wait for news, a finished search that was fully read is forgotten, a cancelled
//! one is forgotten at once) and counts what was started, cancelled and is still running, so a
//! test can prove that nothing is left behind.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use core_model::LinkSearchCursor;
use serde_json::{json, Value};

/// How often a waiting poll looks for news.
const POLL_STEP: Duration = Duration::from_millis(2);

/// What a scripted search finds and when, counted from the moment it starts.
#[derive(Debug, Clone)]
pub struct SearchScript {
    /// Streams as `(found after, quality)`; the address is made from the position.
    pub links: Vec<(Duration, i32)>,
    /// Subtitles as `(found after, language)`.
    pub subtitles: Vec<(Duration, String)>,
    /// When the search ends.
    pub ends_after: Duration,
}

impl SearchScript {
    /// `count` streams of `quality`, one every `gap`, and the end `tail` after the last.
    pub fn steady(count: usize, gap: Duration, quality: i32, tail: Duration) -> Self {
        let links = (1..=count)
            .map(|n| (gap * u32::try_from(n).unwrap_or(1), quality))
            .collect();
        Self {
            links,
            subtitles: Vec::new(),
            ends_after: gap * u32::try_from(count).unwrap_or(1) + tail,
        }
    }

    /// Reads `"<count>,<gap ms>,<tail ms>"`, which is how a test tells the fake sidecar process
    /// what to find; anything else is a search that finds nothing at once.
    pub fn from_data(data: &str) -> Self {
        let numbers: Vec<u64> = data
            .split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        match numbers.as_slice() {
            [count, gap, tail] => Self::steady(
                usize::try_from(*count).unwrap_or(0),
                Duration::from_millis(*gap),
                720,
                Duration::from_millis(*tail),
            ),
            _ => Self::steady(0, Duration::ZERO, 0, Duration::ZERO),
        }
    }
}

/// What the fake has been asked to do so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchStats {
    /// Searches started.
    pub started: u64,
    /// Searches cancelled while they were still known.
    pub cancelled: u64,
    /// Searches that are still known: running, or finished but not fully read.
    pub active: usize,
}

struct RunningSearch {
    started_at: Instant,
    script: SearchScript,
}

/// The fake's searches.
pub struct FakeLinkSearches {
    script_for: Box<dyn Fn(&str) -> SearchScript + Send + Sync>,
    searches: Mutex<HashMap<String, RunningSearch>>,
    next_id: AtomicU64,
    started: AtomicU64,
    cancelled: AtomicU64,
}

impl FakeLinkSearches {
    /// A fake whose searches find what `script_for` says for the data they were started with.
    pub fn new(script_for: impl Fn(&str) -> SearchScript + Send + Sync + 'static) -> Self {
        Self {
            script_for: Box::new(script_for),
            searches: Mutex::default(),
            next_id: AtomicU64::new(1),
            started: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
        }
    }

    /// Starts a search and returns its id at once.
    pub fn start(&self, data: &str) -> String {
        let id = format!("search-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        self.started.fetch_add(1, Ordering::SeqCst);
        let search = RunningSearch {
            started_at: Instant::now(),
            script: (self.script_for)(data),
        };
        self.lock().insert(id.clone(), search);
        id
    }

    /// What search `id` found after the cursors, waiting up to `wait` for news.
    pub fn poll(&self, id: &str, from: LinkSearchCursor, wait: Duration) -> Result<Value, String> {
        let deadline = Instant::now() + wait;
        loop {
            let update = self.look(id, from)?;
            if update.has_news() || Instant::now() >= deadline {
                return Ok(update.into_value());
            }
            std::thread::sleep(POLL_STEP);
        }
    }

    /// Stops search `id`, which is forgotten at once.
    pub fn cancel(&self, id: &str) {
        if self.lock().remove(id).is_some() {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// What was started, cancelled and is still known.
    pub fn stats(&self) -> SearchStats {
        SearchStats {
            started: self.started.load(Ordering::SeqCst),
            cancelled: self.cancelled.load(Ordering::SeqCst),
            active: self.lock().len(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, RunningSearch>> {
        self.searches.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn look(&self, id: &str, from: LinkSearchCursor) -> Result<Update, String> {
        let mut searches = self.lock();
        let search = searches
            .get(id)
            .ok_or_else(|| format!("unknown link search: {id}"))?;
        let elapsed = search.started_at.elapsed();
        let links: Vec<i32> = search
            .script
            .links
            .iter()
            .filter(|(at, _)| *at <= elapsed)
            .map(|(_, quality)| *quality)
            .collect();
        let subtitles: Vec<String> = search
            .script
            .subtitles
            .iter()
            .filter(|(at, _)| *at <= elapsed)
            .map(|(_, language)| language.clone())
            .collect();
        let done = elapsed >= search.script.ends_after;
        let update = Update::new(links, subtitles, done, (from.links, from.subtitles));
        if done
            && update.next_link == update.total_links
            && update.next_subtitle == update.total_subtitles
        {
            searches.remove(id);
        }
        Ok(update)
    }
}

/// The reply of one poll, before it becomes JSON.
struct Update {
    links: Vec<(usize, i32)>,
    subtitles: Vec<(usize, String)>,
    total_links: usize,
    total_subtitles: usize,
    next_link: usize,
    next_subtitle: usize,
    done: bool,
}

impl Update {
    fn new(links: Vec<i32>, subtitles: Vec<String>, done: bool, from: (usize, usize)) -> Self {
        let (total_links, total_subtitles) = (links.len(), subtitles.len());
        Self {
            links: links.into_iter().enumerate().skip(from.0).collect(),
            subtitles: subtitles.into_iter().enumerate().skip(from.1).collect(),
            total_links,
            total_subtitles,
            next_link: total_links.max(from.0),
            next_subtitle: total_subtitles.max(from.1),
            done,
        }
    }

    fn has_news(&self) -> bool {
        !self.links.is_empty() || !self.subtitles.is_empty() || self.done
    }

    fn into_value(self) -> Value {
        let links: Vec<Value> = self
            .links
            .iter()
            .map(|(position, quality)| {
                json!({
                    "source": "Fake", "name": format!("Fake {quality}p #{position}"),
                    "url": format!("https://host{position}.example.test/stream-{position}.mp4"),
                    "referer": "", "quality": quality, "headers": {}, "extractorData": null,
                    "type": "VIDEO", "audioTracks": [],
                })
            })
            .collect();
        let subtitles: Vec<Value> = self
            .subtitles
            .iter()
            .map(|(position, language)| {
                json!({
                    "lang": language, "url": format!("https://subs.example.test/{position}.srt"),
                    "headers": null, "langTag": null,
                })
            })
            .collect();
        json!({
            "links": links, "subtitles": subtitles,
            "nextLink": self.next_link, "nextSubtitle": self.next_subtitle,
            "done": self.done, "success": self.done && self.total_links > 0, "failure": null,
        })
    }
}
