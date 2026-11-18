//! Tests for `storage_cap`: enforce, never-delete saved data, and the busy lock race.

use super::*;
use std::fs;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cs-enforce-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn write(dir: &Path, relative: &str, bytes: usize) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(path, vec![1u8; bytes]).expect("file");
}

#[test]
fn a_folder_within_its_limit_is_left_alone() {
    let dir = scratch("within");
    write(&dir, "runtime-old/x.bin", 1000);
    let report = enforce(&dir, 1, None).expect("ran");
    assert_eq!((report.deleted, report.still_over), (0, false));
    assert!(dir.join("runtime-old/x.bin").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_saved_data_is_never_deleted_even_when_it_alone_is_over() {
    let dir = scratch("never");
    write(&dir, "cloudstream.db", 2_000_000);
    write(&dir, "extensions/P.cs3", 1_000_000);
    write(&dir, "runtime-0.3.6/old.bin", 500_000);
    let before = dir_size(&dir);
    let chosen = plan(collect(&dir, None), before, 1_000);
    let tally = apply(&dir, &chosen);
    assert_eq!(tally, (1, 0));
    assert!(dir.join("cloudstream.db").exists());
    assert!(dir.join("extensions/P.cs3").exists());
    assert!(!dir.join("runtime-0.3.6").exists());
    assert!(chosen.still_over(1_000));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_enforcements_at_once_do_not_fail_or_double_delete() {
    let dir = scratch("race");
    for n in 0..20 {
        write(&dir, &format!("subtitles/{n}.vtt"), 100);
    }
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let _guard = BUSY.lock().unwrap_or_else(|p| p.into_inner());
                let chosen = plan(collect(&dir, None), dir_size(&dir), 0);
                apply(&dir, &chosen)
            })
        })
        .collect();
    let failed: usize = workers
        .into_iter()
        .map(|w| w.join().expect("worker").1)
        .sum();
    assert_eq!(
        failed, 0,
        "serialised runs must not meet already-deleted files"
    );
    assert_eq!(dir_size(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_limit_is_in_binary_gigabytes() {
    assert_eq!(BYTES_PER_GB, 1_073_741_824);
}

#[test]
fn describe_reports_each_tracked_folder() {
    let dir = scratch("describe");
    write(&dir, "logs/a.log", 11);
    write(&dir, "subtitles/a.vtt", 22);
    write(&dir, "sources/t.json", 33);
    write(&dir, "updates/1/x.bin", 44);
    let dto = describe(&dir, 2);
    assert_eq!(dto.logs_bytes, 11);
    assert_eq!(dto.subtitles_bytes, 22);
    assert_eq!(dto.sources_bytes, 33);
    assert_eq!(dto.updates_bytes, 44);
    assert!(!dto.over);
    let _ = fs::remove_dir_all(&dir);
}
