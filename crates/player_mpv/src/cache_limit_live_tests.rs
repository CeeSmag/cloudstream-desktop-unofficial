//! What the real libmpv does with the cache limits while a file is loaded: it reads ahead up to a
//! raised limit once playback goes on (not while paused), trims what lies behind to a lowered one
//! at once, and keeps what it already read ahead when that limit is lowered. Needs a large local video (`CS_CACHE_VIDEO`, 500 MB or more).
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll CS_CACHE_VIDEO=...
//! cargo test -p player_mpv --lib -- --ignored cache_limits --nocapture`.
#![allow(clippy::print_stdout)] // the numbers are worth seeing

use std::sync::Arc;
use std::time::{Duration, Instant};

use player_api::{CacheLimits, PlaybackRequest, StreamKind};

use super::{apply_cache_limits, apply_read_ahead};
use crate::libmpv_locator::locate_libmpv;
use crate::mpv_api::{MpvApi, MpvHandle};
use crate::mpv_event::MpvEvent;
use crate::mpv_player::PlayerOptions;
use crate::player_configuration::configure;

const MIB: u64 = 1024 * 1024;

fn limits(ahead_mib: u64, behind_mib: u64) -> CacheLimits {
    CacheLimits {
        ahead_bytes: ahead_mib * MIB,
        behind_bytes: behind_mib * MIB,
    }
}

/// A headless player with the video loaded and paused, the cache set to `start`.
fn loaded_paused(start: CacheLimits) -> MpvHandle {
    let video = std::env::var("CS_CACHE_VIDEO").expect("CS_CACHE_VIDEO");
    let library = locate_libmpv().expect("libmpv located");
    let handle = MpvHandle::create(Arc::new(MpvApi::load(&library).expect("libmpv loads")))
        .expect("handle created");
    let options = PlayerOptions {
        headless: true,
        ..PlayerOptions::default()
    };
    configure(&handle, &options, false).expect("configured");
    // A local file has no cache unless asked for; the app's streams are on the network.
    handle.set_option("cache", "yes").expect("cache on");
    handle.initialize().expect("initialized");
    let request = PlaybackRequest {
        url: video.replace('\\', "/"),
        kind: StreamKind::Video,
        ..PlaybackRequest::default()
    };
    apply_read_ahead(&handle, &request).expect("read-ahead time");
    apply_cache_limits(&handle, start).expect("limits");
    handle.set_property("pause", "yes").expect("paused");
    handle
        .command(&["loadfile", &request.url])
        .expect("loadfile accepted");
    wait_for_file_loaded(&handle);
    handle
}

fn wait_for_file_loaded(handle: &MpvHandle) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if handle.wait_event(Duration::from_millis(100)) == MpvEvent::FileLoaded {
            return;
        }
    }
    panic!("the file never loaded");
}

/// A number field of mpv's `demuxer-cache-state`, which it hands out as JSON text.
fn state_number(state: &str, field: &str) -> f64 {
    let key = format!("\"{field}\":");
    state
        .split(&key)
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .and_then(|number| number.trim().parse().ok())
        .unwrap_or(-1.0)
}

/// Lets mpv work for `time` and returns (ahead, behind) in MiB.
fn settle(handle: &MpvHandle, what: &str, time: Duration) -> (f64, f64) {
    let until = Instant::now() + time;
    while Instant::now() < until {
        handle.wait_event(Duration::from_millis(50));
    }
    let state = handle.get_string("demuxer-cache-state").unwrap_or_default();
    let ahead = state_number(&state, "fw-bytes") / MIB as f64;
    let behind = state_number(&state, "total-bytes") / MIB as f64 - ahead;
    println!("{what}: ahead {ahead:.0} MiB, behind {behind:.0} MiB");
    (ahead, behind)
}

#[test]
#[ignore = "needs libmpv and a large video (CS_LIBMPV_PATH, CS_CACHE_VIDEO)"]
fn cache_limits_a_raised_limit_reads_on_once_playing_and_a_lowered_one_keeps_what_was_read_ahead() {
    let handle = loaded_paused(limits(100, 100));
    let (first, _) = settle(&handle, "at 100 ahead", Duration::from_secs(5));
    assert!((90.0..=110.0).contains(&first), "filled to {first:.0}");

    apply_cache_limits(&handle, limits(400, 100)).expect("raised");
    let (still, _) = settle(
        &handle,
        "raised to 400, still paused",
        Duration::from_secs(3),
    );
    assert!(still < 150.0, "read on while paused ({still:.0})");
    handle.set_property("pause", "no").expect("plays");
    let (raised, _) = settle(&handle, "raised to 400 and playing", Duration::from_secs(6));
    handle.set_property("pause", "yes").expect("paused again");
    assert!(raised > 350.0, "read on to {raised:.0}");

    apply_cache_limits(&handle, limits(100, 100)).expect("lowered");
    let (lowered, _) = settle(&handle, "lowered to 100 ahead", Duration::from_secs(5));
    assert!(
        lowered > 350.0,
        "libmpv dropped what it read ahead ({lowered:.0})"
    );
}

#[test]
#[ignore = "needs libmpv and a large video (CS_LIBMPV_PATH, CS_CACHE_VIDEO)"]
fn cache_limits_the_part_behind_is_trimmed_to_a_lowered_limit() {
    let handle = loaded_paused(limits(400, 400));
    settle(&handle, "filled", Duration::from_secs(6));
    // Jump ahead inside the cache, so what was before the new position is behind.
    let cached = handle.get_double("demuxer-cache-duration").unwrap_or(0.0);
    handle
        .command(&["seek", &format!("{:.0}", cached * 0.6), "relative"])
        .expect("seek");
    let (_, behind) = settle(
        &handle,
        "after a jump inside the cache",
        Duration::from_secs(6),
    );
    assert!(behind > 60.0, "nothing came to lie behind ({behind:.0})");

    apply_cache_limits(&handle, limits(400, 20)).expect("lowered");
    let (_, trimmed) = settle(&handle, "behind lowered to 20", Duration::from_secs(3));
    assert!(trimmed <= 25.0, "the part behind is still {trimmed:.0}");
}
