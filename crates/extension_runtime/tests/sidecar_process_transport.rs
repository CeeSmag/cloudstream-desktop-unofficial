//! T2: the real process transport against the scripted fake sidecar.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::time::Duration;

use extension_api::GatewayError;
use extension_runtime::{RpcTransport, SidecarLaunchConfig, SidecarProcessTransport};
use serde_json::{json, Value};

const SHORT: Duration = Duration::from_secs(5);

fn transport() -> SidecarProcessTransport {
    SidecarProcessTransport::new(SidecarLaunchConfig {
        program: env!("CARGO_BIN_EXE_fake_sidecar").into(),
        args: Vec::new(),
        environment: Vec::new(),
        os_fs_sandbox: false,
        spawner: None,
    })
}

#[test]
fn ping_round_trips_through_a_real_child_process() {
    assert_eq!(
        transport().call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
}

#[test]
fn echo_returns_the_params_unchanged() {
    let params = json!({"query": "moon landing", "page": 1});
    assert_eq!(transport().call("echo", params.clone(), SHORT), Ok(params));
}

#[test]
fn remote_error_is_reported_and_the_process_keeps_serving() {
    let transport = transport();
    assert!(matches!(
        transport.call("fail", Value::Null, SHORT),
        Err(GatewayError::Remote { kind, .. }) if kind == "TestFailure"
    ));
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
}

#[test]
fn non_json_noise_on_stdout_is_skipped() {
    assert_eq!(
        transport().call("noise", Value::Null, SHORT),
        Ok(json!("after-noise"))
    );
}

#[test]
fn hanging_call_times_out_and_next_call_restarts_the_process() {
    let transport = transport();
    let outcome = transport.call("hang", Value::Null, Duration::from_millis(300));
    assert!(
        matches!(outcome, Err(GatewayError::Timeout { .. })),
        "got {outcome:?}"
    );
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
}

#[test]
fn crashing_child_yields_transport_error_then_recovers() {
    let transport = transport();
    let outcome = transport.call("crash", Value::Null, SHORT);
    assert!(
        matches!(outcome, Err(GatewayError::Transport(_))),
        "got {outcome:?}"
    );
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
}

#[test]
fn transport_retries_once_when_the_child_died_between_calls() {
    let transport = transport();
    assert_eq!(
        transport.call("reply_then_die", Value::Null, SHORT),
        Ok(json!("bye"))
    );
    // The child is gone; the next call must restart inside `call` and still succeed.
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
}

#[test]
fn take_added_does_not_start_a_sidecar_when_none_is_running() {
    let transport = transport();
    assert_eq!(
        transport.call("reply_then_die", Value::Null, SHORT),
        Ok(json!("bye"))
    );
    // Child is gone; take_added must not spawn a replacement JVM for a background poll.
    let outcome = transport.call("take_added_repositories", Value::Null, SHORT);
    assert!(
        matches!(
            outcome,
            Err(GatewayError::Unavailable(_) | GatewayError::Transport(_))
        ),
        "got {outcome:?}"
    );
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong")),
        "a real call still starts a fresh sidecar"
    );
}

#[test]
fn concurrent_calls_after_the_child_exits_all_finish_without_hanging() {
    use std::sync::{Arc, Barrier};
    let transport = Arc::new(transport());
    assert_eq!(
        transport.call("reply_then_die", Value::Null, SHORT),
        Ok(json!("bye"))
    );
    let barrier = Arc::new(Barrier::new(9));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let transport = Arc::clone(&transport);
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            transport.call("ping", Value::Null, SHORT)
        }));
    }
    barrier.wait();
    for handle in handles {
        let outcome = handle.join().expect("thread");
        assert_eq!(outcome, Ok(json!("pong")));
    }
}

#[test]
fn missing_executable_is_reported_as_unavailable() {
    let transport = SidecarProcessTransport::new(SidecarLaunchConfig {
        program: "definitely-not-a-real-program".into(),
        args: Vec::new(),
        environment: Vec::new(),
        os_fs_sandbox: false,
        spawner: None,
    });
    assert!(matches!(
        transport.call("ping", Value::Null, SHORT),
        Err(GatewayError::Unavailable(_))
    ));
}

#[test]
fn remembered_calls_are_repeated_on_a_restarted_process() {
    let transport = transport();
    transport.remember_for_restart("ping", Value::Null, SHORT);
    // First start: nothing to repeat yet when the very first call arrives... except the remembered ping.
    assert_eq!(transport.call("handled", Value::Null, SHORT), Ok(json!(2)));
    // A crash discards the process; the next call starts a new one that first repeats the ping.
    assert!(transport.call("crash", Value::Null, SHORT).is_err());
    assert_eq!(
        transport.call("handled", Value::Null, SHORT),
        Ok(json!(2)),
        "the new process must have seen the remembered ping before this call"
    );
}

