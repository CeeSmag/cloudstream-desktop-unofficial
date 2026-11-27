//! Test world, trait doubles, and local mock HTTP helpers shared across BDD steps.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use app_services::Clock;
use core_model::{
    HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry, PlayableLink,
    PlayableLinks, ProviderInfo, SearchPage, SubtitleTrack,
};
use cucumber::World;
use extension_api::{GatewayError, ProviderGateway};
use extension_repo::{HttpFetcher, RepoError};
use player_api::{
    Chapter, PlaybackRequest, PlaybackState, PlaybackStatus, PlayerError, VideoPlayer,
};
use skip_times::{SkipError, SkipFetcher, SkipKind, SkipSegment};
use storage::Database;
use subtitle_store::{FetchError, ReadySubtitle, SubtitleFetcher};

pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

pub struct MapFetcher(pub Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl HttpFetcher for MapFetcher {
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
        self.0
            .lock()
            .expect("fetcher")
            .get(url)
            .cloned()
            .ok_or_else(|| RepoError::Download {
                url: url.into(),
                reason: "not found".into(),
            })
    }
}

pub struct MapSubFetcher(pub Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl SubtitleFetcher for MapSubFetcher {
    fn fetch(&self, url: &str, _headers: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.0
            .lock()
            .expect("sub fetcher")
            .get(url)
            .cloned()
            .ok_or(FetchError::Status { status: 404 })
    }
}

pub struct StaticSkipFetcher(pub String);

impl SkipFetcher for StaticSkipFetcher {
    fn get_text(&self, _url: &str) -> Result<String, SkipError> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
pub struct BddGateway {
    pub providers_on_load: Mutex<Vec<ProviderInfo>>,
    pub trust_history: Mutex<Vec<bool>>,
    pub unloaded: Mutex<Vec<PathBuf>>,
    pub home_reply: Mutex<Option<HomePage>>,
    pub search_reply: Mutex<SearchPage>,
    pub details_reply: Mutex<Option<LoadDetails>>,
    pub links_reply: Mutex<PlayableLinks>,
}

impl ProviderGateway for BddGateway {
    fn load_extension(&self, _: &Path, trusted: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
        self.trust_history.lock().expect("trust").push(trusted);
        Ok(self.providers_on_load.lock().expect("providers").clone())
    }
    fn unload_extension(&self, path: &Path) -> Result<(), GatewayError> {
        self.unloaded
            .lock()
            .expect("unloaded")
            .push(path.to_path_buf());
        Ok(())
    }
    fn search(&self, _: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
        Ok(self.search_reply.lock().expect("search").clone())
    }
    fn main_page(
        &self,
        _: &str,
        _: u32,
        _: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        Ok(self.home_reply.lock().expect("home").clone())
    }
    fn load(&self, _: &str, _: &str) -> Result<Option<LoadDetails>, GatewayError> {
        Ok(self.details_reply.lock().expect("details").clone())
    }
    fn load_links(&self, _: &str, _: &str) -> Result<PlayableLinks, GatewayError> {
        Ok(self.links_reply.lock().expect("links").clone())
    }
    fn start_link_search(&self, _: &str, _: &str) -> Result<String, GatewayError> {
        Ok("bdd-search-1".into())
    }
    fn poll_link_search(
        &self,
        _: &str,
        cursor: LinkSearchCursor,
        _: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let batch = self.links_reply.lock().expect("links").clone();
        Ok(LinkSearchUpdate {
            links: batch.links.get(cursor.links..).unwrap_or(&[]).to_vec(),
            subtitles: batch
                .subtitles
                .get(cursor.subtitles..)
                .unwrap_or(&[])
                .to_vec(),
            done: true,
            success: true,
            failure: None,
            next_link: batch.links.len(),
            next_subtitle: batch.subtitles.len(),
        })
    }
    fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }
}

#[derive(Default)]
pub struct BddPlayer {
    pub played: Mutex<Vec<PlaybackRequest>>,
    pub status: Mutex<PlaybackStatus>,
}

impl VideoPlayer for BddPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.played.lock().expect("played").push(request.clone());
        let mut st = self.status.lock().expect("status");
        st.state = PlaybackState::Playing;
        st.position_ms = request.start_position_ms;
        Ok(())
    }
    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.status.lock().expect("status").state = if paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        };
        Ok(())
    }
    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.status.lock().expect("status").position_ms = position_ms;
        Ok(())
    }
    fn stop(&self) -> Result<(), PlayerError> {
        self.status.lock().expect("status").state = PlaybackState::Idle;
        Ok(())
    }
    fn status(&self) -> PlaybackStatus {
        self.status.lock().expect("status").clone()
    }
}

pub struct LocalHttpServer {
    pub base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LocalHttpServer {
    pub fn start(routes: Vec<(String, u16, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Windows: accepted sockets inherit the listener's non-blocking mode.
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 2048];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let path = req
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let (status, body) = routes
                            .iter()
                            .find(|(p, _, _)| p == path)
                            .map(|(_, st, b)| (*st, b.as_slice()))
                            .unwrap_or((404, b"not found"));
                        let reason = if status == 200 { "OK" } else { "Not Found" };
                        let header = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url: format!("http://{addr}"),
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for LocalHttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergedPrompt {
    Skip { label: &'static str, to_ms: i64 },
    NextEpisode,
}

#[derive(World, Default)]
pub struct AppWorld {
    pub data_dir: Option<PathBuf>,
    pub db: Option<Arc<Database>>,
    pub gateway: Arc<BddGateway>,
    pub player: Arc<BddPlayer>,
    pub http_files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub repo_urls: HashMap<String, String>,
    pub stream_server: Option<LocalHttpServer>,
    pub loaded_details: Option<LoadDetails>,
    pub chosen_link: Option<PlayableLink>,
    pub sub_files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub sub_tracks: Vec<SubtitleTrack>,
    pub delivered_subs: Vec<ReadySubtitle>,
    pub failed_sub_count: usize,
    pub episode_duration_ms: i64,
    pub chapters: Vec<Chapter>,
    pub online_skip_json: String,
    pub merged_skips: Vec<SkipSegment>,
}

impl std::fmt::Debug for AppWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppWorld")
            .field("data_dir", &self.data_dir)
            .finish()
    }
}

impl Drop for AppWorld {
    fn drop(&mut self) {
        self.stream_server.take();
        self.db.take();
        if let Some(dir) = self.data_dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl AppWorld {
    pub fn init_temp_env(&mut self) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cs_bdd_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db = Arc::new(Database::open(&dir.join("cloudstream.db")).expect("open db"));
        self.data_dir = Some(dir);
        self.db = Some(db);
    }

    pub fn prompt_at(&self, pos_ms: i64, has_next: bool) -> Option<MergedPrompt> {
        for seg in &self.merged_skips {
            let start = seg.start_ms as i64;
            let end = seg.end_ms as i64;
            if pos_ms >= start && pos_ms < end {
                let ends_episode = matches!(seg.kind, SkipKind::Credits | SkipKind::Preview)
                    && self.episode_duration_ms > 0
                    && self.episode_duration_ms - end < 20_000;
                if has_next && ends_episode {
                    return Some(MergedPrompt::NextEpisode);
                }
                let label = match seg.kind {
                    SkipKind::Intro => "Skip Intro",
                    SkipKind::Recap => "Skip Recap",
                    SkipKind::Credits => "Skip Credits",
                    SkipKind::Preview => "Skip Preview",
                };
                return Some(MergedPrompt::Skip { label, to_ms: end });
            }
        }
        None
    }
}
