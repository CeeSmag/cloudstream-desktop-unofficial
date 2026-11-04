//! T4: mpv's own detailed log reaches the app's log only while verbose logging is switched on.
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv --test t4_log_level -- --ignored`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use player_api::{PlaybackRequest, PlaybackState, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// A 2-second synthetic video produced by ffmpeg's lavfi inside mpv; needs no file or network.
const TEST_STREAM: &str = "av://lavfi:testsrc=duration=2:size=64x64:rate=10";

/// Counts the events that carry mpv's own messages.
struct CountMpvMessages(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> Layer<S> for CountMpvMessages {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target().contains("mpv_log") {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn play_to_the_end(player: &MpvPlayer) {
    let request = PlaybackRequest {
        url: TEST_STREAM.to_owned(),
        title: "test".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if player.status().state == PlaybackState::Ended {
            // The event thread may still be writing the last messages.
            std::thread::sleep(Duration::from_millis(300));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the stream never ended");
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn verbose_logging_adds_mpvs_own_messages_and_switching_it_off_stops_them() {
    let counted = Arc::new(AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(CountMpvMessages(Arc::clone(&counted)));
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

    play_to_the_end(&player);
    let quiet = counted.load(Ordering::SeqCst);

    player.set_verbose_log(true);
    let before = counted.load(Ordering::SeqCst);
    play_to_the_end(&player);
    let verbose = counted.load(Ordering::SeqCst) - before;

    player.set_verbose_log(false);
    let before = counted.load(Ordering::SeqCst);
    play_to_the_end(&player);
    let quiet_again = counted.load(Ordering::SeqCst) - before;

    assert!(
        verbose >= 20,
        "verbose logging forwarded only {verbose} messages"
    );
    assert!(
        verbose > quiet * 3 + 10,
        "verbose {verbose} is not clearly more than quiet {quiet}"
    );
    assert!(
        quiet_again * 3 < verbose,
        "switching it off left {quiet_again} messages (verbose gave {verbose})"
    );
}
