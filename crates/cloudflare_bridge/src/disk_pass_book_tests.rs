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

/// A valid scope file of exactly `len` bytes: the padding sits inside the JSON, before its last byte, so
/// any truncation breaks it.
fn padded_scope_file(len: usize) -> String {
    let text = encode_scope(&{
        let mut entry = ScopeEntry::empty(pass("p1", "a").scope().clone());
        entry.save(pass("p1", "a"), now());
        entry
    });
    let (body, last) = text.split_at(text.len() - 1);
    format!("{body}{}{last}", " ".repeat(len - text.len()))
}

#[test]
fn the_file_size_limit_is_64_kib_and_a_file_of_exactly_that_size_reads_whole() {
    assert_eq!(MAX_PASS_FILE_BYTES, 65_536);
    let made = book("exact_limit");
    let limit = usize::try_from(MAX_PASS_FILE_BYTES).expect("fits");
    fs::write(scope_path(&made), padded_scope_file(limit)).expect("write");
    assert_eq!(made.live_passes(now()).len(), 1, "not cut short");
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn a_file_one_byte_over_the_limit_is_refused_for_its_size() {
    let made = book("over_limit");
    let limit = usize::try_from(MAX_PASS_FILE_BYTES).expect("fits");
    fs::write(scope_path(&made), padded_scope_file(limit + 1)).expect("write");
    let (found, logs) = captured(|| made.live_passes(now()));
    assert!(found.is_empty());
    assert!(logs.contains("larger than a pass file can be"), "{logs}");
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn a_scope_file_that_cannot_be_read_is_logged_and_kept_and_a_missing_one_is_quiet() {
    let made = book("unreadable_entry");
    fs::create_dir_all(scope_path(&made)).expect("a folder where the file should be");
    let (found, logs) = captured(|| made.read_scope(&scope_path(&made)));
    assert!(found.is_none());
    assert!(
        logs.contains("cannot read a saved Cloudflare pass"),
        "{logs}"
    );
    assert!(
        scope_path(&made).exists(),
        "an I/O error is not damage: nothing is removed"
    );
    let missing = made.dir().join("domain-gone.test.json");
    let (found, logs) = captured(|| made.read_scope(&missing));
    assert!(found.is_none());
    assert!(
        logs.is_empty(),
        "a file another instance just removed is not worth a warning: {logs}"
    );
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn removing_an_absent_file_is_fine_and_a_folder_in_its_place_is_an_error() {
    let made = book("remove");
    assert!(remove_if_present(&made.dir().join("absent.json")).is_ok());
    let folder_in_place = made.dir().join("dir.json");
    fs::create_dir_all(folder_in_place.join("inner")).expect("folder");
    let error = remove_if_present(&folder_in_place).expect_err("not a file");
    assert!(matches!(
        error,
        PassStoreError::Io {
            operation: "remove a saved pass",
            ..
        }
    ));
    let _ = fs::remove_dir_all(made.dir());
}

#[test]
fn opening_creates_missing_parent_folders() {
    let root = folder("nested_open");
    let nested = root.join("a").join("b");
    let made = DiskPassBook::open(&nested).expect("book");
    assert!(nested.is_dir());
    made.save(pass("p1", "a"), now()).expect("save");
    assert_eq!(made.live_passes(now()).len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_temporary_file_is_stale_only_after_a_full_minute() {
    assert!(!is_stale_temp_age(
        STALE_TEMP_AGE - Duration::from_millis(1)
    ));
    assert!(!is_stale_temp_age(STALE_TEMP_AGE));
    assert!(is_stale_temp_age(STALE_TEMP_AGE + Duration::from_millis(1)));
}

#[test]
fn a_change_waits_for_the_lock_up_to_its_limit_and_not_beyond() {
    assert!(may_wait_longer(Duration::ZERO));
    assert!(may_wait_longer(LOCK_WAIT - Duration::from_millis(1)));
    assert!(!may_wait_longer(LOCK_WAIT));
}

#[test]
fn a_scope_file_that_cannot_be_replaced_fails_the_save_and_leaves_no_temporary() {
    let made = book("unreplaceable");
    fs::create_dir_all(scope_path(&made).join("inner")).expect("a folder in the file's place");
    let error = made
        .save(pass("p1", "a"), now())
        .expect_err("cannot rename over a folder");
    assert!(
        matches!(
            error,
            PassStoreError::Io {
                operation: "publish a saved pass",
                ..
            }
        ),
        "{error}"
    );
    let temps = fs::read_dir(made.dir())
        .expect("list")
        .flatten()
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "tmp"))
        .count();
    assert_eq!(temps, 0);
    let _ = fs::remove_dir_all(made.dir());
}

/// A folder this user may read but not change (as after a permission mix-up). Skipped when running
/// as root, which may write anyway.
#[cfg(unix)]
#[test]
fn a_read_only_folder_fails_writes_and_keeps_a_damaged_file_it_cannot_remove() {
    use std::os::unix::fs::PermissionsExt;
    let made = book("read_only");
    made.save(pass("p1", "a"), now()).expect("save");
    fs::write(made.dir().join("host-x.test.json"), "{damaged").expect("damaged file");
    fs::set_permissions(made.dir(), fs::Permissions::from_mode(0o500)).expect("read only");
    if fs::write(made.dir().join("probe"), "x").is_ok() {
        let _ = fs::set_permissions(made.dir(), fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(made.dir());
        return; // root: permissions do not hold
    }
    let other = Spec::new("p2", "b").domain("www.site.test").build();
    let error = made.save(other, now()).expect_err("read only");
    assert!(
        matches!(
            error,
            PassStoreError::Io {
                operation: "write a saved pass",
                ..
            }
        ),
        "{error}"
    );
    let (found, logs) = captured(|| made.live_passes(now()));
    assert_eq!(found.len(), 1, "the good pass is still read");
    assert!(
        logs.contains("could not remove an unreadable saved pass"),
        "{logs}"
    );
    let _ = fs::set_permissions(made.dir(), fs::Permissions::from_mode(0o700));
    let _ = fs::remove_dir_all(made.dir());
}
