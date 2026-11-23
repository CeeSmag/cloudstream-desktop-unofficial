//! T2: saved Cloudflare passes end to end. The real watcher thread, a scripted helper process that passes
//! every check and reports the pass, a real exchange folder and a real pass folder.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point
#![cfg(unix)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cloudflare_bridge::{
    spawn_watcher, AllowedHosts, HelperLaunch, SavedPasses, WatchConfig, WatchHandle,
};

/// How long a wait for the watcher (polling every 250 ms) may take before the test fails.
const PATIENCE: Duration = Duration::from_secs(5);

/// How long to watch for an answer that must not come (six watcher polls).
const ABSENCE: Duration = Duration::from_millis(1_500);

struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cs_cf_saved_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("exchange")).expect("exchange");
        Self { root }
    }

    fn exchange(&self) -> PathBuf {
        self.root.join("exchange")
    }

    fn passes(&self) -> PathBuf {
        self.root.join("passes")
    }

    fn start(&self, allowed: &[&str]) -> WatchHandle {
        spawn_watcher(WatchConfig {
            dir: self.exchange(),
            allowed: AllowedHosts::new(allowed.iter().map(|host| (*host).to_owned())),
            helper: Some(fake_helper(&self.exchange())),
            passes: Some(SavedPasses::on_disk(self.passes()).expect("pass folder")),
        })
    }

    fn ask(&self, id: &str, host: &str, extra: &str) {
        let dir = self.exchange();
        let text =
            format!("id={id}\nhost={host}\nurl=https://{host}/api/{id}\nmethod=GET\n{extra}");
        std::fs::write(dir.join(format!("request-{id}.tmp")), text).expect("write");
        std::fs::rename(
            dir.join(format!("request-{id}.tmp")),
            dir.join(format!("request-{id}.txt")),
        )
        .expect("publish");
    }

    fn answer(&self, id: &str) -> String {
        wait_for(|| std::fs::read_to_string(self.exchange().join(format!("result-{id}.txt"))).ok())
            .unwrap_or_else(|| panic!("no answer for {id}"))
    }

    /// What the sidecar does once it has its answer: removes its request and the answer.
    fn finish(&self, id: &str) {
        for name in [format!("request-{id}.txt"), format!("result-{id}.txt")] {
            let _ = std::fs::remove_file(self.exchange().join(name));
        }
    }

    fn reached_helper(&self, id: &str) -> bool {
        self.exchange()
            .join(format!("verify-{id}.received"))
            .exists()
            || self
                .exchange()
                .join(format!("present-{id}.received"))
                .exists()
    }

    /// Waits until the watcher has kept a pass for `scope_file`.
    fn wait_kept(&self, scope_file: &str) {
        let path = self.passes().join(scope_file);
        wait_for(|| path.exists().then_some(())).unwrap_or_else(|| panic!("{scope_file} not kept"));
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn wait_for<T>(probe: impl FnMut() -> Option<T>) -> Option<T> {
    wait_up_to(PATIENCE, probe)
}

fn wait_up_to<T>(limit: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < limit {
        if let Some(found) = probe() {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// The pass the fake browser reports: a clearance for `domain` lasting an hour, and a session cookie.
fn payload(domain: &str) -> String {
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_millis()
        + 3_600_000;
    let cookie = |name: &str, value: &str, expiry: &str| {
        format!(
            r#"{{"name":"{name}","value":"{value}","domain":"{domain}","path":"/","secure":true,"http_only":true{expiry}}}"#
        )
    };
    let clearance = cookie(
        "cf_clearance",
        "clear-value",
        &format!(r#","expires_ms":{expires}"#),
    );
    let session = cookie("sess", "session-value", "");
    format!(r#"{{"user_agent":"Fake/1","cookies":[{clearance},{session}]}}"#)
}

/// A helper that passes every check at once: it answers the sidecar's file and reports the pass. Hosts
/// under `solo.test` get a host-only clearance, others one for `.site.test`.
fn fake_helper(dir: &Path) -> HelperLaunch {
    let script = format!(
        r#"while IFS="$(printf '\t')" read -r kind id host url method standalone; do
  case "$kind" in
    verify|present)
      : > "{dir}/$kind-$id.received"
      printf 'ok=true\nuserAgent=Fake/1\ncookie=cf_clearance=clear-value\n' > "{dir}/result-$id.tmp"
      mv "{dir}/result-$id.tmp" "{dir}/result-$id.txt"
      case "$host" in
        *solo.test) pass='{solo}';;
        *) pass='{shared}';;
      esac
      printf 'passed\t%s\t%s\t%s\n' "$host" "$id" "$pass";;
    quit) exit 0;;
  esac
done"#,
        dir = dir.display(),
        solo = payload("www.solo.test"),
        shared = payload(".site.test"),
    );
    HelperLaunch {
        program: "sh".into(),
        args: vec!["-c".into(), script],
    }
}

fn pass_id(answer: &str) -> String {
    answer
        .lines()
        .find_map(|line| line.strip_prefix("pass="))
        .unwrap_or_else(|| panic!("not a saved answer: {answer}"))
        .to_owned()
}

#[test]
fn a_checked_site_s_pass_answers_its_sibling_host_without_a_window() {
    let rig = Rig::new("sibling");
    let watch = rig.start(&["www.site.test"]);
    rig.ask("r1", "www.site.test", "");
    assert!(rig.answer("r1").starts_with("ok=true"));
    rig.wait_kept("domain-site.test.json");
    rig.ask("r2", "api.site.test", "");
    let answer = rig.answer("r2");
    assert!(answer.contains("source=saved\n"), "{answer}");
    assert!(
        answer.contains("cookie=cf_clearance=clear-value\n"),
        "{answer}"
    );
    assert!(
        answer.contains("cookie=sess=session-value\n"),
        "same run: {answer}"
    );
    assert!(answer.contains("validUntil="), "{answer}");
    assert!(!rig.reached_helper("r2"));
    drop(watch);
}

#[test]
fn a_pass_survives_a_restart_without_its_session_cookies() {
    let rig = Rig::new("restart");
    let first = rig.start(&["www.site.test"]);
    rig.ask("r1", "www.site.test", "");
    rig.answer("r1");
    rig.wait_kept("domain-site.test.json");
    drop(first);
    let second = rig.start(&["www.site.test"]);
    rig.ask("r2", "www.site.test", "");
    let answer = rig.answer("r2");
    assert!(answer.contains("source=saved\n"), "{answer}");
    assert!(
        answer.contains("cookie=cf_clearance=clear-value\n"),
        "{answer}"
    );
    assert!(
        !answer.contains("sess="),
        "a session cookie ends with its run: {answer}"
    );
    assert!(!rig.reached_helper("r2"));
    drop(second);
}

#[test]
fn a_host_only_pass_is_not_shared_with_another_host() {
    let rig = Rig::new("host_only");
    let watch = rig.start(&["solo.test"]);
    rig.ask("r1", "www.solo.test", "");
    rig.answer("r1");
    rig.wait_kept("host-www.solo.test.json");
    rig.ask("r2", "api.solo.test", "");
    rig.answer("r2");
    assert!(
        rig.reached_helper("r2"),
        "the clearance was set for www only"
    );
    rig.ask("r3", "www.solo.test", "");
    assert!(rig.answer("r3").contains("source=saved"));
    drop(watch);
}

#[test]
fn a_refused_pass_opens_the_window_and_is_never_handed_out_again() {
    let rig = Rig::new("refused");
    let watch = rig.start(&["site.test"]);
    rig.ask("r1", "www.site.test", "");
    rig.answer("r1");
    rig.wait_kept("domain-site.test.json");
    rig.ask("r2", "www.site.test", "");
    let refused = pass_id(&rig.answer("r2"));
    rig.ask("r3", "www.site.test", &format!("rejected={refused}\n"));
    rig.answer("r3");
    assert!(rig.reached_helper("r3"), "checked again in the window");
    for id in ["r4", "r5", "r6"] {
        rig.ask(id, "api.site.test", "");
        let answer = rig.answer(id);
        assert!(!answer.contains("source=saved"), "{id}: {answer}");
        assert!(rig.reached_helper(id), "{id}");
    }
    drop(watch);
}

#[test]
fn a_browser_only_origin_and_a_viewer_retry_go_to_the_window() {
    let rig = Rig::new("browser_only");
    let watch = rig.start(&["www.site.test"]);
    rig.ask("r1", "www.site.test", "");
    rig.answer("r1");
    rig.wait_kept("domain-site.test.json");
    rig.ask("r2", "www.site.test", "need=browser\n");
    assert!(!rig.answer("r2").contains("source=saved"));
    assert!(rig.reached_helper("r2"));
    drop(watch);
}

#[test]
fn a_pass_for_a_site_that_is_no_longer_allowed_is_not_used() {
    let rig = Rig::new("not_allowed");
    let first = rig.start(&["www.site.test"]);
    rig.ask("r1", "www.site.test", "");
    rig.answer("r1");
    rig.wait_kept("domain-site.test.json");
    drop(first);
    rig.finish("r1");
    let second = rig.start(&[]);
    rig.ask("r2", "www.site.test", "");
    let early = wait_up_to(ABSENCE, || {
        std::fs::read_to_string(rig.exchange().join("result-r2.txt")).ok()
    });
    assert!(
        early.is_none(),
        "not answered from the pass: the viewer is asked first: {early:?}"
    );
    drop(second);
}

const STRESS_HOSTS: [&str; 3] = ["a.site.test", "b.site.test", "c.site.test"];

/// `plugins` threads each ask `asks` times, round-robin over the hosts, all at once.
fn ask_from_many_plugins(
    rig: &Arc<Rig>,
    plugins: usize,
    asks: usize,
) -> Vec<(String, &'static str)> {
    let threads: Vec<_> = (0..plugins)
        .map(|plugin| {
            let rig = Arc::clone(rig);
            std::thread::spawn(move || {
                (0..asks)
                    .map(|ask| {
                        let host = STRESS_HOSTS[(plugin + ask) % STRESS_HOSTS.len()];
                        let id = format!("p{plugin}-{ask}");
                        rig.ask(&id, host, "");
                        (id, host)
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    threads
        .into_iter()
        .flat_map(|plugin| plugin.join().expect("plugin"))
        .collect()
}

/// T2 race: eight "plugin processes" ask about three hosts of one checked site at once. Every request gets
/// exactly one answer, each answer comes from the saved pass or the window (never both, never neither), and
/// each host gets the pass exactly up to the re-ask limit within the guard's window.
#[test]
fn many_plugins_asking_at_once_each_get_one_answer_and_the_guard_holds() {
    const REASK_LIMIT: usize = 4; // reask_guard::REASK_LIMIT
    let rig = Arc::new(Rig::new("stress"));
    let watch = rig.start(&["site.test"]);
    rig.ask("warm", "www.site.test", "");
    rig.answer("warm");
    rig.wait_kept("domain-site.test.json");
    let started = Instant::now();
    let asked = ask_from_many_plugins(&rig, 8, 12);
    let mut saved_per_host: HashMap<&str, usize> = HashMap::new();
    for (id, host) in &asked {
        let saved = rig.answer(id).contains("source=saved");
        assert_ne!(
            saved,
            rig.reached_helper(id),
            "{id}: exactly one of pass or window"
        );
        *saved_per_host.entry(host).or_default() += usize::from(saved);
    }
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "inside one guard window"
    );
    for host in STRESS_HOSTS {
        assert_eq!(
            saved_per_host.get(host).copied().unwrap_or(0),
            REASK_LIMIT,
            "{host}"
        );
    }
    drop(watch);
}
