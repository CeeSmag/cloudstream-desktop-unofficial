use std::time::{Duration, Instant};

use cloudflare_bridge::CloudflareEvent;

use super::*;

fn t0() -> Instant {
    Instant::now()
}

fn needs_permission(host: &str, id: &str) -> CloudflareEvent {
    CloudflareEvent::NeedsPermission {
        host: host.to_owned(),
        id: id.to_owned(),
    }
}

fn verifying(host: &str, id: &str) -> CloudflareEvent {
    CloudflareEvent::Verifying {
        host: host.to_owned(),
        id: id.to_owned(),
    }
}

fn passed(host: &str, id: &str) -> CloudflareEvent {
    CloudflareEvent::Passed {
        host: host.to_owned(),
        id: id.to_owned(),
    }
}

fn failed(host: &str, id: &str, reason: &str, retry: bool) -> CloudflareEvent {
    CloudflareEvent::Failed {
        host: host.to_owned(),
        id: id.to_owned(),
        reason: reason.to_owned(),
        retry,
    }
}

fn dismissed(id: &str) -> CloudflareEvent {
    CloudflareEvent::Dismissed { id: id.to_owned() }
}

fn shown_id(queue: &PromptQueue) -> Option<&str> {
    queue
        .shown
        .as_ref()
        .map(|entry| entry.prompt.request_id.as_str())
}

#[test]
fn empty_queue_projects_hidden_snapshot_and_no_actions() {
    let mut queue = PromptQueue::default();
    assert_eq!(
        queue.snapshot(),
        BannerSnapshot {
            phase: BannerPhase::Hidden,
            host: String::new(),
            message: String::new(),
            can_retry: false,
            waiting_count: 0,
            can_cycle: false,
        }
    );
    assert_eq!(queue.primary_action(), None);
    assert_eq!(queue.dismiss_shown(), None);
    assert!(!queue.cycle_next());
    assert!(!queue.begin_retry("r1", "retry-1".into()));
    assert!(!queue.expire(t0()));
}

#[test]
fn permission_granted_reports_host_without_mutating_banner() {
    let mut queue = PromptQueue::default();
    let now = t0();
    let outcome = queue.apply_event(
        CloudflareEvent::PermissionGranted {
            host: "vcloud.my".into(),
        },
        now,
    );
    assert_eq!(
        outcome,
        ApplyOutcome {
            granted_host: Some("vcloud.my".into()),
            passed: false,
            changed: false,
        }
    );
    assert_eq!(queue.snapshot().phase, BannerPhase::Hidden);
}

#[test]
fn first_prompt_is_shown_and_others_wait_in_arrival_order() {
    let mut queue = PromptQueue::default();
    let now = t0();
    let first = queue.apply_event(needs_permission("a.test", "a"), now);
    let second = queue.apply_event(needs_permission("b.test", "b"), now);
    let third = queue.apply_event(needs_permission("c.test", "c"), now);
    assert_eq!(
        (first.changed, second.changed, third.changed),
        (true, true, true)
    );
    assert_eq!(shown_id(&queue), Some("a"));
    assert_eq!(queue.snapshot().waiting_count, 2);
    assert!(queue.snapshot().can_cycle);
}

#[test]
fn dismissing_advances_queued_prompts_in_arrival_order() {
    let mut queue = PromptQueue::default();
    let now = t0();
    for (host, id) in [("a.test", "a"), ("b.test", "b"), ("c.test", "c")] {
        queue.apply_event(needs_permission(host, id), now);
    }
    assert_eq!(
        queue.dismiss_shown(),
        Some(BannerCommand::Dismiss { id: "a".into() })
    );
    assert_eq!(shown_id(&queue), Some("b"));
    assert!(queue.apply_event(dismissed("b"), now).changed);
    assert_eq!(shown_id(&queue), Some("c"));
    assert_eq!(
        (queue.snapshot().waiting_count, queue.snapshot().can_cycle),
        (0, false)
    );
}

