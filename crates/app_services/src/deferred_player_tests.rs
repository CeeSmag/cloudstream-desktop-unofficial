use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use player_api::{Chapter, MediaTrack, PlaybackState, SubtitleSource, TrackKind, ENGLISH};

use super::*;

/// What a player was asked, in order.
type Log = Arc<Mutex<Vec<String>>>;

fn note(log: &Log, entry: String) {
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry);
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// A player that answers with something other than the defaults, and writes down every call.
struct RecordingPlayer {
    log: Log,
}

impl VideoPlayer for RecordingPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        note(&self.log, format!("play {}", request.url));
        Ok(())
    }
    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        note(&self.log, format!("paused {paused}"));
        Ok(())
    }
    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        note(&self.log, format!("seek {position_ms}"));
        Ok(())
    }
    fn stop(&self) -> Result<(), PlayerError> {
        note(&self.log, "stop".to_owned());
        Ok(())
    }
    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        note(&self.log, format!("volume {percent}"));
        Ok(())
    }
    fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), PlayerError> {
        note(
            &self.log,
            format!("cache {}/{}", limits.ahead_bytes, limits.behind_bytes),
        );
        Ok(())
    }
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus {
            state: PlaybackState::Playing,
            position_ms: 42,
            ..PlaybackStatus::default()
        }
    }
    fn tracks(&self) -> Vec<MediaTrack> {
        vec![MediaTrack {
            id: 7,
            kind: TrackKind::Audio,
            language: "eng".to_owned(),
            title: String::new(),
            selected: true,
            external: false,
        }]
    }
    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        note(&self.log, format!("select {kind:?} {id:?}"));
        Ok(())
    }
    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        note(&self.log, format!("subtitles {}", subtitles.len()));
    }
    fn pending_subtitles(&self) -> usize {
        3
    }
    fn set_preferred_audio(&self, language: Option<Language>) {
        note(&self.log, format!("audio {}", language.is_some()));
    }
    fn set_verbose_log(&self, enabled: bool) {
        note(&self.log, format!("verbose {enabled}"));
    }
    fn set_preferred_subtitles(&self, languages: &[Language]) {
        note(
            &self.log,
            format!("preferred subtitles {}", languages.len()),
        );
    }
    fn chapters(&self) -> Vec<Chapter> {
        vec![Chapter {
            title: "Intro".to_owned(),
            time_ms: 0,
        }]
    }
    fn is_embedded(&self) -> bool {
        true
    }
    fn set_surface_size(&self, width: u32, height: u32) {
        note(&self.log, format!("surface {width}x{height}"));
    }
    fn latest_frame(&self, _: u64) -> Option<Arc<VideoFrame>> {
        Some(Arc::new(VideoFrame::default()))
    }
}

struct Rig {
    player: Arc<DeferredPlayer>,
    starts: Arc<AtomicUsize>,
    log: Log,
}

/// A deferred player whose factory fails its first `failures` times and counts every try.
fn rig(failures: usize) -> Rig {
    rig_taking(failures, Duration::ZERO)
}

/// Like [`rig`], with a factory that takes `start_time`: callers that arrive meanwhile overlap it.
fn rig_taking(failures: usize, start_time: Duration) -> Rig {
    let log: Log = Arc::default();
    let starts = Arc::new(AtomicUsize::new(0));
    let (factory_log, factory_starts) = (Arc::clone(&log), Arc::clone(&starts));
    let player = Arc::new(DeferredPlayer::new(
        true,
        Box::new(move || {
            let attempt = factory_starts.fetch_add(1, Ordering::SeqCst);
            thread::sleep(start_time);
            if attempt < failures {
                return Err(PlayerError::Unavailable(
                    "libmpv would not start".to_owned(),
                ));
            }
            let player: Arc<dyn VideoPlayer> = Arc::new(RecordingPlayer {
                log: Arc::clone(&factory_log),
            });
            Ok(player)
        }),
    ));
    Rig {
        player,
        starts,
        log,
    }
}

fn request(url: &str) -> PlaybackRequest {
    PlaybackRequest {
        url: url.to_owned(),
        ..PlaybackRequest::default()
    }
}

#[test]
fn nothing_is_started_until_the_first_play_and_questions_get_idle_answers() {
    let rig = rig(0);
    assert_eq!(rig.player.status(), PlaybackStatus::default());
    assert!(rig.player.tracks().is_empty());
    assert!(rig.player.chapters().is_empty());
    assert_eq!(rig.player.pending_subtitles(), 0);
    assert!(rig.player.latest_frame(0).is_none());
    assert!(
        rig.player.is_embedded(),
        "what the constructor said, without a player"
    );
    assert_eq!(rig.starts.load(Ordering::SeqCst), 0);
}

#[test]
fn actions_before_the_first_play_have_nothing_to_act_on_and_start_nothing() {
    let rig = rig(0);
    assert!(rig.player.set_paused(true).is_ok());
    assert!(rig.player.seek_to(5).is_ok());
    assert!(rig.player.stop().is_ok());
    assert!(rig.player.select_track(TrackKind::Audio, Some(1)).is_ok());
    rig.player.add_subtitles(&[SubtitleSource::default()]);
    assert_eq!(rig.starts.load(Ordering::SeqCst), 0);
    assert!(entries(&rig.log).is_empty());
}

