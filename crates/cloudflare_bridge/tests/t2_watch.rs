//! T2: the watcher with a scripted helper process, on a real exchange folder.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cloudflare_bridge::{
    spawn_watcher, AllowedHosts, CloudflareEvent, HelperLaunch, WatchConfig, WatchHandle,
};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cs_cf_t2_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

/// A helper that reports the window open and passes every verify at once (writes the result file).
fn fake_helper(dir: &Path) -> HelperLaunch {
    let script = format!(
        r#"publish() {{
  printf 'ok=true\nuserAgent=Fake\ncookie=cf_clearance=x\n' > "{dir}/result-$1.tmp"
  mv "{dir}/result-$1.tmp" "{dir}/result-$1.txt"
}}
finish() {{
  publish "$1"
  printf 'passed\t%s\t%s\n' "$2" "$1"
}}
while IFS="$(printf '\t')" read -r kind id host url method standalone; do
  printf '%s\n' "$url" > "{dir}/$kind-$id.received"
  case "$kind" in
    verify|present) printf 'open\t%s\t%s\n' "$host" "$id"
            case "$id" in
              failed-first) printf 'failed\t%s\t%s\tfixture failure\n' "$host" "$id";;
              first|app-retry-*|hold-*) ;;
              second) finish "$id" "$host"; finish "first" "$host";;
              committed) publish "$id"; exit 1;;
              *) finish "$id" "$host";;
            esac;;
    cancel) printf 'failed\tsite.test\t%s\tcancelled\n' "$id";;
    quit) exit 0;;
  esac
done"#,
        dir = dir.display()
    );
    HelperLaunch {
        program: "sh".into(),
        args: vec!["-c".into(), script],
    }
}

fn request(dir: &Path, id: &str, host: &str) {
    let text = format!("id={id}\nhost={host}\nurl=https://{host}/api/{id}.m3u8\nmethod=GET\n");
    std::fs::write(dir.join(format!("request-{id}.tmp")), text).expect("write");
    std::fs::rename(
        dir.join(format!("request-{id}.tmp")),
        dir.join(format!("request-{id}.txt")),
    )
    .expect("rename");
}