#[test]
fn the_same_remembered_call_is_not_repeated_twice() {
    let transport = transport();
    transport.remember_for_restart("ping", Value::Null, SHORT);
    transport.remember_for_restart("ping", Value::Null, SHORT);
    assert_eq!(transport.call("handled", Value::Null, SHORT), Ok(json!(2)));
}

#[test]
fn a_remembered_setting_is_repeated_once_with_its_latest_value() {
    let transport = transport();
    transport.remember_setting_for_restart("set_debug_logging", json!({"enabled": true}), SHORT);
    transport.remember_setting_for_restart("set_debug_logging", json!({"enabled": false}), SHORT);
    // One repeat on the first start plus the `handled` call itself.
    assert_eq!(transport.call("handled", Value::Null, SHORT), Ok(json!(2)));
}

#[test]
fn after_shutdown_no_sidecar_is_started_again() {
    let transport = transport();
    assert_eq!(
        transport.call("ping", Value::Null, SHORT),
        Ok(json!("pong"))
    );
    transport.shutdown();
    let outcome = transport.call("ping", Value::Null, SHORT);
    assert!(
        matches!(outcome, Err(GatewayError::Unavailable(_))),
        "got {outcome:?}"
    );
}

#[test]
fn shutdown_ends_the_sidecar_even_while_a_call_is_waiting_on_it() {
    use std::sync::Arc;
    use std::time::Instant;
    let transport = Arc::new(transport());
    let waiting = Arc::clone(&transport);
    let call = std::thread::spawn(move || {
        let started = Instant::now();
        let outcome = waiting.call("hang", Value::Null, Duration::from_secs(30));
        (outcome, started.elapsed())
    });
    // Let the call reach the sidecar and start waiting.
    std::thread::sleep(Duration::from_millis(800));
    transport.shutdown();
    let (outcome, took) = call.join().expect("call thread");
    assert!(outcome.is_err(), "the call must not succeed: {outcome:?}");
    assert!(
        took < Duration::from_secs(10),
        "the waiting call was not released by the shutdown (took {took:?})"
    );
}

#[test]
fn poll_link_search_after_sidecar_exit_returns_transport_without_retrying_dead_id() {
    let transport = transport();
    let search_id = transport
        .call("start_link_search", json!({"data": "1,20,0"}), SHORT)
        .expect("start search");
    assert_eq!(
        transport.call("reply_then_die", Value::Null, SHORT),
        Ok(json!("bye"))
    );
    std::thread::sleep(Duration::from_millis(100));
    let outcome = transport.call(
        "poll_link_search",
        json!({"id": search_id, "fromLink": 0, "fromSubtitle": 0, "waitMs": 50}),
        SHORT,
    );
    assert!(
        matches!(outcome, Err(GatewayError::Transport(_))),
        "must report Transport rather than retrying a dead UUID on a fresh JVM; got {outcome:?}"
    );
}

/// Linux: a sidecar with the production process limits (process group, parent-death signal).
#[cfg(target_os = "linux")]
fn limited_transport(
    spawner: Option<std::sync::Arc<extension_runtime::SidecarSpawner>>,
) -> SidecarProcessTransport {
    SidecarProcessTransport::new(SidecarLaunchConfig {
        program: env!("CARGO_BIN_EXE_fake_sidecar").into(),
        args: Vec::new(),
        environment: vec![(
            "JAVA_OPTS".to_owned(),
            "-Dcloudstream.data.dir=\"/tmp/unused\"".to_owned(),
        )],
        os_fs_sandbox: false,
        spawner,
    })
}

/// Calls `handled` (the fake's count of calls served by this process) from a thread that ends.
#[cfg(target_os = "linux")]
fn handled_from_short_thread(transport: &std::sync::Arc<SidecarProcessTransport>) -> Value {
    let caller = std::sync::Arc::clone(transport);
    std::thread::spawn(move || caller.call("handled", Value::Null, SHORT).expect("call"))
        .join()
        .expect("caller thread")
}

#[cfg(target_os = "linux")]
#[test]
fn sidecar_started_by_short_lived_caller_thread_keeps_serving_with_spawner() {
    let spawner = extension_runtime::SidecarSpawner::start().expect("spawner");
    let transport = std::sync::Arc::new(limited_transport(Some(std::sync::Arc::new(spawner))));
    assert_eq!(handled_from_short_thread(&transport), json!(1));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        handled_from_short_thread(&transport),
        json!(2),
        "the sidecar was restarted after its caller thread ended"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn sidecar_started_by_short_lived_caller_thread_dies_without_spawner() {
    // Documents the kernel rule the spawner works around: the parent-death signal follows the thread.
    let transport = std::sync::Arc::new(limited_transport(None));
    assert_eq!(handled_from_short_thread(&transport), json!(1));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        handled_from_short_thread(&transport),
        json!(1),
        "expected a fresh process"
    );
}
