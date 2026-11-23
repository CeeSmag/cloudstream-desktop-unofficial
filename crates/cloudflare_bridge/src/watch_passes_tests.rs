//! The watcher's saved-pass glue, driven step by step: no thread, no helper, a fixed clock.

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use crate::pass_book::MemoryPassBook;
use crate::pass_cookie::{CookieExpiry, PassCookie};
use crate::pass_fixtures::{cookie, folder, now, run};
use crate::pass_ids::PassId;
use crate::test_logs::captured;
use crate::wall_clock::FixedClock;
use crate::watch_requests::{PendingRequest, RequestStage};
use crate::{AllowedHosts, AskReason, CloudflareEvent, HelperStatus, RequestNeed};

use super::*;

const SECRET: &str = "clearance-value-kept-secret";

struct Rig {
    dir: PathBuf,
    events_tx: mpsc::Sender<CloudflareEvent>,
    events: Receiver<CloudflareEvent>,
    clock: Arc<FixedClock>,
}

impl Rig {
    fn new(name: &str) -> Self {
        let (events_tx, events) = mpsc::channel();
        Self {
            dir: folder(&format!("watch_passes_{name}")),
            events_tx,
            events,
            clock: Arc::new(FixedClock::at(now())),
        }
    }

    fn watcher(&self, allowed: &[&str]) -> Watcher<'_> {
        let passes = SavedPasses {
            book: Box::new(MemoryPassBook::default()),
            clock: Box::new(Arc::clone(&self.clock)),
            run: run(),
        };
        let allowed = AllowedHosts::new(allowed.iter().map(|host| (*host).to_owned()));
        Watcher::for_tests(self.dir.clone(), allowed, &self.events_tx, Some(passes))
    }

    fn result(&self, id: &str) -> String {
        std::fs::read_to_string(self.dir.join(format!("result-{id}.txt"))).unwrap_or_default()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn request(id: &str, host: &str) -> VerifyRequest {
    VerifyRequest {
        id: id.into(),
        host: host.into(),
        url: format!("https://{host}/watch/{id}"),
        method: "GET".into(),
        standalone: false,
        reason: AskReason::default(),
    }
}

fn issued(cookies: Vec<PassCookie>) -> IssuedPass {
    IssuedPass {
        user_agent: "Agent/1".into(),
        cookies,
    }
}

fn clearance(value: &str, domain: &str) -> PassCookie {
    let until = now() + Duration::from_secs(1_800);
    cookie("cf_clearance", value, domain, CookieExpiry::At(until))
}

/// The helper passed `request`'s check with `pass`.
fn pass_check(watcher: &mut Watcher<'_>, request: VerifyRequest, pass: IssuedPass) {
    let (host, id) = (request.host.clone(), request.id.clone());
    watcher.requests.insert(
        id.clone(),
        PendingRequest::new(request, RequestStage::Helper(7)),
    );
    watcher.browser_status(HelperStatus::Passed {
        host,
        id,
        issued: Some(pass),
    });
}

fn saved_id(watcher: &Watcher<'_>) -> Option<PassId> {
    let desk = watcher.passes.as_ref()?;
    let live = desk.passes.book.live_passes(desk.now());
    live.first().map(|pass| pass.id().clone())
}

#[test]
fn a_passed_check_is_kept_and_answers_a_sibling_host_at_once() {
    let rig = Rig::new("serve");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    let id = saved_id(&watcher).expect("kept");
    let events_before: Vec<_> = rig.events.try_iter().collect();
    assert!(matches!(
        events_before.as_slice(),
        [CloudflareEvent::Passed { .. }]
    ));
    watcher.route(request("r2", "api.site.test"));
    let answer = rig.result("r2");
    assert!(
        answer.starts_with("ok=true\nuserAgent=Agent/1\nsource=saved\n"),
        "{answer}"
    );
    assert!(answer.contains(&format!("pass={id}\n")), "{answer}");
    assert!(
        answer.contains(&format!("cookie=cf_clearance={SECRET}\n")),
        "{answer}"
    );
    assert!(
        rig.events.try_iter().next().is_none(),
        "no banner for a saved pass"
    );
    assert!(watcher.requests.is_empty(), "nothing waits");
}

#[test]
fn a_pass_the_site_refused_is_retired_and_the_site_checked_again() {
    let rig = Rig::new("reject");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    let id = saved_id(&watcher).expect("kept");
    let mut refused = request("r2", "www.site.test");
    refused.reason.rejected = crate::RejectedPass::Named(id);
    watcher.route(refused);
    assert!(saved_id(&watcher).is_none(), "retired");
    assert!(
        rig.result("r2").starts_with("ok=false"),
        "went to the (missing) window: {}",
        rig.result("r2")
    );
    watcher.route(request("r3", "www.site.test"));
    assert!(
        !rig.result("r3").contains("source=saved"),
        "never handed out again"
    );
    pass_check(
        &mut watcher,
        request("r4", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    assert!(
        saved_id(&watcher).is_none(),
        "the same clearance is not kept again"
    );
}

#[test]
fn requests_that_cannot_use_a_cookie_pass_go_to_the_window() {
    let rig = Rig::new("bypass");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    let mut browser = request("r2", "www.site.test");
    browser.reason.need = RequestNeed::Browser;
    watcher.route(browser);
    assert!(!rig.result("r2").contains("source=saved"));
    let mut retry = request("r3", "www.site.test");
    retry.standalone = true;
    watcher.route(retry);
    assert!(!rig.result("r3").contains("source=saved"));
    watcher.route(request("r4", "other.test"));
    assert!(!rig.result("r4").contains("source=saved"));
    assert!(saved_id(&watcher).is_some(), "the pass is still kept");
}

#[test]
fn an_origin_that_keeps_asking_without_naming_the_pass_stops_getting_it() {
    let rig = Rig::new("guard");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    let limit = crate::reask_guard::REASK_LIMIT;
    let served: Vec<bool> = (0..limit + 2)
        .map(|index| {
            let id = format!("q{index}");
            watcher.route(request(&id, "www.site.test"));
            rig.result(&id).contains("source=saved")
        })
        .collect();
    let expected: Vec<bool> = (0..limit + 2).map(|index| index < limit).collect();
    assert_eq!(served, expected);
    rig.clock.advance(Duration::from_secs(11));
    watcher.route(request("r9", "www.site.test"));
    assert!(
        rig.result("r9").contains("source=saved"),
        "the guard lifts after its window"
    );
}

#[test]
fn a_rejection_queued_behind_an_ordinary_request_retires_the_pass_for_everyone_after() {
    let rig = Rig::new("queued");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    let id = saved_id(&watcher).expect("kept");
    watcher.route(request("before", "www.site.test"));
    assert!(
        rig.result("before").contains("source=saved"),
        "not known to be refused yet"
    );
    let mut refused = request("refusal", "www.site.test");
    refused.reason.rejected = crate::RejectedPass::Named(id.clone());
    watcher.route(refused);
    for later in ["after-1", "after-2"] {
        watcher.route(request(later, "api.site.test"));
        assert!(!rig.result(later).contains("source=saved"), "{later}");
    }
    let mut late_duplicate = request("again", "www.site.test");
    late_duplicate.reason.rejected = crate::RejectedPass::Named(id);
    watcher.route(late_duplicate);
    assert!(saved_id(&watcher).is_none());
}

#[test]
fn a_pass_is_kept_only_for_a_request_the_watcher_sent_to_the_helper() {
    let rig = Rig::new("unknown");
    let mut watcher = rig.watcher(&["www.site.test"]);
    watcher.browser_status(HelperStatus::Passed {
        host: "www.site.test".into(),
        id: "never-asked".into(),
        issued: Some(issued(vec![clearance(SECRET, ".site.test")])),
    });
    assert!(saved_id(&watcher).is_none());
    let mut other_host = request("r1", "www.site.test");
    watcher.requests.insert(
        "r1".into(),
        PendingRequest::new(other_host.clone(), RequestStage::Helper(7)),
    );
    other_host.host = "evil.test".into();
    watcher.browser_status(HelperStatus::Passed {
        host: other_host.host,
        id: "r1".into(),
        issued: Some(issued(vec![clearance(SECRET, ".site.test")])),
    });
    assert!(
        saved_id(&watcher).is_none(),
        "the status names another host"
    );
}

#[test]
fn a_pass_without_a_usable_clearance_is_not_kept() {
    let rig = Rig::new("unusable");
    let mut watcher = rig.watcher(&["www.site.test"]);
    let session = cookie("cf_clearance", SECRET, ".site.test", CookieExpiry::Session);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![session]),
    );
    pass_check(
        &mut watcher,
        request("r2", "www.site.test"),
        issued(vec![clearance(SECRET, "other.test")]),
    );
    pass_check(
        &mut watcher,
        request("r3", "www.site.test"),
        issued(Vec::new()),
    );
    let mut bad_url = request("r4", "www.site.test");
    bad_url.url = "not a url".into();
    pass_check(
        &mut watcher,
        bad_url,
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    assert!(saved_id(&watcher).is_none());
}

#[test]
fn a_pass_that_cannot_be_written_to_the_sidecar_falls_back_to_the_window() {
    let rig = Rig::new("unwritable");
    let mut watcher = rig.watcher(&["www.site.test"]);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    std::fs::create_dir_all(rig.dir.join("result-r2.txt")).expect("block the answer");
    assert!(
        !watcher.answer_from_saved_pass(&request("r2", "www.site.test"), "https://www.site.test")
    );
    assert!(
        watcher.answer_from_saved_pass(&request("r3", "www.site.test"), "https://www.site.test")
    );
}

#[test]
fn without_saved_passes_everything_routes_as_before() {
    let rig = Rig::new("off");
    let allowed = AllowedHosts::new(["www.site.test".to_owned()]);
    let mut watcher = Watcher::for_tests(rig.dir.clone(), allowed, &rig.events_tx, None);
    pass_check(
        &mut watcher,
        request("r1", "www.site.test"),
        issued(vec![clearance(SECRET, ".site.test")]),
    );
    assert!(
        !watcher.answer_from_saved_pass(&request("r2", "www.site.test"), "https://www.site.test")
    );
    watcher.retire_rejected_pass(&request("r3", "www.site.test"));
}

#[test]
fn no_cookie_value_reaches_the_log() {
    let rig = Rig::new("logs");
    let ((), logs) = captured(|| {
        let mut watcher = rig.watcher(&["www.site.test"]);
        pass_check(
            &mut watcher,
            request("r1", "www.site.test"),
            issued(vec![clearance(SECRET, ".site.test")]),
        );
        watcher.route(request("r2", "api.site.test"));
        let id = saved_id(&watcher).expect("kept");
        let mut refused = request("r3", "www.site.test");
        refused.reason.rejected = crate::RejectedPass::Named(id);
        watcher.route(refused);
    });
    assert!(logs.contains("kept the check's Cloudflare pass"), "{logs}");
    assert!(
        logs.contains("answered from a saved Cloudflare pass"),
        "{logs}"
    );
    assert!(logs.contains("is retired"), "{logs}");
    assert!(!logs.contains(SECRET), "{logs}");
}
