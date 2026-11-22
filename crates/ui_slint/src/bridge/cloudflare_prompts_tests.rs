use super::*;

fn view(state: i32, host: &str, id: &str) -> CloudflareBannerView {
    CloudflareBannerView {
        state,
        host: host.to_owned(),
        message: String::new(),
        request_id: id.to_owned(),
        can_retry: state == 4,
    }
}

fn shown_id(queue: &PromptQueue) -> Option<&str> {
    queue.shown().map(|shown| shown.request_id.as_str())
}

#[test]
fn banner_for_maps_each_event_to_its_state() {
    let host = || "site.test".to_owned();
    let state = |event| banner_for(event).unwrap().state;
    assert_eq!(
        state(CloudflareEvent::NeedsPermission {
            host: host(),
            id: "r1".into()
        }),
        1
    );
    assert_eq!(
        state(CloudflareEvent::Verifying {
            host: host(),
            id: "r1".into()
        }),
        2
    );
    assert_eq!(
        state(CloudflareEvent::Passed {
            host: host(),
            id: "r1".into()
        }),
        3
    );
    assert_eq!(state(CloudflareEvent::Dismissed { id: "r1".into() }), 0);
    assert!(banner_for(CloudflareEvent::PermissionGranted { host: host() }).is_none());
    let failed = banner_for(CloudflareEvent::Failed {
        host: host(),
        id: "r1".into(),
        reason: "closed".into(),
        retry: true,
    })
    .unwrap();
    assert_eq!(
        (failed.state, failed.message.as_str(), failed.can_retry),
        (4, "closed", true)
    );
}

#[test]
fn the_first_prompt_is_shown_and_the_others_wait_in_arrival_order() {
    let mut queue = PromptQueue::default();
    queue.apply(view(1, "a.test", "a"));
    queue.apply(view(1, "b.test", "b"));
    queue.apply(view(1, "c.test", "c"));
    assert_eq!(shown_id(&queue), Some("a"));
    assert_eq!(queue.more(), 2);
    queue.finish_shown("a");
    assert_eq!(shown_id(&queue), Some("b"));
    queue.apply(view(0, "", "b"));
    assert_eq!(shown_id(&queue), Some("c"));
    assert_eq!(queue.more(), 0);
}

#[test]
fn an_update_for_the_prompt_on_screen_changes_it_in_place() {
    let mut queue = PromptQueue::default();
    queue.apply(view(1, "a.test", "a"));
    queue.apply(view(2, "a.test", "a"));
    assert_eq!(queue.shown().map(|shown| shown.state), Some(2));
    assert_eq!(queue.more(), 0);
}

#[test]
fn a_request_for_permission_takes_the_screen_from_a_check_that_only_runs() {
    let mut queue = PromptQueue::default();
    queue.apply(view(2, "a.test", "a"));
    queue.apply(view(1, "b.test", "b"));
    assert_eq!(
        shown_id(&queue),
        Some("b"),
        "the viewer can act on this one"
    );
    assert_eq!(queue.more(), 1, "the running check waits, it is not lost");
    queue.finish_shown("b");
    assert_eq!(shown_id(&queue), Some("a"));
}

#[test]
fn a_prompt_for_permission_is_never_preempted_and_never_waits_behind_a_running_check() {
    let mut queue = PromptQueue::default();
    queue.apply(view(1, "a.test", "a"));
    queue.apply(view(2, "b.test", "b"));
    queue.apply(view(1, "c.test", "c"));
    assert_eq!(shown_id(&queue), Some("a"));
    queue.finish_shown("a");
    assert_eq!(
        shown_id(&queue),
        Some("c"),
        "permission first, then the running check"
    );
    queue.finish_shown("c");
    assert_eq!(shown_id(&queue), Some("b"));
}

#[test]
fn a_check_that_ends_before_its_turn_is_never_shown() {
    let mut queue = PromptQueue::default();
    queue.apply(view(1, "a.test", "a"));
    queue.apply(view(2, "b.test", "b"));
    queue.apply(view(3, "b.test", "b"));
    assert_eq!(queue.more(), 0, "verified while waiting: no flicker");
    queue.apply(view(2, "c.test", "c"));
    queue.apply(view(0, "", "c"));
    assert_eq!(queue.more(), 0);
    assert_eq!(shown_id(&queue), Some("a"));
}

#[test]
fn a_verified_banner_gives_way_to_a_waiting_permission_request_and_is_not_restored() {
    let mut queue = PromptQueue::default();
    queue.apply(view(2, "a.test", "a"));
    queue.apply(view(3, "a.test", "a"));
    assert_eq!(queue.shown().map(|shown| shown.state), Some(3));
    queue.apply(view(1, "b.test", "b"));
    assert_eq!(shown_id(&queue), Some("b"));
    assert_eq!(queue.more(), 0, "a finished check does not come back");
}

#[test]
fn a_failure_is_shown_when_the_screen_is_free_and_waits_when_it_is_not() {
    let mut queue = PromptQueue::default();
    queue.apply(view(4, "a.test", "a"));
    assert_eq!(shown_id(&queue), Some("a"));
    queue.apply(view(4, "b.test", "b"));
    assert_eq!(queue.more(), 1);
    queue.apply(view(0, "", "a"));
    assert_eq!(shown_id(&queue), Some("b"));
}

#[test]
fn a_retry_replaces_the_failed_prompt_under_its_new_request() {
    let mut queue = PromptQueue::default();
    queue.apply(view(4, "a.test", "a"));
    queue.retrying("app-retry-1".to_owned());
    let shown = queue.shown().unwrap();
    assert_eq!(
        (shown.request_id.as_str(), shown.state, shown.can_retry),
        ("app-retry-1", 2, false)
    );
    queue.apply(view(3, "a.test", "app-retry-1"));
    assert_eq!(queue.shown().map(|shown| shown.state), Some(3));
}

#[test]
fn finishing_a_prompt_that_is_not_on_screen_changes_nothing() {
    let mut queue = PromptQueue::default();
    queue.apply(view(1, "a.test", "a"));
    queue.apply(view(1, "b.test", "b"));
    queue.finish_shown("b");
    assert_eq!(shown_id(&queue), Some("a"));
    assert_eq!(queue.more(), 1);
}