#[test]
fn in_place_update_changes_shown_or_waiting_entry_without_duplicating() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(needs_permission("a.test", "a"), now);
    queue.apply_event(verifying("b.test", "b"), now);
    let outcome = queue.apply_event(failed("b.test", "b", "timeout", true), now);
    assert!(
        !outcome.changed,
        "hidden waiting update leaves banner unchanged"
    );
    assert_eq!(queue.snapshot().waiting_count, 1);

    let update = queue.apply_event(verifying("a.test", "a"), now);
    assert!(update.changed);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verifying);
    assert_eq!(shown_id(&queue), Some("a"));
    assert_eq!(queue.snapshot().waiting_count, 1);
}

#[test]
fn permission_request_preempts_verifying_and_preserves_original_arrival_order() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("first.test", "v1"), now);
    queue.apply_event(verifying("second.test", "v2"), now);
    queue.apply_event(needs_permission("ask1.test", "p1"), now);
    assert_eq!(shown_id(&queue), Some("p1"));
    assert_eq!(queue.snapshot().waiting_count, 2);

    // Dismiss p1 -> v1 resumes (it arrived before v2).
    queue.dismiss_shown();
    assert_eq!(shown_id(&queue), Some("v1"));

    // Preempt v1 again alongside a third verifying check v3; original FIFO order stays v1, v2, v3.
    queue.apply_event(verifying("third.test", "v3"), now);
    queue.apply_event(needs_permission("ask2.test", "p2"), now);
    assert_eq!(shown_id(&queue), Some("p2"));
    queue.dismiss_shown();
    assert_eq!(shown_id(&queue), Some("v1"));
    queue.apply_event(dismissed("v1"), now);
    assert_eq!(shown_id(&queue), Some("v2"));
    queue.apply_event(dismissed("v2"), now);
    assert_eq!(shown_id(&queue), Some("v3"));
}

#[test]
fn permission_prompt_is_never_preempted_and_outranks_failed_and_verifying() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(needs_permission("a.test", "a"), now);
    queue.apply_event(verifying("b.test", "b"), now);
    queue.apply_event(failed("d.test", "d", "closed", true), now);
    queue.apply_event(needs_permission("c.test", "c"), now);
    assert_eq!(shown_id(&queue), Some("a"));

    queue.dismiss_shown();
    assert_eq!(shown_id(&queue), Some("c"), "permission first");
    queue.dismiss_shown();
    assert_eq!(shown_id(&queue), Some("d"), "then failure");
    queue.dismiss_shown();
    assert_eq!(shown_id(&queue), Some("b"), "then verifying");
}

#[test]
fn check_that_ends_before_its_turn_is_dropped_silently() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(needs_permission("a.test", "a"), now);
    queue.apply_event(verifying("b.test", "b"), now);
    let pass_outcome = queue.apply_event(passed("b.test", "b"), now);
    assert!(pass_outcome.passed);
    assert!(pass_outcome.changed, "waiting count decreased from 1 to 0");
    assert_eq!(queue.snapshot().waiting_count, 0);
    assert_eq!(shown_id(&queue), Some("a"));

    queue.apply_event(verifying("c.test", "c"), now);
    queue.apply_event(dismissed("c"), now);
    assert_eq!(queue.snapshot().waiting_count, 0);
    assert_eq!(shown_id(&queue), Some("a"));
}

#[test]
fn passed_banner_immediately_yields_to_waiting_verifying_check() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), now);
    queue.apply_event(verifying("other.site", "r2"), now);

    let outcome = queue.apply_event(passed("vcloud.my", "r1"), now);
    assert!(outcome.passed);
    assert!(outcome.changed);
    assert_eq!(shown_id(&queue), Some("r2"));
    assert_eq!(queue.snapshot().phase, BannerPhase::Verifying);
    assert_eq!(queue.snapshot().host, "other.site");
    assert_eq!(queue.snapshot().waiting_count, 0);
}

#[test]
fn passed_banner_immediately_yields_to_waiting_failed_check() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), now);
    queue.apply_event(failed("other.site", "r2", "closed", true), now);

    let outcome = queue.apply_event(passed("vcloud.my", "r1"), now);
    assert!(outcome.passed);
    assert_eq!(shown_id(&queue), Some("r2"));
    assert_eq!(queue.snapshot().phase, BannerPhase::Failed);
    assert_eq!(queue.snapshot().waiting_count, 0);
}

