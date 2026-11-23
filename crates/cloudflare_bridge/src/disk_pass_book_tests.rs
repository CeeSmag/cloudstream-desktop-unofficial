use std::thread;
use std::time::Duration;

use crate::pass_book::contract_tests::pass_book_contract;
use crate::pass_fixtures::{folder, later, now, pass, Spec};
use crate::test_logs::captured;

use super::*;

mod contract {
    use super::*;

    pass_book_contract!(|cap, name: &str| DiskPassBook::open_with_cap(
        folder(&format!("contract_{name}")),
        cap
    )
    .expect("book"));
}

fn book(name: &str) -> DiskPassBook {
    DiskPassBook::open(folder(name)).expect("book")
}

fn scope_path(book: &DiskPassBook) -> PathBuf {
    book.dir().join("domain-site.test.json")
}

#[test]
fn a_saved_pass_is_found_by_a_book_opened_later_on_the_same_folder() {
    let first = book("reopen");
    first.save(pass("p1", "a"), now()).expect("save");
    let second = DiskPassBook::open(first.dir()).expect("book");
    assert_eq!(second.live_passes(now()), vec![pass("p1", "a")]);
    let _ = fs::remove_dir_all(first.dir());
}

#[test]
fn a_saved_pass_file_is_written_whole_with_no_temporary_left() {
    let made = book("whole");
    made.save(pass("p1", "a"), now()).expect("save");
    let names: Vec<String> = fs::read_dir(made.dir())
        .expect("list")
        .flatten()
        .map(|item| item.file_name().to_string_lossy().into_owned())
        .filter(|name| name != LOCK_FILE)
        .collect();
    assert_eq!(names, ["domain-site.test.json"]);
    let _ = fs::remove_dir_all(made.dir());
}

#[cfg(unix)]
#[test]
fn the_folder_and_its_files_are_private_to_this_user() {
    use std::os::unix::fs::PermissionsExt;
    let made = book("private");
    made.save(pass("p1", "a"), now()).expect("save");
    let mode = |path: &Path| fs::metadata(path).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(made.dir()), 0o700);
    assert_eq!(mode(&scope_path(&made)), 0o600);
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn an_unreadable_file_is_removed_and_logged_without_its_content() {
    let cases: [(&str, String); 4] = [
        ("corrupt", "{not json secret-cookie".to_owned()),
        (
            "version",
            r#"{"version":2,"scope":".site.test","secret":"secret-cookie"}"#.to_owned(),
        ),
        (
            "oversized",
            format!("{{\"v\":\"secret-cookie{}\"}}", "x".repeat(70 * 1024)),
        ),
        (
            "misnamed",
            encode_scope(&ScopeEntry::empty(crate::CookieDomain::HostOnly(
                "other.test".into(),
            ))),
        ),
    ];
    for (name, text) in cases {
        let made = book(&format!("unreadable_{name}"));
        fs::write(scope_path(&made), &text).expect("write");
        let (found, logs) = captured(|| made.live_passes(now()));
        assert!(found.is_empty(), "{name}");
        assert!(!scope_path(&made).exists(), "{name}: removed");
        assert!(!logs.contains("secret-cookie"), "{name}: {logs}");
        assert!(
            logs.contains("removing a saved Cloudflare pass"),
            "{name}: {logs}"
        );
        let _ = fs::remove_dir_all(made.dir());
    }
}

