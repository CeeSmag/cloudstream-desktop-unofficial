//! T4: about 30 s into a stream the player writes one "playback diagnostics" line, and none before.
//! Takes about 35 s. Run with
//! `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv --test t4_playback_diagnostics -- --ignored`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use player_api::{PlaybackRequest, PlaybackState, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// A 40-second synthetic video produced by ffmpeg's lavfi inside mpv; needs no file or network.
const TEST_STREAM: &str = "av://lavfi:testsrc=duration=40:size=64x64:rate=10";

/// The line the player writes once per stream.
const DIAGNOSTICS_LINE: &str = "playback diagnostics";

/// Keeps the message of every event written by the diagnostics module.
struct RecordDiagnostics(Arc<Mutex<Vec<String>>>);

struct MessageOf(String);

/// Collects the message and every field of an event as `name=value` text.
impl Visit for MessageOf {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!("{}={value:?} ", field.name()));
    }
}

impl<S: tracing::Subscriber> Layer<S> for RecordDiagnostics {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if !event.metadata().target().contains("playback_diagnostics") {
            return;
        }
        let mut message = MessageOf(String::new());
        event.record(&mut message);
        if let Ok(mut lines) = self.0.lock() {
            lines.push(message.0);
        }
    }
}

fn count_of(lines: &Mutex<Vec<String>>) -> usize {
    let lines = lines.lock().expect("lock");
    lines
        .iter()
        .filter(|line| line.contains(DIAGNOSTICS_LINE))
        .count()
}

fn sleep_until(deadline: Instant) {
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH); takes about 35 s"]
fn diagnostics_are_written_once_about_thirty_seconds_into_the_stream() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(RecordDiagnostics(Arc::clone(&lines)));
    tracing::subscriber::set_global_default(subscriber).expect("no other subscriber");

    let library = locate_libmpv().expect("libmpv located");
    let player = MpvPlayer::new(
        &library,
        &PlayerOptions {
            headless: true,
            ..PlayerOptions::default()
        },
    )
    .expect("player starts");
    let request = PlaybackRequest {
        url: TEST_STREAM.to_owned(),
        title: "test".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    let started = Instant::now();

    sleep_until(started + Duration::from_secs(20));
    assert_eq!(player.status().state, PlaybackState::Playing);
    assert_eq!(count_of(&lines), 0, "nothing is written before the delay");

    sleep_until(started + Duration::from_secs(35));
    assert_eq!(player.status().state, PlaybackState::Playing);
    assert_eq!(count_of(&lines), 1, "exactly one line after the delay");
    let line = lines.lock().expect("lock").join("\n");
    // The properties the line reads exist in libmpv: the picture size and the frame rate are real.
    assert!(line.contains("size=64x64"), "size missing in: {line}");
    assert!(
        line.contains("container_fps=10.00"),
        "frame rate missing in: {line}"
    );
}