#[test]
fn passed_banner_immediately_yields_to_waiting_permission_check_and_is_not_restored() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), now);
    queue.apply_event(passed("vcloud.my", "r1"), now);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);
    assert_eq!(queue.snapshot().host, "vcloud.my");

    queue.apply_event(needs_permission("other.site", "r2"), now);
    assert_eq!(shown_id(&queue), Some("r2"));
    assert_eq!(queue.snapshot().phase, BannerPhase::NeedsPermission);
    assert_eq!(queue.snapshot().waiting_count, 0);
}

#[test]
fn incoming_verifying_or_failed_check_immediately_replaces_shown_verified_banner() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), now);
    queue.apply_event(passed("vcloud.my", "r1"), now);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);

    // Same host or another host starting a verifying check must immediately replace Verified.
    queue.apply_event(verifying("vcloud.my", "r2"), now);
    assert_eq!(shown_id(&queue), Some("r2"));
    assert_eq!(queue.snapshot().phase, BannerPhase::Verifying);
    assert_eq!(queue.snapshot().waiting_count, 0);

    // Passing r2 and receiving a Failed check must also immediately replace Verified.
    queue.apply_event(passed("vcloud.my", "r2"), now);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);
    queue.apply_event(failed("other.site", "r3", "timed out", true), now);
    assert_eq!(shown_id(&queue), Some("r3"));
    assert_eq!(queue.snapshot().phase, BannerPhase::Failed);
    assert_eq!(queue.snapshot().waiting_count, 0);
}

#[test]
fn verified_banner_expires_at_exact_deadline_and_never_sticks() {
    let mut queue = PromptQueue::default();
    let start = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), start);
    queue.apply_event(passed("vcloud.my", "r1"), start);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);

    assert!(
        !queue.expire(start + VERIFIED_SHOWN - Duration::from_millis(1)),
        "still visible just before the deadline"
    );
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);

    assert!(
        queue.expire(start + VERIFIED_SHOWN),
        "expires deterministically at the exact deadline"
    );
    assert_eq!(queue.snapshot().phase, BannerPhase::Hidden);
    assert!(!queue.expire(start + VERIFIED_SHOWN + Duration::from_secs(1)));
}

#[test]
fn expire_does_nothing_for_active_prompts() {
    let mut queue = PromptQueue::default();
    let start = t0();
    let later = start + Duration::from_secs(600);
    for event in [
        needs_permission("a.test", "r1"),
        verifying("b.test", "r2"),
        failed("c.test", "r3", "err", true),
    ] {
        let mut q = PromptQueue::default();
        q.apply_event(event, start);
        assert!(!q.expire(later));
        assert_ne!(q.snapshot().phase, BannerPhase::Hidden);
    }
    assert!(!queue.expire(later));
}

#[test]
fn new_check_or_pass_for_same_host_clears_stale_failed_prompt() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(failed("VCloud.MY", "old", "closed", true), now);
    queue.apply_event(failed("other.site", "other", "closed", true), now);
    assert_eq!(shown_id(&queue), Some("old"));

    // New verifying check for same host (different case) clears stale failure and takes over.
    queue.apply_event(verifying("vcloud.my", "fresh"), now);
    assert_eq!(
        shown_id(&queue),
        Some("other"),
        "other host failure outranks verifying"
    );
    assert_eq!(queue.snapshot().waiting_count, 1);

    // Passing for other.site under a different request id clears other.site's stale failure too.
    queue.apply_event(passed("OTHER.SITE", "other-pass"), now);
    assert_eq!(shown_id(&queue), Some("fresh"));
    assert_eq!(queue.snapshot().phase, BannerPhase::Verifying);
    assert_eq!(queue.snapshot().waiting_count, 0);

    // Waiting failure for same host is also cleared when a NeedsPermission arrives for that host.
    queue.apply_event(failed("vcloud.my", "f-wait", "err", true), now);
    queue.apply_event(needs_permission("vcloud.my", "p-new"), now);
    assert_eq!(shown_id(&queue), Some("p-new"));
    assert_eq!(
        queue.snapshot().waiting_count,
        1,
        "only fresh verifying remains"
    );
}

