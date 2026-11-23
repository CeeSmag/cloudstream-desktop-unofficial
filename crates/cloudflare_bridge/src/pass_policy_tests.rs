use crate::pass_fixtures::{later, now, pass, run, Spec};
use crate::{AskReason, PassId};

use super::*;

const ORIGIN: &str = "https://api.site.test";

fn request(url: &str) -> VerifyRequest {
    VerifyRequest {
        id: "q1".into(),
        host: Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .unwrap_or_default(),
        url: url.into(),
        method: "GET".into(),
        standalone: false,
        reason: AskReason::default(),
    }
}

fn allowed() -> AllowedHosts {
    AllowedHosts::new(["www.site.test".to_owned()])
}

struct Facts {
    request: VerifyRequest,
    passes: Vec<SavedPass>,
    allowed: AllowedHosts,
    guard: ReaskGuard,
    now: SystemTime,
}

impl Facts {
    fn new(passes: Vec<SavedPass>) -> Self {
        Self {
            request: request("https://api.site.test/list?page=2"),
            passes,
            allowed: allowed(),
            guard: ReaskGuard::default(),
            now: now(),
        }
    }

    fn decide(&self) -> PassDecision {
        decide(&PassQuery {
            request: &self.request,
            origin: ORIGIN,
            passes: &self.passes,
            allowed: &self.allowed,
            guard: &self.guard,
            now: self.now,
            run: &run(),
        })
    }

    fn served_id(&self) -> Option<String> {
        match self.decide() {
            PassDecision::Serve(served) => Some(served.id.as_str().to_owned()),
            PassDecision::Bypass(_) => None,
        }
    }
}

fn bypass(reason: BypassReason) -> PassDecision {
    PassDecision::Bypass(reason)
}

#[test]
fn a_domain_pass_answers_a_sibling_host_of_its_site() {
    let facts = Facts::new(vec![pass("p1", "a")]);
    let PassDecision::Serve(served) = facts.decide() else {
        panic!("{:?}", facts.decide());
    };
    assert_eq!(
        served,
        pass("p1", "a").served_for(
            &Url::parse("https://api.site.test/list").expect("url"),
            now(),
            &run()
        )
    );
}

#[test]
fn a_viewer_retry_a_browser_only_origin_or_a_refused_pass_goes_to_the_window() {
    let mut facts = Facts::new(vec![pass("p1", "a")]);
    facts.request.standalone = true;
    assert_eq!(facts.decide(), bypass(BypassReason::Standalone));
    facts.request.standalone = false;
    facts.request.reason.need = RequestNeed::Browser;
    assert_eq!(facts.decide(), bypass(BypassReason::NeedsBrowser));
    facts.request.reason.need = RequestNeed::Any;
    facts.request.reason.rejected = RejectedPass::Named(PassId::parse("p1").expect("id"));
    assert_eq!(facts.decide(), bypass(BypassReason::RejectedPass));
    facts.request.reason.rejected = RejectedPass::Unnamed;
    assert_eq!(facts.decide(), bypass(BypassReason::RejectedPass));
}

#[test]
fn an_address_that_does_not_parse_goes_to_the_window() {
    let mut facts = Facts::new(vec![pass("p1", "a")]);
    facts.request.url = "https://".into();
    assert_eq!(facts.decide(), bypass(BypassReason::NotAWebPage));
}

#[test]
fn no_live_pass_that_reaches_the_address_means_no_answer() {
    let host_only = Spec::new("p1", "a").domain("www.site.test").build();
    assert_eq!(
        Facts::new(vec![host_only]).decide(),
        bypass(BypassReason::NoPass)
    );
    let mut expired = Facts::new(vec![pass("p2", "b")]);
    expired.now = later(1_800 - 60);
    assert_eq!(expired.decide(), bypass(BypassReason::NoPass));
    expired.now = later(1_800 - 61);
    assert_eq!(expired.served_id().as_deref(), Some("p2"));
    let mut plain = Facts::new(vec![pass("p3", "c")]);
    plain.request = request("http://api.site.test/");
    assert_eq!(
        plain.decide(),
        bypass(BypassReason::NoPass),
        "the clearance is Secure"
    );
    assert_eq!(
        Facts::new(Vec::new()).decide(),
        bypass(BypassReason::NoPass)
    );
}

#[test]
fn a_pass_whose_site_is_no_longer_allowed_is_not_handed_out() {
    let mut facts = Facts::new(vec![pass("p1", "a")]);
    facts.allowed = AllowedHosts::default();
    assert_eq!(facts.decide(), bypass(BypassReason::NotAllowed));
    facts.allowed = AllowedHosts::new(["site.test".to_owned()]);
    assert_eq!(
        facts.served_id().as_deref(),
        Some("p1"),
        "the parent site covers www"
    );
}

#[test]
fn the_most_recently_obtained_unguarded_pass_wins() {
    let older = Spec::new("old", "a").obtained(now()).build();
    let newer = Spec::new("new", "b")
        .host("www.site.test")
        .domain(".site.test")
        .obtained(later(5))
        .build();
    let api = Spec::new("api", "c")
        .host("api.site.test")
        .domain("api.site.test")
        .obtained(later(2))
        .build();
    let mut facts = Facts::new(vec![older, newer, api]);
    facts.allowed = AllowedHosts::new(["site.test".to_owned()]);
    facts.now = later(6);
    assert_eq!(facts.served_id().as_deref(), Some("new"));
    for _ in 0..crate::reask_guard::REASK_LIMIT {
        facts
            .guard
            .note_served(ORIGIN, &PassId::parse("new").expect("id"), later(6));
    }
    assert_eq!(
        facts.served_id().as_deref(),
        Some("api"),
        "the guarded pass is skipped"
    );
    for name in ["api", "old"] {
        for _ in 0..crate::reask_guard::REASK_LIMIT {
            facts
                .guard
                .note_served(ORIGIN, &PassId::parse(name).expect("id"), later(6));
        }
    }
    assert_eq!(facts.decide(), bypass(BypassReason::Guarded));
}

#[test]
fn every_reason_reads_well_in_the_log() {
    let reasons = [
        BypassReason::Standalone,
        BypassReason::NeedsBrowser,
        BypassReason::RejectedPass,
        BypassReason::NotAWebPage,
        BypassReason::NoPass,
        BypassReason::NotAllowed,
        BypassReason::Guarded,
    ];
    let texts: std::collections::HashSet<&str> =
        reasons.iter().map(|reason| reason.as_str()).collect();
    assert_eq!(texts.len(), reasons.len(), "each reason has its own text");
}
