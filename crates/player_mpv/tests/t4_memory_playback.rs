//! T4: how much memory the real libmpv, set up as in the app (pictures rendered into memory and
//! copied the way the UI copies them), holds while a video plays. Prints the resident and
//! proportional memory of this process every few seconds and the peak at the end. Linux only
//! (reads /proc); `docs/MEMORY.md` scenario S3 for the player alone.
//!
//! Needs a video file (a long 1080p one shows the cache filling). Run with
//! `CS_LIBMPV_PATH=... CS_BENCH_VIDEO=video.mp4 [CS_BENCH_SECONDS=30] [CS_BENCH_CACHE_MIB=128]
//! cargo test -p player_mpv --release --test t4_memory_playback -- --ignored --nocapture`.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout
)] // test code: failing fast is the point
#![cfg(target_os = "linux")]

use std::time::{Duration, Instant};

use player_api::{CacheLimits, PlaybackRequest, PlaybackState, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

const MIB: u64 = 1024 * 1024;
/// How often the UI looks for a new picture (the app's frame timer).
const FRAME_TICK: Duration = Duration::from_millis(16);
/// How often memory is printed.
const REPORT_EVERY: Duration = Duration::from_secs(5);

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(default)
}

/// `(resident, proportional)` memory of this process in MiB.
fn memory_mib() -> (u64, u64) {
    let field = |text: &str, name: &str| -> u64 {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|kib| kib.parse::<u64>().ok())
            .unwrap_or(0)
            / 1024
    };
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    (field(&status, "VmRSS:"), field(&rollup, "Pss:"))
}

/// CPU seconds (user + system) this process has used.
fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // Fields after the parenthesized name: utime is the 12th, stime the 13th (clock ticks of 10 ms).
    let after_name = stat.rsplit_once(')').map_or("", |(_, rest)| rest);
    let ticks = |index: usize| -> f64 {
        after_name
            .split_whitespace()
            .nth(index)
            .and_then(|field| field.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

/// A player set up as in the app, with the video cache of `CS_BENCH_CACHE_MIB` (default 128).
fn embedded_player() -> MpvPlayer {
    let library = locate_libmpv().expect("libmpv located");
    let options = PlayerOptions {
        embedded: true,
        ..PlayerOptions::default()
    };
    let player = MpvPlayer::new(&library, &options).expect("player starts");
    player.set_surface_size(1280, 720);
    let cache_mib = env_number("CS_BENCH_CACHE_MIB", 128);
    let ahead = cache_mib * MIB / 2;
    player
        .set_cache_limits(CacheLimits {
            ahead_bytes: ahead,
            behind_bytes: cache_mib * MIB - ahead,
        })
        .expect("cache limits");
    player
}

/// Plays `CS_BENCH_VIDEO`, silent.
fn start(player: &MpvPlayer) {
    let url = std::env::var("CS_BENCH_VIDEO").expect("CS_BENCH_VIDEO names a video file");
    player.set_volume(0).expect("volume");
    player
        .play(&PlaybackRequest {
            url,
            title: "memory".to_owned(),
            ..PlaybackRequest::default()
        })
        .expect("loadfile accepted");
}

/// Looks for pictures as the UI does for `seconds`, printing memory as it goes; returns the
/// number of pictures and the peak proportional memory.
fn watch(player: &MpvPlayer, seconds: u64) -> (u32, u64) {
    let (start, mut last_report) = (Instant::now(), Instant::now());
    let (mut last_serial, mut frames, mut peak_pss) = (0_u64, 0_u32, 0_u64);
    while start.elapsed() < Duration::from_secs(seconds) {
        if let Some(frame) = player.latest_frame(last_serial) {
            last_serial = frame.serial;
            frames += 1;
            // What the UI does with a new picture: a fresh copy of its pixels.
            std::hint::black_box(frame.rgba.clone());
        }
        if last_report.elapsed() >= REPORT_EVERY {
            last_report = Instant::now();
            let (rss, pss) = memory_mib();
            peak_pss = peak_pss.max(pss);
            let state = player.status().state;
            println!(
                "MEM t={:>3}s state={state:?} frames={frames} rss {rss} MiB, pss {pss} MiB",
                start.elapsed().as_secs()
            );
        }
        std::thread::sleep(FRAME_TICK);
    }
    (frames, peak_pss)
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH) and a video (set CS_BENCH_VIDEO)"]
fn playing_a_video_reports_the_players_memory() {
    let player = embedded_player();
    let (idle_rss, idle_pss) = memory_mib();
    println!("MEM before play: rss {idle_rss} MiB, pss {idle_pss} MiB");
    start(&player);
    let seconds = env_number("CS_BENCH_SECONDS", 30);
    let cpu_before = cpu_seconds();
    let (frames, peak) = watch(&player, seconds);
    let (rss, pss) = memory_mib();
    let cpu = cpu_seconds() - cpu_before;
    println!(
        "MEM end: rss {rss} MiB, pss {pss} MiB, peak pss {} MiB, frames {frames}, cpu {cpu:.1} s in {seconds} s",
        peak.max(pss)
    );
    assert!(frames > 0, "no picture was ever delivered");
    assert!(matches!(
        player.status().state,
        PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
    ));
}