#[test]
fn primary_action_maps_each_phase_and_ignores_non_retryable_or_verified() {
    let mut queue = PromptQueue::default();
    let now = t0();

    queue.apply_event(needs_permission("a.test", "r1"), now);
    assert_eq!(
        queue.primary_action(),
        Some(BannerCommand::Verify { id: "r1".into() })
    );

    queue.apply_event(verifying("a.test", "r1"), now);
    assert_eq!(
        queue.primary_action(),
        Some(BannerCommand::Show { id: "r1".into() })
    );

    queue.apply_event(failed("a.test", "r1", "closed", true), now);
    assert_eq!(
        queue.primary_action(),
        Some(BannerCommand::Retry { id: "r1".into() })
    );

    queue.apply_event(failed("a.test", "r1", "unsupported", false), now);
    assert_eq!(queue.primary_action(), None);

    queue.apply_event(verifying("a.test", "r1"), now);
    queue.apply_event(passed("a.test", "r1"), now);
    assert_eq!(queue.primary_action(), None);
}

#[test]
fn dismissing_verified_banner_emits_no_watcher_command() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(verifying("vcloud.my", "r1"), now);
    queue.apply_event(passed("vcloud.my", "r1"), now);
    assert_eq!(queue.snapshot().phase, BannerPhase::Verified);

    assert_eq!(queue.dismiss_shown(), None);
    assert_eq!(queue.snapshot().phase, BannerPhase::Hidden);
}

#[test]
fn begin_retry_validates_id_and_retryable_phase() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(failed("a.test", "r1", "blocked", false), now);
    assert!(!queue.begin_retry("r1", "retry-1".into()));

    queue.apply_event(failed("a.test", "r1", "closed", true), now);
    assert!(!queue.begin_retry("wrong", "retry-1".into()));
    assert!(queue.begin_retry("r1", "retry-1".into()));
    let snap = queue.snapshot();
    assert_eq!(
        (
            shown_id(&queue),
            snap.phase,
            snap.message.as_str(),
            snap.can_retry
        ),
        (Some("retry-1"), BannerPhase::Verifying, "", false)
    );
}

#[test]
fn begin_retry_reconciles_with_waiting_permission_prompt() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(failed("a.test", "r1", "closed", true), now);
    queue.apply_event(needs_permission("b.test", "p1"), now);
    assert_eq!(shown_id(&queue), Some("r1"), "failed is not preempted");

    assert!(queue.begin_retry("r1", "retry-2".into()));
    assert_eq!(
        shown_id(&queue),
        Some("p1"),
        "retrying yields to waiting permission"
    );
    assert_eq!(queue.snapshot().waiting_count, 1);
}

#[test]
fn cycle_next_is_noop_when_only_lower_priority_waits_or_verified_shown() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(needs_permission("a.test", "p1"), now);
    queue.apply_event(verifying("b.test", "v1"), now);
    assert_eq!(
        (queue.snapshot().can_cycle, queue.cycle_next()),
        (false, false)
    );
    assert_eq!(shown_id(&queue), Some("p1"));

    queue.dismiss_shown();
    queue.apply_event(passed("b.test", "v1"), now);
    assert!(!queue.cycle_next());
}

#[test]
fn cycle_next_rotates_same_priority_tier_round_robin() {
    let mut queue = PromptQueue::default();
    let now = t0();
    for (host, id) in [("one.test", "v1"), ("two.test", "v2"), ("three.test", "v3")] {
        queue.apply_event(verifying(host, id), now);
    }
    assert!(queue.snapshot().can_cycle);
    let mut seen = vec![shown_id(&queue).unwrap().to_owned()];
    for _ in 0..3 {
        assert!(queue.cycle_next());
        seen.push(shown_id(&queue).unwrap().to_owned());
    }
    assert_eq!(seen, ["v1", "v2", "v3", "v1"]);
}

#[test]
fn snapshot_projects_failed_reason_and_retry_flag() {
    let mut queue = PromptQueue::default();
    let now = t0();
    queue.apply_event(failed("site.test", "r1", "window closed", true), now);
    assert_eq!(
        queue.snapshot(),
        BannerSnapshot {
            phase: BannerPhase::Failed,
            host: "site.test".into(),
            message: "window closed".into(),
            can_retry: true,
            waiting_count: 0,
            can_cycle: false,
        }
    );
}