#[test]
fn settings_made_before_the_first_play_are_applied_in_full_before_it_plays() {
    let rig = rig(0);
    rig.player.set_surface_size(800, 450);
    rig.player.set_volume(30).expect("volume");
    rig.player
        .set_cache_limits(CacheLimits {
            ahead_bytes: 6,
            behind_bytes: 2,
        })
        .expect("cache");
    rig.player.set_preferred_audio(Some(ENGLISH));
    rig.player.set_preferred_subtitles(&[ENGLISH]);
    rig.player.set_verbose_log(true);
    assert!(entries(&rig.log).is_empty(), "nothing reached a player yet");
    rig.player.play(&request("https://x/1")).expect("play");
    let log = entries(&rig.log);
    assert_eq!(log.last().map(String::as_str), Some("play https://x/1"));
    for expected in [
        "surface 800x450",
        "volume 30",
        "cache 6/2",
        "audio true",
        "preferred subtitles 1",
        "verbose true",
    ] {
        assert!(
            log.iter().any(|entry| entry == expected),
            "{expected} missing from {log:?}"
        );
    }
    assert_eq!(rig.starts.load(Ordering::SeqCst), 1);
}

#[test]
fn only_the_last_value_of_a_setting_is_applied() {
    let rig = rig(0);
    rig.player.set_volume(10).expect("volume");
    rig.player.set_volume(55).expect("volume");
    rig.player.play(&request("u")).expect("play");
    let volumes: Vec<String> = entries(&rig.log)
        .into_iter()
        .filter(|entry| entry.starts_with("volume"))
        .collect();
    assert_eq!(volumes, ["volume 55"]);
}

#[test]
fn once_started_every_call_goes_straight_through() {
    let rig = rig(0);
    rig.player.play(&request("u")).expect("play");
    rig.player.set_volume(80).expect("volume");
    rig.player.set_paused(true).expect("pause");
    rig.player.seek_to(1_000).expect("seek");
    assert_eq!(rig.player.status().position_ms, 42);
    assert_eq!(rig.player.tracks().len(), 1);
    assert_eq!(rig.player.pending_subtitles(), 3);
    assert!(rig.player.latest_frame(0).is_some());
    let log = entries(&rig.log);
    assert!(log.contains(&"volume 80".to_owned()));
    assert!(log.contains(&"paused true".to_owned()));
    assert!(log.contains(&"seek 1000".to_owned()));
}

#[test]
fn a_failed_start_is_reported_and_the_next_play_tries_again_with_the_settings_kept() {
    let rig = rig(1);
    rig.player.set_volume(25).expect("volume");
    let error = rig
        .player
        .play(&request("u"))
        .expect_err("the first start fails");
    assert!(
        error.to_string().contains("libmpv would not start"),
        "{error}"
    );
    assert!(entries(&rig.log).is_empty());
    rig.player
        .play(&request("u"))
        .expect("the second start works");
    assert_eq!(rig.starts.load(Ordering::SeqCst), 2);
    assert!(
        entries(&rig.log).contains(&"volume 25".to_owned()),
        "the setting survived the failure"
    );
}

/// Eight threads press play at once; the player must start exactly once and every play must arrive.
fn first_plays_together(threads: usize) -> Rig {
    let rig = rig_taking(0, Duration::from_millis(2));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|index| {
            let (player, barrier) = (Arc::clone(&rig.player), Arc::clone(&barrier));
            thread::spawn(move || {
                barrier.wait();
                player.play(&request(&format!("u{index}")))
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread").expect("play");
    }
    rig
}

#[test]
fn stress_concurrent_first_plays_start_the_player_exactly_once() {
    let deadline = Instant::now() + Duration::from_secs(20);
    for _ in 0..60 {
        assert!(Instant::now() < deadline, "too slow: a deadlock?");
        let rig = first_plays_together(8);
        assert_eq!(rig.starts.load(Ordering::SeqCst), 1);
        let plays = entries(&rig.log)
            .iter()
            .filter(|entry| entry.starts_with("play "))
            .count();
        assert_eq!(plays, 8, "every play reached the player");
    }
}

#[test]
fn stress_a_setting_changed_while_the_player_starts_is_never_lost() {
    for _ in 0..200 {
        let rig = rig(0);
        let setter = {
            let player = Arc::clone(&rig.player);
            thread::spawn(move || {
                for volume in 1..=60_u8 {
                    player.set_volume(volume).expect("volume");
                }
            })
        };
        let starter = {
            let player = Arc::clone(&rig.player);
            thread::spawn(move || player.play(&request("u")))
        };
        setter.join().expect("setter");
        starter.join().expect("starter").expect("play");
        let last = entries(&rig.log)
            .into_iter()
            .rev()
            .find(|entry| entry.starts_with("volume"))
            .expect("the volume reached the player");
        assert_eq!(
            last, "volume 60",
            "the final setting is the one that stands"
        );
    }
}
