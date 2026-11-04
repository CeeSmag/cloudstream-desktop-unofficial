//! Reads the chapters of a real file through the real libmpv. mpv turns the tracks of a CUE sheet
//! into chapters, so a generated WAV plus a CUE sheet is a file with known chapters.
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv --lib -- --ignored chapters`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::libmpv_locator::locate_libmpv;
use crate::mpv_api::{MpvApi, MpvHandle};
use crate::mpv_event::MpvEvent;
use crate::mpv_player::PlayerOptions;
use crate::player_configuration::configure;

const SAMPLE_RATE: u32 = 8000;
const SECONDS: u32 = 2;

/// Two seconds of silence as a mono 16-bit WAV.
fn silent_wav() -> Vec<u8> {
    let data_len = SAMPLE_RATE * SECONDS * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(bytes.len() + data_len as usize, 0);
    bytes
}

/// Three tracks: at 0 s, at 37/75 s (493 ms) and at 60/75 s (800 ms, no title).
const CUE: &str = "FILE \"chapters.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Opening\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Part A\"\n    INDEX 01 00:00:37\n  TRACK 03 AUDIO\n    INDEX 01 00:00:60\n";

fn wait_for_file_loaded(handle: &MpvHandle) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if handle.wait_event(Duration::from_millis(100)) == MpvEvent::FileLoaded {
            return true;
        }
    }
    false
}

fn cue_url(folder: &Path) -> String {
    folder
        .join("chapters.cue")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn chapters_of_a_real_file_are_read_through_libmpv() {
    let folder = std::env::temp_dir().join(format!("cs_chapters_{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("chapters.wav"), silent_wav()).expect("wav written");
    std::fs::write(folder.join("chapters.cue"), CUE).expect("cue written");

    let library = locate_libmpv().expect("libmpv located");
    let handle = MpvHandle::create(Arc::new(MpvApi::load(&library).expect("libmpv loads")))
        .expect("handle created");
    let options = PlayerOptions {
        headless: true,
        ..PlayerOptions::default()
    };
    configure(&handle, &options, false).expect("configured");
    handle.initialize().expect("initialized");
    handle
        .command(&["loadfile", &cue_url(&folder)])
        .expect("loadfile accepted");
    let loaded = wait_for_file_loaded(&handle);

    let chapters = read_chapters(|name| handle.get_string(name));
    std::fs::remove_dir_all(&folder).ok();
    assert!(loaded, "the file never loaded");
    let seen: Vec<_> = chapters
        .iter()
        .map(|c| (c.title.as_str(), c.time_ms))
        .collect();
    assert_eq!(
        seen,
        [("Opening", 0), ("Part A", 493), ("Chapter 3", 800)],
        "the chapters libmpv reports"
    );
}
