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
              first|app-retry-*) ;;
              second) finish "$id" "$host"; finish "first" "$host";;
              committed) publish "$id"; exit 1;;
              *) finish "$id" "$host";;
            esac;;
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
fn allowed_site_burst_is_verified_by_the_helper_each_request_answered_once() {
    let dir = folder("burst");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::new(["site.test".to_owned()]),
        helper: Some(fake_helper(&dir)),
    });
    for index in 0..20 {
        request(&dir, &format!("b{index}"), "cdn.site.test");
    }
    for index in 0..20 {
        let path = dir.join(format!("result-b{index}.txt"));
        let answer = wait_for_file(&path, Duration::from_secs(5)).expect("answer");
        assert!(answer.starts_with("ok=true"), "{answer}");
        assert!(dir.join(format!("verify-b{index}.received")).is_file());
        assert!(!dir.join(format!("present-b{index}.received")).exists());
    }
    let passed: std::collections::HashSet<_> = collect_passed(&watch, 20).into_iter().collect();
    assert_eq!(passed.len(), 20, "one pass per request");
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_buttons_keep_same_host_requests_separate_when_they_finish_in_reverse_order() {
    let dir = folder("now");
    let allowed = AllowedHosts::default();
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: allowed.clone(),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, "first", "site.test");
    request(&dir, "second", "site.test");
    for _ in 0..2 {
        assert!(matches!(
            watch.events.recv_timeout(Duration::from_secs(2)),
            Ok(CloudflareEvent::NeedsPermission { .. })
        ));
    }
    watch.verify_now("first");
    watch.verify_now("second");
    let passed = collect_passed(&watch, 2);
    assert_eq!(passed, ["second", "first"]);
    for id in passed {
        assert_eq!(
            std::fs::read_to_string(dir.join(format!("present-{id}.received")))
                .unwrap()
                .trim(),
            format!("https://site.test/api/{id}.m3u8")
        );
    }
    watch.show_check("first");
    assert_eq!(
        watch.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        CloudflareEvent::Dismissed { id: "first".into() }
    );
    assert!(!dir.join("show-site.test.received").exists());
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
