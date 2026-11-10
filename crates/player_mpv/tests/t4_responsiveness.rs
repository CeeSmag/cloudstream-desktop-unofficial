//! T4: how fast the real libmpv, set up as in the app (pictures rendered into memory), reacts to
//! pause, resume and jumps. Prints the timings; fails only when something never happens.
//! Needs a seekable video file with sound that is at least 3 minutes long.
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll CS_BENCH_VIDEO=...\\video.mkv
//! cargo test -p player_mpv --release --test t4_responsiveness -- --ignored --nocapture`.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout
)] // test code: failing fast is the point

use std::time::{Duration, Instant};

use player_api::{PlaybackRequest, PlaybackState, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

/// How often each action is timed.
const ROUNDS: usize = 12;
/// Nothing in this test may take longer than this.
const LIMIT: Duration = Duration::from_secs(15);

fn embedded_player() -> MpvPlayer {
    let library = locate_libmpv().expect("libmpv located");
    let options = PlayerOptions {
        embedded: true,
        ..PlayerOptions::default()
    };
    let player = MpvPlayer::new(&library, &options).expect("player starts");
    player.set_surface_size(1920, 1080);
    player
}

/// Milliseconds until `done` holds, polled every half millisecond.
fn millis_until(what: &str, done: impl Fn() -> bool) -> f64 {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < LIMIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_micros(500));
    }
    start.elapsed().as_secs_f64() * 1000.0
}

fn serial(player: &MpvPlayer) -> u64 {
    player.latest_frame(0).map_or(0, |frame| frame.serial)
}

fn report(name: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    let worst = samples[samples.len() - 1];
    println!("BENCH {name:<46} median {median:>8.1} ms   worst {worst:>8.1} ms");
}

fn start(player: &MpvPlayer) {
    let url = std::env::var("CS_BENCH_VIDEO").expect("CS_BENCH_VIDEO names a video file");
    let request = PlaybackRequest {
        url,
        title: "bench".to_owned(),
        ..PlaybackRequest::default()
    };
    player.set_volume(0).expect("volume set");
    player.play(&request).expect("loadfile accepted");
    millis_until("playing", || {
        player.status().state == PlaybackState::Playing && player.status().position_ms > 1500
    });
}

/// Pauses or plays; returns the milliseconds until (the command returned, the status shows it).
fn time_pause(player: &MpvPlayer, paused: bool) -> (f64, f64) {
    let wanted = if paused {
        PlaybackState::Paused
    } else {
        PlaybackState::Playing
    };
    let begin = Instant::now();
    player.set_paused(paused).expect("pause set");
    let call = begin.elapsed().as_secs_f64() * 1000.0;
    let status = call + millis_until("the status", || player.status().state == wanted);
    (call, status)
}

/// Pauses, then times from the play command to the first picture that moves again.
fn time_resume_picture(player: &MpvPlayer) -> f64 {
    player.set_paused(true).expect("paused");
    std::thread::sleep(Duration::from_millis(400));
    let before = serial(player);
    let begin = Instant::now();
    player.set_paused(false).expect("playing");
    millis_until("a new picture", || serial(player) != before);
    let picture = begin.elapsed().as_secs_f64() * 1000.0;
    std::thread::sleep(Duration::from_millis(500));
    picture
}

/// Pictures handed to the app per second while the video plays (the file has 24 a second).
fn pictures_per_second(player: &MpvPlayer) -> f64 {
    let (mut seen, mut last) = (0_u32, serial(player));
    let begin = Instant::now();
    while begin.elapsed() < Duration::from_secs(4) {
        let now = serial(player);
        seen += u32::from(now != last);
        last = now;
        std::thread::sleep(Duration::from_millis(2));
    }
    f64::from(seen) / begin.elapsed().as_secs_f64()
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH) and a video (set CS_BENCH_VIDEO)"]
fn pause_and_resume_timings() {
    let player = embedded_player();
    start(&player);
    let (mut pause_call, mut pause_status) = (Vec::new(), Vec::new());
    let (mut resume_call, mut resume_status, mut resume_picture) =
        (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..ROUNDS {
        let (call, status) = time_pause(&player, true);
        pause_call.push(call);
        pause_status.push(status);
        std::thread::sleep(Duration::from_millis(400));
        let (call, status) = time_pause(&player, false);
        resume_call.push(call);
        resume_status.push(status);
        std::thread::sleep(Duration::from_millis(700));
        resume_picture.push(time_resume_picture(&player));
    }
    report("pause: command returns", pause_call);
    report("pause: status says Paused", pause_status);
    report("resume: command returns", resume_call);
    report("resume: status says Playing", resume_status);
    report("resume: first new picture", resume_picture);
    println!(
        "BENCH pictures per second while playing            {:.1}",
        pictures_per_second(&player)
    );
}

/// Jumps while paused (so every new picture is the jump's); returns (call, picture, status).
fn time_seek(player: &MpvPlayer, target_ms: i64) -> (f64, f64, f64) {
    let before = serial(player);
    let begin = Instant::now();
    player.seek_to(target_ms).expect("seek accepted");
    let call = begin.elapsed().as_secs_f64() * 1000.0;
    millis_until("the picture of the new place", || serial(player) != before);
    let picture = begin.elapsed().as_secs_f64() * 1000.0;
    millis_until("the status position", || {
        (player.status().position_ms - target_ms).abs() < 1500
    });
    (call, picture, begin.elapsed().as_secs_f64() * 1000.0)
}

fn seek_series(player: &MpvPlayer, name: &str, step_ms: i64) {
    let (mut calls, mut pictures, mut statuses) = (Vec::new(), Vec::new(), Vec::new());
    let mut target = if step_ms > 0 { 10_000 } else { 170_000 };
    for _ in 0..ROUNDS {
        target += step_ms;
        let (call, picture, status) = time_seek(player, target);
        calls.push(call);
        pictures.push(picture);
        statuses.push(status);
        std::thread::sleep(Duration::from_millis(300));
    }
    report(&format!("{name}: command returns"), calls);
    report(&format!("{name}: picture of the new place"), pictures);
    report(&format!("{name}: status shows the new place"), statuses);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH) and a video (set CS_BENCH_VIDEO)"]
fn jump_timings() {
    let player = embedded_player();
    start(&player);
    time_pause(&player, true);
    std::thread::sleep(Duration::from_millis(500));
    seek_series(&player, "jump +5 s", 5_000);
    seek_series(&player, "jump -5 s", -5_000);
    seek_series(&player, "jump +10 s", 10_000);

    // Five presses of the arrow key sent one after the other, as when the key is tapped quickly.
    let mut bursts = Vec::new();
    for round in 0..6_i64 {
        let base = 20_000 + round * 20_000;
        let begin = Instant::now();
        for press in 1..=5_i64 {
            player.seek_to(base + press * 5_000).expect("seek accepted");
        }
        millis_until("the burst's last place", || {
            (player.status().position_ms - (base + 25_000)).abs() < 1500
        });
        bursts.push(begin.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(250));
    }
    report("5 jumps in a row: status at the last place", bursts);
}
