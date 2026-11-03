//! T4: the real libmpv turns on the subtitle in the preferred language by itself, once per stream,
//! and leaves the viewer's own choice alone.
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv --test t4_subtitle_selection -- --ignored`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::time::{Duration, Instant};

use player_api::{
    find_language, MediaTrack, PlaybackRequest, SubtitleSource, TrackKind, VideoPlayer, ENGLISH,
};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

fn headless_player() -> MpvPlayer {
    let library = locate_libmpv().expect("libmpv located");
    MpvPlayer::new(
        &library,
        &PlayerOptions {
            headless: true,
            ..PlayerOptions::default()
        },
    )
    .expect("player starts")
}

fn subtitle_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("cs_selection_{}_{name}.srt", std::process::id()));
    std::fs::write(&path, "1\n00:00:00,000 --> 00:00:09,000\nHello\n\n").expect("srt written");
    path
}

fn request(files: &[(&str, &std::path::Path)]) -> PlaybackRequest {
    PlaybackRequest {
        url: "av://lavfi:testsrc=duration=20:size=160x90:rate=10".to_owned(),
        subtitles: files
            .iter()
            .map(|(language, path)| SubtitleSource {
                url: path.to_string_lossy().into_owned(),
                language: (*language).to_owned(),
            })
            .collect(),
        ..PlaybackRequest::default()
    }
}

fn subtitles(player: &MpvPlayer) -> Vec<MediaTrack> {
    player
        .tracks()
        .into_iter()
        .filter(|t| t.kind == TrackKind::Subtitle)
        .collect()
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn showing(player: &MpvPlayer, wanted_title: &str) -> bool {
    subtitles(player)
        .iter()
        .any(|t| t.selected && t.title.contains(wanted_title))
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn the_first_preferred_language_that_the_stream_has_is_turned_on_by_itself() {
    let (english, arabic) = (subtitle_file("en"), subtitle_file("ar"));
    let player = headless_player();
    player.set_preferred_subtitles(&[find_language("ara").expect("Arabic"), ENGLISH]);

    player
        .play(&request(&[("English", &english), ("Arabic", &arabic)]))
        .expect("plays");

    wait_until("Arabic to show", || showing(&player, "Arabic"));
    assert!(
        !showing(&player, "English"),
        "only the preferred language shows"
    );
    let _ = (std::fs::remove_file(english), std::fs::remove_file(arabic));
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn a_subtitle_the_viewer_turned_off_stays_off() {
    let english = subtitle_file("off");
    let player = headless_player();
    player.set_preferred_subtitles(&[ENGLISH]);
    player
        .play(&request(&[("English", &english)]))
        .expect("plays");
    wait_until("English to show", || showing(&player, "English"));

    player
        .select_track(TrackKind::Subtitle, None)
        .expect("subtitles off");
    std::thread::sleep(Duration::from_millis(2_500));

    assert!(
        subtitles(&player).iter().all(|t| !t.selected),
        "the automatic choice overrode the viewer"
    );
    let _ = std::fs::remove_file(english);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn with_the_automatic_choice_off_nothing_is_turned_on() {
    let english = subtitle_file("auto_off");
    let player = headless_player();
    player.set_preferred_subtitles(&[ENGLISH]);
    player.set_preferred_subtitles(&[]);
    player
        .play(&request(&[("English", &english)]))
        .expect("plays");
    wait_until("the subtitle track to be listed", || {
        !subtitles(&player).is_empty()
    });
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(subtitles(&player).iter().all(|t| !t.selected));
    let _ = std::fs::remove_file(english);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn a_new_stream_gets_its_own_automatic_choice() {
    let english = subtitle_file("again");
    let player = headless_player();
    player.set_preferred_subtitles(&[ENGLISH]);
    player
        .play(&request(&[("English", &english)]))
        .expect("plays");
    wait_until("English to show", || showing(&player, "English"));
    player
        .select_track(TrackKind::Subtitle, None)
        .expect("subtitles off");

    // Another source of the same video: the choice is made again for it.
    player
        .play(&request(&[("English", &english)]))
        .expect("plays again");

    wait_until("English to show on the new stream", || {
        showing(&player, "English")
    });
    let _ = std::fs::remove_file(english);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn a_preferred_subtitle_that_arrives_after_the_viewer_chose_is_not_forced_on() {
    let (english, arabic) = (subtitle_file("late_en"), subtitle_file("late_ar"));
    let player = headless_player();
    player.set_preferred_subtitles(&[find_language("ara").expect("Arabic")]);
    player
        .play(&request(&[("English", &english)]))
        .expect("plays");
    wait_until("the English track to be listed", || {
        !subtitles(&player).is_empty()
    });
    player
        .select_track(TrackKind::Subtitle, None)
        .expect("subtitles off");

    // The provider's slow service delivers the Arabic file only now.
    player.add_subtitles(&[SubtitleSource {
        url: arabic.to_string_lossy().into_owned(),
        language: "Arabic".to_owned(),
    }]);
    wait_until("the Arabic track to be listed", || {
        subtitles(&player).len() == 2
    });
    std::thread::sleep(Duration::from_millis(2_500));

    assert!(
        subtitles(&player).iter().all(|t| !t.selected),
        "a subtitle was forced on after the viewer chose"
    );
    let _ = (std::fs::remove_file(english), std::fs::remove_file(arabic));
}