#[test]
fn a_file_at_the_size_limit_is_still_read() {
    let made = book("at_limit");
    let text = encode_scope(&{
        let mut entry = ScopeEntry::empty(pass("p1", "a").scope().clone());
        entry.save(pass("p1", "a"), now());
        entry
    });
    let padded = format!(
        "{text}{}",
        " ".repeat(usize::try_from(MAX_PASS_FILE_BYTES).expect("fits") - text.len())
    );
    fs::write(scope_path(&made), padded).expect("write");
    assert_eq!(made.live_passes(now()).len(), 1);
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn other_files_in_the_folder_are_left_alone() {
    let made = book("others");
    fs::write(made.dir().join("notes.txt"), "x").expect("write");
    fs::create_dir_all(made.dir().join("sub.json")).expect("dir");
    assert!(made.live_passes(now()).is_empty());
    assert!(made.dir().join("notes.txt").exists());
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn prune_removes_temporaries_left_by_a_stopped_writer_but_not_fresh_ones() {
    let made = book("temps");
    let stale = made.dir().join(".domain-site.test.json.1.1.tmp");
    let fresh = made.dir().join(".domain-site.test.json.1.2.tmp");
    fs::write(&stale, "partial").expect("write");
    fs::write(&fresh, "partial").expect("write");
    let old = SystemTime::now() - Duration::from_secs(120);
    File::options()
        .write(true)
        .open(&stale)
        .and_then(|file| file.set_modified(old))
        .expect("age");
    made.prune(now()).expect("prune");
    assert!(!stale.exists());
    assert!(fresh.exists(), "another writer may still be renaming it");
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn a_folder_that_cannot_be_created_is_an_error_naming_it() {
    let dir = folder("blocked");
    let file = dir.join("file");
    fs::write(&file, "x").expect("write");
    let error = DiskPassBook::open(file.join("passes")).expect_err("not a folder");
    assert!(matches!(
        error,
        PassStoreError::Io {
            operation: "create the pass folder",
            ..
        }
    ));
    assert!(error.to_string().contains("passes"), "{error}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_that_vanished_reads_as_empty_and_cannot_be_written() {
    let made = book("vanished");
    fs::remove_dir_all(made.dir()).expect("remove");
    assert!(made.live_passes(now()).is_empty());
    let error = made.save(pass("p1", "a"), now()).expect_err("no folder");
    assert!(matches!(error, PassStoreError::Io { .. }), "{error}");
}

#[test]
fn a_change_waits_for_another_instance_s_lock_then_gives_up() {
    let made = book("lock_timeout");
    let holder = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(made.dir().join(LOCK_FILE))
        .expect("lock file");
    holder.lock().expect("held by the other instance");
    let started = Instant::now();
    let error = made.save(pass("p1", "a"), now()).expect_err("locked");
    assert!(
        matches!(error, PassStoreError::LockTimeout { .. }),
        "{error}"
    );
    assert!(started.elapsed() >= LOCK_WAIT);
    assert!(error.to_string().contains("locked"));
    holder.unlock().expect("release");
    made.save(pass("p1", "a"), now()).expect("free again");
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn a_rejection_survives_a_restart() {
    let first = book("reject_restart");
    first.save(pass("p1", "a"), now()).expect("save");
    assert!(first
        .reject(&PassId::parse("p1").expect("id"), now())
        .expect("reject"));
    let second = DiskPassBook::open(first.dir()).expect("book");
    assert_eq!(
        second.save(pass("p2", "a"), later(1)).expect("save"),
        SaveOutcome::Rejected
    );
    let _ = fs::remove_dir_all(first.dir());
}

/// Rounds each instance runs in the race below, and the scopes they share (few enough rejections per
/// scope that the per-scope cap never drops one).
const RACE_ROUNDS: usize = 200;
const RACE_SCOPES: usize = 50;

/// T2 race: two app instances (two books on one folder) save, reject and prune the same scopes at once.
/// Every rejection that reported success is remembered (no lost update between the instances' read and
/// write), and no reader ever finds a damaged file (writes are whole), whatever the interleaving.
#[test]
fn two_instances_on_one_folder_never_lose_a_rejection_or_see_a_partial_file() {
    let dir = folder("two_instances");
    let workers: Vec<_> = (0..2)
        .map(|side| {
            let book = DiskPassBook::open(&dir).expect("book");
            thread::spawn(move || captured(|| run_instance(&book, side)))
        })
        .collect();
    let mut rejected = Vec::new();
    for worker in workers {
        let (values, logs) = worker.join().expect("instance");
        assert!(!logs.contains("removing a saved Cloudflare pass"), "{logs}");
        rejected.extend(values);
    }
    assert!(
        rejected.len() > RACE_ROUNDS / 2,
        "the race rejected passes on both sides"
    );
    let reader = DiskPassBook::open(&dir).expect("reader");
    for (host, value) in rejected {
        let again = Spec::new("again", &value).host(&host).domain(&host).build();
        let outcome = reader.save(again, later(1)).expect("save");
        assert_eq!(outcome, SaveOutcome::Rejected, "{value} came back");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// One instance's work; returns the (host, clearance) of every pass it rejected.
fn run_instance(book: &DiskPassBook, side: usize) -> Vec<(String, String)> {
    let mut rejected = Vec::new();
    for index in 0..RACE_ROUNDS {
        let host = format!("h{}.site.test", index % RACE_SCOPES);
        let (id, value) = (format!("p{side}-{index}"), format!("v{side}-{index}"));
        let made = Spec::new(&id, &value).host(&host).domain(&host).build();
        book.save(made, now()).expect("save");
        for found in book.live_passes(now()) {
            assert!(found.is_fresh(now()), "every read is a whole, valid pass");
        }
        if book
            .reject(&PassId::parse(&id).expect("id"), now())
            .expect("reject")
        {
            rejected.push((host, value));
        }
        if index % 40 == 0 {
            book.prune(now()).expect("prune");
        }
    }
    rejected
}
