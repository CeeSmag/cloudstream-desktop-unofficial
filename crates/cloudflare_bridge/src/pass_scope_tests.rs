use std::time::Duration;

use crate::pass_fixtures::{later, now, pass, Spec};

use super::*;

fn scope() -> ScopeEntry {
    ScopeEntry::empty(CookieDomain::Domain("site.test".into()))
}

fn id(text: &str) -> PassId {
    PassId::parse(text).expect("id")
}

#[test]
fn the_first_pass_is_saved_and_a_newer_one_replaces_it() {
    let mut entry = scope();
    assert_eq!(
        entry.save(pass("p1", "a"), now()),
        SaveOutcome::Saved { replaced: None }
    );
    let newer = Spec::new("p2", "b").obtained(later(5)).build();
    assert_eq!(
        entry.save(newer, later(5)),
        SaveOutcome::Saved {
            replaced: Some(id("p1"))
        }
    );
    assert_eq!(
        entry.live_pass(later(5)).map(|pass| pass.id().as_str()),
        Some("p2")
    );
}

#[test]
fn the_same_clearance_is_kept_once_under_its_first_id() {
    let mut entry = scope();
    entry.save(pass("p1", "a"), now());
    assert_eq!(
        entry.save(pass("p2", "a"), now()),
        SaveOutcome::AlreadyKept(id("p1"))
    );
    assert_eq!(
        entry.live_pass(now()).map(|pass| pass.id().as_str()),
        Some("p1")
    );
}

#[test]
fn an_older_pass_does_not_replace_a_newer_live_one() {
    let mut entry = scope();
    entry.save(Spec::new("p2", "b").obtained(later(10)).build(), later(10));
    let older = Spec::new("p1", "a").obtained(now()).build();
    assert_eq!(entry.save(older, later(10)), SaveOutcome::Outdated);
    assert_eq!(
        entry.live_pass(later(10)).map(|pass| pass.id().as_str()),
        Some("p2")
    );
}

#[test]
fn an_older_pass_replaces_a_kept_one_that_is_no_longer_fresh() {
    let mut entry = scope();
    entry.save(
        Spec::new("p2", "b").obtained(later(10)).lasts(100).build(),
        later(10),
    );
    let older_but_longer = Spec::new("p1", "a").lasts(3_600).build();
    assert_eq!(
        entry.save(older_but_longer, later(60)),
        SaveOutcome::Saved {
            replaced: Some(id("p2"))
        }
    );
}

#[test]
fn a_pass_too_close_to_its_end_is_not_saved() {
    let mut entry = scope();
    let short = Spec::new("p1", "a").lasts(60).build();
    assert_eq!(entry.save(short, now()), SaveOutcome::Stale);
    assert!(entry.is_empty());
}

#[test]
fn a_rejected_clearance_is_dropped_and_never_saved_again_until_it_expires() {
    let mut entry = scope();
    entry.save(pass("p1", "a"), now());
    assert!(!entry.reject(&id("other")));
    assert!(entry.reject(&id("p1")));
    assert!(entry.live_pass(now()).is_none());
    assert!(!entry.is_empty(), "the rejection is remembered");
    assert!(!entry.reject(&id("p1")), "a second rejection finds nothing");
    assert_eq!(
        entry.save(pass("p3", "a"), later(10)),
        SaveOutcome::Rejected
    );
    assert_eq!(
        entry.save(pass("p4", "b"), later(10)),
        SaveOutcome::Saved { replaced: None },
        "another clearance is fine"
    );
    let after_expiry = later(1_801);
    let again = Spec::new("p5", "a").obtained(after_expiry).build();
    let mut fresh = scope();
    fresh.rejected = entry.rejected.clone();
    assert_eq!(
        fresh.save(again, after_expiry),
        SaveOutcome::Saved { replaced: None }
    );
}

#[test]
fn rejections_are_capped_keeping_the_longest_lived() {
    let mut entry = scope();
    for index in 0..MAX_REJECTED_PER_SCOPE + 4 {
        let name = format!("p{index}");
        let lasts = 600 + u64::try_from(index).expect("small");
        entry.pass = Some(Spec::new(&name, &format!("v{index}")).lasts(lasts).build());
        assert!(entry.reject(&id(&name)));
    }
    assert_eq!(entry.rejected.len(), MAX_REJECTED_PER_SCOPE);
    assert!(entry
        .rejected
        .iter()
        .all(|rejected| rejected.value.expose() != "v0"));
    assert!(entry
        .rejected
        .iter()
        .any(|rejected| rejected.value.expose() == "v19"));
}

#[test]
fn prune_drops_what_can_no_longer_be_used() {
    let mut entry = scope();
    entry.save(Spec::new("p1", "a").lasts(600).build(), now());
    assert!(!entry.prune(now()), "nothing to drop yet");
    assert!(
        entry.prune(later(600) - Duration::from_secs(30)),
        "inside the margin"
    );
    assert!(entry.is_empty());
    entry.save(
        Spec::new("p2", "b").obtained(later(1_000)).build(),
        later(1_000),
    );
    entry.reject(&id("p2"));
    assert!(!entry.prune(later(1_000)));
    assert!(
        entry.prune(later(2_800)),
        "the rejection expired with its pass"
    );
    assert!(entry.is_empty());
}

#[test]
fn obtained_at_is_the_pass_s_and_none_for_rejections_only() {
    let mut entry = scope();
    assert_eq!(entry.obtained_at(), None);
    entry.save(pass("p1", "a"), now());
    assert_eq!(entry.obtained_at(), Some(now()));
    entry.reject(&id("p1"));
    assert_eq!(entry.obtained_at(), None);
}

#[test]
fn scopes_over_cap_evicts_the_oldest_passes_and_never_a_rejection_list() {
    let scopes = [
        ("new", Some(later(30))),
        ("guard", None),
        ("old", Some(now())),
        ("mid", Some(later(10))),
    ];
    assert!(scopes_over_cap(&scopes, 4).is_empty());
    assert!(scopes_over_cap(&scopes, 10).is_empty());
    assert_eq!(scopes_over_cap(&scopes, 3), ["old"]);
    assert_eq!(scopes_over_cap(&scopes, 2), ["old", "mid"]);
    assert_eq!(scopes_over_cap(&scopes, 0), ["old", "mid", "new"]);
    assert!(scopes_over_cap::<&str>(&[("guard", None)], 0).is_empty());
}

#[test]
fn a_different_pass_obtained_at_the_same_moment_replaces_the_kept_one() {
    let mut entry = scope();
    entry.save(pass("p1", "a"), now());
    assert_eq!(
        entry.save(pass("p2", "b"), now()),
        SaveOutcome::Saved {
            replaced: Some(id("p1"))
        },
        "only a strictly later pass is protected"
    );
}

#[test]
fn a_rejection_ends_exactly_when_its_clearance_would_have() {
    let mut entry = scope();
    entry.save(Spec::new("p1", "a").lasts(600).build(), now());
    assert!(entry.reject(&id("p1")));
    let just_before = Spec::new("p2", "a").obtained(later(599)).build();
    assert_eq!(entry.save(just_before, later(599)), SaveOutcome::Rejected);
    let at_the_end = Spec::new("p3", "a").obtained(later(600)).build();
    assert_eq!(
        entry.save(at_the_end, later(600)),
        SaveOutcome::Saved { replaced: None }
    );
}
