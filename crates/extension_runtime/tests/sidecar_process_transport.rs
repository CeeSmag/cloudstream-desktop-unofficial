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
fn missing_executable_is_reported_as_unavailable() {
    let transport = SidecarProcessTransport::new(SidecarLaunchConfig {
        program: "definitely-not-a-real-program".into(),
        args: Vec::new(),
        environment: Vec::new(),
    });
    assert!(matches!(
        transport.call("ping", Value::Null, SHORT),
        Err(GatewayError::Unavailable(_))
    ));
}
