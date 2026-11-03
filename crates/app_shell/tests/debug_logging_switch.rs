//! The debug logging switch: files are written, sorted by category and level, only while it is on,
//! and while it is off the debug calls are not even evaluated.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::fs;
use std::path::Path;

use app_shell::{start_logging, DebugLogs};
use tracing::Level;

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap_or_default()
}

/// Off: nothing is created and debug calls are rejected before their arguments are built.
fn assert_nothing_happens_while_off(logs: &DebugLogs) {
    assert!(!tracing::enabled!(target: "player_mpv::mpv_player", Level::DEBUG));
    tracing::debug!(target: "player_mpv::mpv_player", "written while off");
    assert!(!logs.directory().exists(), "no debug folder while off");
}

fn write_one_message_of_each_kind() {
    tracing::debug!(target: "player_mpv::mpv_player", "loaded the file");
    tracing::info!(target: "sidecar", "plugin said hi");
    tracing::warn!(target: "extension_runtime::transport", "slow call");
    tracing::error!(target: "webapp", "button failed");
    tracing::debug!(target: "hyper::proto", "third party noise");
}

/// Every category got its own file with the level on each line.
fn assert_messages_are_sorted(dir: &Path) {
    let player = read(dir, "player-logs.txt");
    assert!(player.contains("DEBUG") && player.contains("loaded the file"));
    assert!(read(dir, "sidecar-logs.txt").contains("INFO"));
    assert!(read(dir, "extensions-logs.txt").contains("WARN"));
    assert!(read(dir, "webapp-logs.txt").contains("ERROR"));
}

fn assert_problems_file_holds_only_problems(dir: &Path) {
    let problems = read(dir, "warnings-and-errors.txt");
    assert!(problems.contains("slow call") && problems.contains("button failed"));
    assert!(
        !problems.contains("loaded the file"),
        "debug is not a problem"
    );
    assert!(
        !problems.contains("plugin said hi"),
        "info is not a problem"
    );
}

fn assert_left_out(logs: &DebugLogs, text: &str) {
    for file in logs.files() {
        let content = read(logs.directory(), file.file_name);
        assert!(
            !content.contains(text),
            "{text} found in {}",
            file.file_name
        );
    }
}

#[test]
fn debug_files_fill_only_while_the_switch_is_on() {
    let root = std::env::temp_dir().join(format!("cs_debug_switch_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let logs = start_logging(&root.join("logs").join("cloudstream.log"));
    assert_nothing_happens_while_off(&logs);

    logs.set_enabled(true).expect("switches on");
    assert!(tracing::enabled!(target: "player_mpv::mpv_player", Level::DEBUG));
    write_one_message_of_each_kind();
    assert_messages_are_sorted(logs.directory());
    assert_problems_file_holds_only_problems(logs.directory());
    assert_left_out(&logs, "third party noise");
    assert_left_out(&logs, "written while off");

    logs.set_enabled(false).expect("switches off");
    assert!(!tracing::enabled!(target: "player_mpv::mpv_player", Level::DEBUG));
    tracing::debug!(target: "player_mpv::mpv_player", "written after off");
    tracing::error!(target: "webapp", "error after off");
    assert_left_out(&logs, "written after off");
    assert_left_out(&logs, "error after off");

    let _ = fs::remove_dir_all(&root);
}
