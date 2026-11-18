//! T2: the watcher with a scripted helper process, on a real exchange folder.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cloudflare_bridge::{spawn_watcher, AllowedHosts, CloudflareEvent, HelperLaunch, WatchConfig};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cs_cf_t2_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

/// A helper that reports the window open and passes every verify at once (writes the result file).
fn fake_helper(dir: &Path) -> HelperLaunch {
    let script = format!(
        r#"while IFS="$(printf '\t')" read -r kind id host url; do
  case "$kind" in
    verify) printf 'open\t%s\n' "$host"
            printf 'ok=true\nuserAgent=Fake\ncookie=cf_clearance=x\n' > "{dir}/result-$id.tmp"
            mv "{dir}/result-$id.tmp" "{dir}/result-$id.txt"
            printf 'passed\t%s\n' "$host";;
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
    let text = format!("id={id}\nhost={host}\nurl=https://{host}/api/video.m3u8\n");
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

#[test]
fn not_allowed_site_is_answered_at_once_and_ui_is_asked() {
    let dir = folder("ask");
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: AllowedHosts::default(),
        helper: Some(fake_helper(&dir)),
    });
    request(&dir, "r1", "site.test");
    let answer = wait_for_file(&dir.join("result-r1.txt"), Duration::from_secs(3)).expect("answer");
    assert!(
        answer.contains("ok=false") && answer.contains("retry=true"),
        "{answer}"
    );
    let event = watch
        .events
        .recv_timeout(Duration::from_secs(2))
        .expect("event");
    assert_eq!(
        event,
        CloudflareEvent::NeedsPermission {
            host: "site.test".into()
        }
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
    }
    let mut passed = 0;
    while let Ok(event) = watch.events.recv_timeout(Duration::from_millis(500)) {
        if event
            == (CloudflareEvent::Passed {
                host: "cdn.site.test".into(),
            })
        {
            passed += 1;
        }
    }
    assert_eq!(passed, 20, "one pass per request");
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_now_allows_the_site_and_opens_its_front_page() {
    let dir = folder("now");
    let allowed = AllowedHosts::default();
    let watch = spawn_watcher(WatchConfig {
        dir: dir.clone(),
        allowed: allowed.clone(),
        helper: Some(fake_helper(&dir)),
    });
    watch.verify_now("site.test");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_passed = false;
    while Instant::now() < deadline && !saw_passed {
        if let Ok(event) = watch.events.recv_timeout(Duration::from_millis(200)) {
            saw_passed = event
                == (CloudflareEvent::Passed {
                    host: "site.test".into(),
                });
        }
    }
    assert!(saw_passed);
    assert!(allowed.is_allowed("site.test"));
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
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