fn wait_for_file(path: &Path, limit: Duration) -> Option<String> {
    let started = Instant::now();
    while started.elapsed() < limit {
        if let Ok(text) = std::fs::read_to_string(path) {
            return Some(text);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn collect_passed(watch: &WatchHandle, total: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut passed = Vec::new();
    while passed.len() < total {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if let CloudflareEvent::Passed { id, .. } = watch
            .events
            .recv_timeout(remaining)
            .expect("terminal status")
        {
            passed.push(id);
        }
    }
    passed
}

/// The next event that `pick` accepts; others (permission grants and the like) are skipped.
fn next_matching<T>(watch: &WatchHandle, pick: impl Fn(&CloudflareEvent) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = watch
            .events
            .recv_timeout(remaining)
            .expect("expected event");
        if let Some(found) = pick(&event) {
            return found;
        }
    }
}

/// Waits until the helper has been told about every request, so the watcher has routed them all.
fn wait_forwarded(dir: &Path, ids: &[String]) {
    for id in ids {
        let marker = dir.join(format!("verify-{id}.received"));
        assert!(
            wait_for_file(&marker, Duration::from_secs(3)).is_some(),
            "{id} reached the helper"
        );
    }
}

fn verifying_id(event: &CloudflareEvent) -> Option<String> {
    match event {
        CloudflareEvent::Verifying { id, .. } => Some(id.clone()),
        _ => None,
    }
}

fn dismissed_id(event: &CloudflareEvent) -> Option<String> {
    match event {
        CloudflareEvent::Dismissed { id } => Some(id.clone()),
        _ => None,
    }
}

#[test]
fn pending_request_is_answered_when_the_viewer_dismisses_it() {
    let dir = folder("ask");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::default(),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, "r1", "site.test");
    let event = watch
        .events
        .recv_timeout(Duration::from_secs(2))
        .expect("event");
    assert_eq!(
        event,
        CloudflareEvent::NeedsPermission {
            host: "site.test".into(),
            id: "r1".into(),
        }
    );
    assert!(!dir.join("result-r1.txt").exists());
    watch.dismiss_check("r1");
    let answer = wait_for_file(&dir.join("result-r1.txt"), Duration::from_secs(2)).expect("answer");
    assert!(
        answer.contains("ok=false") && answer.contains("retry=true"),
        "{answer}"
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_burst_for_an_allowed_site_reaches_the_helper_whole_while_the_banner_hears_one_check() {
    let dir = folder("burst");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".to_owned()]),
        helper: Some(fake_helper(&dir)),
    });
    let ids: Vec<String> = (0..20).map(|index| format!("hold-b{index}")).collect();
    for id in &ids {
        request(&dir, id, "cdn.site.test");
    }
    wait_forwarded(&dir, &ids);
    for id in &ids {
        assert!(!dir.join(format!("present-{id}.received")).exists());
    }
    let owner = next_matching(&watch, verifying_id);
    assert!(ids.contains(&owner));
    assert!(
        watch
            .events
            .recv_timeout(Duration::from_millis(600))
            .is_err(),
        "the other 19 requests raise no events"
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A site the viewer has not allowed: one prompt, the extra request fails at once.
fn prompt_once_and_defer_the_extra_request(dir: &Path, watch: &WatchHandle) {
    request(dir, "first", "site.test");
    assert_eq!(
        watch
            .events
            .recv_timeout(Duration::from_secs(2))
            .expect("the prompt"),
        CloudflareEvent::NeedsPermission {
            host: "site.test".into(),
            id: "first".into()
        }
    );
    request(dir, "extra", "site.test");
    let answer =
        wait_for_file(&dir.join("result-extra.txt"), Duration::from_secs(2)).expect("answer");
    assert!(
        answer.contains("ok=false") && answer.contains("retry=true"),
        "{answer}"
    );
    assert!(
        !dir.join("verify-extra.received").exists(),
        "it never reaches the helper"
    );
}

#[test]
fn requests_for_one_site_share_one_prompt_and_the_prompted_request_is_the_one_opened() {
    let dir = folder("now");
    let allowed = AllowedHosts::default();
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: allowed.clone(),
        helper: Some(fake_helper(&dir)),
    });
    prompt_once_and_defer_the_extra_request(&dir, &watch);
    watch.verify_now("first");
    assert_eq!(next_matching(&watch, verifying_id), "first");
    request(&dir, "second", "site.test");
    assert_eq!(
        collect_passed(&watch, 1),
        ["first"],
        "the quiet request that finishes with it raises no event"
    );
    let opened = std::fs::read_to_string(dir.join("present-first.received")).unwrap();
    assert_eq!(
        opened.trim(),
        "https://site.test/api/first.m3u8",
        "the exact challenged address"
    );
    let second = wait_for_file(&dir.join("result-second.txt"), Duration::from_secs(2));
    assert!(second.expect("second answered").starts_with("ok=true"));
    watch.show_check("first");
    assert_eq!(next_matching(&watch, dismissed_id), "first");
    assert!(allowed.is_allowed("site.test"));
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn viewer_click_after_the_provider_withdraws_does_not_open_a_window() {
    let dir = folder("expired");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::default(),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, "withdrawn", "site.test");
    assert!(matches!(
        watch.events.recv_timeout(Duration::from_secs(3)),
        Ok(CloudflareEvent::NeedsPermission { .. })
    ));
    std::fs::remove_file(dir.join("request-withdrawn.txt")).unwrap();
    watch.verify_now("withdrawn");
    assert_eq!(
        watch.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        CloudflareEvent::Dismissed {
            id: "withdrawn".into()
        }
    );
    assert!(!dir.join("present-withdrawn.received").exists());
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retry_immediately_dismissed_cancels_its_new_window_id() {
    let dir = folder("retry");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".into()]),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, "failed-first", "site.test");
    for _ in 0..2 {
        let event = watch.events.recv_timeout(Duration::from_secs(2)).unwrap();
        if matches!(event, CloudflareEvent::Failed { id, retry: true, .. } if id == "failed-first")
        {
            let attempt = watch.retry_check("failed-first");
            watch.dismiss_check(&attempt);
            let asked = dir.join(format!("present-{attempt}.received"));
            assert_eq!(
                wait_for_file(&asked, Duration::from_secs(2))
                    .unwrap()
                    .trim(),
                "https://site.test/api/failed-first.m3u8"
            );
            let cancelled = dir.join(format!("cancel-{attempt}.received"));
            assert!(wait_for_file(&cancelled, Duration::from_secs(2)).is_some());
            drop(watch);
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    }
    panic!("initial request never failed");
}

#[test]
fn browser_fetch_without_helper_is_answered_gone_at_once() {
    let dir = folder("orphan");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::default(),
        helper: None,
    });
    std::fs::write(
        dir.join("fetch-f1.txt"),
        "id=f1\nhost=site.test\nmethod=GET\nurl=https://site.test/\n",
    )
    .expect("fetch");
    let answer =
        wait_for_file(&dir.join("fetchresult-f1.txt"), Duration::from_secs(3)).expect("answer");
    assert!(answer.contains("gone=true"), "{answer}");
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn allowed_site_without_helper_is_declined_with_reason() {
    let dir = folder("nohelper");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".to_owned()]),
        helper: None,
    });
    request(&dir, "n1", "site.test");
    let answer = wait_for_file(&dir.join("result-n1.txt"), Duration::from_secs(3)).expect("answer");
    assert!(
        answer.contains("ok=false") && !answer.contains("retry=true"),
        "{answer}"
    );
    assert!(matches!(
        watch.events.recv_timeout(Duration::from_secs(2)),
        Ok(CloudflareEvent::Failed { .. })
    ));
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn helper_exit_after_publishing_success_preserves_the_terminal_result() {
    let dir = folder("committed");
    let host = "site.test";
    let id = "committed";
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new([host.to_owned()]),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, id, host);
    assert_eq!(collect_passed(&watch, 1), [id]);
    let result = std::fs::read_to_string(dir.join(format!("result-{id}.txt"))).unwrap();
    assert!(result.starts_with("ok=true\n"), "{result}");
    assert!(watch
        .events
        .recv_timeout(Duration::from_millis(300))
        .is_err());
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_burst_for_an_unapproved_site_asks_the_viewer_once_and_the_rest_fail_at_once() {
    let dir = folder("unapproved");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::default(),
        helper: Some(fake_helper(&dir)),
    });
    let ids: Vec<String> = (0..5).map(|index| format!("u{index}")).collect();
    for id in &ids {
        request(&dir, id, "site.test");
    }
    let owner = match watch.events.recv_timeout(Duration::from_secs(2)).unwrap() {
        CloudflareEvent::NeedsPermission { host, id } => {
            assert_eq!(host, "site.test");
            id
        }
        other => panic!("expected the one prompt, got {other:?}"),
    };
    for id in ids.iter().filter(|id| **id != owner) {
        let path = dir.join(format!("result-{id}.txt"));
        let answer = wait_for_file(&path, Duration::from_secs(2)).expect("fails at once");
        assert!(
            answer.contains("retry=true"),
            "asked again next time, not held against the site: {answer}"
        );
    }
    assert!(
        !dir.join(format!("result-{owner}.txt")).exists(),
        "the prompted request waits for the viewer"
    );
    assert!(
        watch
            .events
            .recv_timeout(Duration::from_millis(600))
            .is_err(),
        "one prompt, not five"
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn when_the_provider_withdraws_the_prompted_request_the_banner_passes_to_the_next_one() {
    let dir = folder("withdraw");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".to_owned()]),
        helper: Some(fake_helper(&dir)),
    });
    let ids: Vec<String> = ["hold-1", "hold-2", "hold-3"].map(String::from).to_vec();
    for id in &ids {
        request(&dir, id, "site.test");
    }
    wait_forwarded(&dir, &ids);
    let owner = next_matching(&watch, verifying_id);
    std::fs::remove_file(dir.join(format!("request-{owner}.txt"))).unwrap();
    assert_eq!(next_matching(&watch, dismissed_id), owner);
    let next = next_matching(&watch, verifying_id);
    assert_ne!(next, owner, "a request that still waits speaks now");
    assert!(ids.contains(&next));
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dismissing_the_prompt_retires_every_request_that_waited_behind_it() {
    let dir = folder("dismiss");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".to_owned()]),
        helper: Some(fake_helper(&dir)),
    });
    let ids: Vec<String> = ["hold-1", "hold-2", "hold-3"].map(String::from).to_vec();
    for id in &ids {
        request(&dir, id, "site.test");
    }
    wait_forwarded(&dir, &ids);
    let owner = next_matching(&watch, verifying_id);
    watch.dismiss_check(&owner);
    assert_eq!(next_matching(&watch, dismissed_id), owner);
    for id in ids.iter().filter(|id| **id != owner) {
        let marker = dir.join(format!("cancel-{id}.received"));
        assert!(
            wait_for_file(&marker, Duration::from_secs(2)).is_some(),
            "{id} was cancelled in the helper"
        );
    }
    assert!(
        watch
            .events
            .recv_timeout(Duration::from_millis(600))
            .is_err(),
        "and nobody else is shown"
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Several threads write requests for three sites while the watcher routes them. Whatever the
/// interleaving, every request is answered, and the banner hears of at most one request per check.
const WRITERS: usize = 4;
const PER_WRITER: usize = 30;

/// Writer threads put requests for three sites into the folder at the same time.
fn write_concurrently(dir: &Path) {
    let writers: Vec<_> = (0..WRITERS)
        .map(|writer| {
            let dir = dir.to_path_buf();
            std::thread::spawn(move || {
                for index in 0..PER_WRITER {
                    let host = ["one.test", "two.test", "three.test"][index % 3];
                    request(&dir, &format!("s{writer}-{index}"), host);
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("writer");
    }
}

/// Several threads write requests for three sites while the watcher routes them. Whatever the
/// interleaving, every request is answered, and the banner hears of at most one request per check.
#[test]
fn concurrent_requests_for_several_sites_are_all_answered_and_only_owners_speak() {
    let dir = folder("stress");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["one.test".into(), "two.test".into(), "three.test".into()]),
        helper: Some(fake_helper(&dir)),
    });
    write_concurrently(&dir);
    for writer in 0..WRITERS {
        for index in 0..PER_WRITER {
            let path = dir.join(format!("result-s{writer}-{index}.txt"));
            let answer = wait_for_file(&path, Duration::from_secs(10)).expect("answered");
            assert!(answer.starts_with("ok=true"), "{answer}");
        }
    }
    let mut speakers = std::collections::HashSet::new();
    while let Ok(event) = watch.events.recv_timeout(Duration::from_millis(800)) {
        if let CloudflareEvent::Passed { id, .. } = event {
            assert!(speakers.insert(id), "a request passes once");
        }
    }
    assert!(speakers.len() >= 3, "each site has a request that speaks");
    assert!(
        speakers.len() < WRITERS * PER_WRITER,
        "not every request speaks"
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A helper that writes the arguments it was started with, then waits for `quit`.
fn args_recording_helper(dir: &Path) -> HelperLaunch {
    let script = format!(
        r#"printf '%s %s\n' "$0" "$1" > "{dir}/args.tmp"
mv "{dir}/args.tmp" "{dir}/args.txt"
while IFS="$(printf '\t')" read -r kind id host url method standalone; do
  case "$kind" in quit) exit 0;; esac
done"#,
        dir = dir.display()
    );
    HelperLaunch {
        program: "sh".into(),
        args: vec!["-c".into(), script],
    }
}

fn started_with(name: &str, windows: Option<usize>) -> String {
    let dir = folder(name);
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(vec!["site.test".to_owned()]),
        helper: Some(args_recording_helper(&dir)),
    });
    if let Some(count) = windows {
        watch.set_helper_windows(count);
    }
    request(&dir, "one", "site.test");
    wait_for_file(&dir.join("args.txt"), Duration::from_secs(5)).expect("the helper started")
}

#[test]
fn the_helper_is_started_with_the_number_of_windows_it_may_keep() {
    assert_eq!(
        started_with("windows_flag", Some(3)).trim(),
        "--max-windows 3"
    );
}

#[test]
fn a_helper_is_started_without_a_window_limit_until_one_is_set() {
    assert!(!started_with("windows_unset", None).contains("--max-windows"));
}
