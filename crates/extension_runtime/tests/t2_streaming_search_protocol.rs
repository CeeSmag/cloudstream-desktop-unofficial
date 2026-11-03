//! T2: the streaming link search over the real JSON-lines process protocol, against the scripted
//! fake sidecar: a child process, the process transport and the gateway, with streams that arrive
//! over time. Checks that updates chain, that a long poll waits for news, that a cancelled search
//! is forgotten, and that starting and cancelling many searches leaves nothing running.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::time::{Duration, Instant};

use core_model::LinkSearchCursor;
use extension_api::{GatewayError, ProviderGateway};
use extension_runtime::{
    RpcProviderGateway, RpcTransport, SidecarLaunchConfig, SidecarProcessTransport,
};
use serde_json::{json, Value};

const CALL: Duration = Duration::from_secs(10);

fn transport() -> SidecarProcessTransport {
    SidecarProcessTransport::new(SidecarLaunchConfig {
        program: env!("CARGO_BIN_EXE_fake_sidecar").into(),
        args: Vec::new(),
        environment: Vec::new(),
    })
}

/// A search that finds `count` streams, one every `gap_ms`, and ends `tail_ms` after the last.
fn data(count: u32, gap_ms: u32, tail_ms: u32) -> String {
    format!("{count},{gap_ms},{tail_ms}")
}

#[test]
fn updates_arrive_over_time_chain_their_cursors_and_end_with_done() {
    let gateway = RpcProviderGateway::new(transport());
    let id = gateway
        .start_link_search("Fake", &data(6, 30, 40))
        .expect("search starts");
    let mut cursor = LinkSearchCursor::default();
    let (mut urls, mut updates) = (Vec::new(), 0);
    let started = Instant::now();
    loop {
        let update = gateway
            .poll_link_search(&id, cursor, Duration::from_millis(500))
            .expect("poll answers");
        urls.extend(update.links.iter().map(|link| link.url.clone()));
        cursor = update.cursor();
        updates += 1;
        if update.done {
            assert!(
                update.success,
                "a search that found streams ends in success"
            );
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the search never ended"
        );
    }
    assert_eq!(urls.len(), 6, "every stream arrives exactly once: {urls:?}");
    urls.sort();
    urls.dedup();
    assert_eq!(urls.len(), 6, "a stream arrived twice");
    assert!(updates > 1, "the streams arrived in more than one update");
}

#[test]
fn a_long_poll_waits_for_the_first_stream_and_returns_as_soon_as_it_arrives() {
    let gateway = RpcProviderGateway::new(transport());
    let id = gateway
        .start_link_search("Fake", &data(1, 250, 5_000))
        .expect("search starts");
    let started = Instant::now();
    let update = gateway
        .poll_link_search(&id, LinkSearchCursor::default(), Duration::from_secs(3))
        .expect("poll answers");
    let waited = started.elapsed();
    assert_eq!(update.links.len(), 1);
    assert!(
        waited >= Duration::from_millis(150) && waited < Duration::from_millis(2_500),
        "returned after {waited:?}"
    );
    gateway.cancel_link_search(&id).expect("cancel answers");
}

#[test]
fn a_cancelled_search_is_forgotten_and_asking_about_it_fails_clearly() {
    let gateway = RpcProviderGateway::new(transport());
    let id = gateway
        .start_link_search("Fake", &data(50, 20, 1_000))
        .expect("search starts");
    gateway.cancel_link_search(&id).expect("cancel answers");
    match gateway.poll_link_search(&id, LinkSearchCursor::default(), Duration::ZERO) {
        Err(GatewayError::Remote { message, .. }) => {
            assert!(
                message.contains("unknown link search"),
                "message was {message}"
            );
        }
        other => panic!("expected an error naming the unknown search, got {other:?}"),
    }
}

fn search_stats(transport: &SidecarProcessTransport) -> (u64, u64, u64) {
    let stats = transport
        .call("search_stats", Value::Null, CALL)
        .expect("stats answer");
    let field = |name: &str| stats[name].as_u64().expect("a number");
    (field("started"), field("cancelled"), field("active"))
}

/// Opening and leaving many videos in a row: every search is started, every one is either stopped
/// or read to its end, and the runtime ends with none left over.
#[test]
fn many_searches_started_and_cancelled_or_finished_leave_none_running() {
    const SEARCHES: u64 = 60;
    let transport = transport();
    let mut finished_by_reading = 0;
    for round in 0..SEARCHES {
        let id = transport
            .call(
                "start_link_search",
                json!({"provider": "Fake", "data": data(2, 5, 5)}),
                CALL,
            )
            .expect("search starts");
        if round % 3 == 0 {
            // The viewer stays to the end: read until the search says it is over.
            let mut cursor = (0, 0);
            loop {
                let update = transport
                    .call(
                        "poll_link_search",
                        json!({"id": id, "fromLink": cursor.0, "fromSubtitle": cursor.1, "waitMs": 100}),
                        CALL,
                    )
                    .expect("poll answers");
                cursor = (
                    update["nextLink"].as_u64().unwrap_or(0),
                    update["nextSubtitle"].as_u64().unwrap_or(0),
                );
                if update["done"] == true {
                    break;
                }
            }
            finished_by_reading += 1;
        } else {
            transport
                .call("cancel_link_search", json!({"id": id}), CALL)
                .expect("cancel answers");
        }
    }
    let (started, cancelled, active) = search_stats(&transport);
    assert_eq!(started, SEARCHES);
    assert_eq!(cancelled, SEARCHES - finished_by_reading);
    assert_eq!(active, 0, "searches were left running in the runtime");
}
