use std::collections::VecDeque;

use core_model::{LinkSearchUpdate, PlayableLinks};

use super::*;
use crate::test_support::FakeGateway;

fn link(url: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        url: url.into(),
        quality,
        link_type: "VIDEO".into(),
        ..PlayableLink::default()
    }
}

fn update(links: Vec<PlayableLink>, done: bool) -> LinkSearchUpdate {
    LinkSearchUpdate {
        next_link: links.len(),
        links,
        done,
        success: done,
        ..LinkSearchUpdate::default()
    }
}

fn streaming(updates: Vec<Result<LinkSearchUpdate, GatewayError>>) -> Arc<FakeGateway> {
    let gateway = FakeGateway::default();
    *gateway.search_updates.lock().expect("lock") = Some(VecDeque::from(updates));
    Arc::new(gateway)
}

fn search(gateway: &Arc<FakeGateway>, data: &str) -> LinkSearch {
    LinkSearch::start(Arc::clone(gateway) as Arc<dyn ProviderGateway>, "P", data).expect("starts")
}

#[test]
fn streams_are_reported_as_they_arrive() {
    let gateway = streaming(vec![
        Ok(update(vec![link("a", 480)], false)),
        Ok(update(vec![link("b", 1080), link("c", 720)], true)),
    ]);
    let mut search = search(&gateway, "data");
    let first = search.poll(Duration::ZERO).expect("first");
    assert_eq!((first.links.len(), first.done), (1, false));
    let last = search.poll(Duration::ZERO).expect("last");
    let urls: Vec<_> = last.links.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(urls, ["b", "c"]);
    assert!(last.done && last.failure.is_none());
}

#[test]
fn streams_a_player_cannot_open_are_dropped() {
    let mut torrent = link("magnet", 4000);
    torrent.link_type = "MAGNET".into();
    let gateway = streaming(vec![Ok(update(vec![torrent, link("ok", 360)], true))]);
    let batch = search(&gateway, "data")
        .poll(Duration::ZERO)
        .expect("batch");
    assert_eq!(batch.links.len(), 1);
}

#[test]
fn an_empty_run_is_searched_again_and_a_later_run_wins() {
    let gateway = streaming(vec![
        Ok(update(vec![], true)),
        Ok(update(vec![link("late", 720)], true)),
    ]);
    let mut search = search(&gateway, "data");
    let first = search.poll(Duration::ZERO).expect("first");
    assert!(!first.done && first.links.is_empty(), "silently retried");
    let second = search.poll(Duration::ZERO).expect("second");
    assert!(second.done && second.links.len() == 1);
    assert_eq!(*gateway.searches_started.lock().expect("lock"), 2);
}

#[test]
fn three_empty_runs_end_the_search_and_a_url_payload_tries_the_extractor() {
    let gateway = streaming(vec![
        Ok(update(vec![], true)),
        Ok(update(vec![], true)),
        Ok(update(vec![], true)),
    ]);
    *gateway.extract_reply.lock().expect("lock") = Some(PlayableLinks {
        success: true,
        links: vec![link("via-extractor", 480)],
        subtitles: Vec::new(),
    });
    let mut search = search(&gateway, "https://host.test/v");
    let mut batch = LinkBatch::default();
    while !batch.done {
        batch = search.poll(Duration::ZERO).expect("poll");
    }
    assert_eq!(batch.links[0].url, "via-extractor");
}

#[test]
fn a_provider_failure_without_streams_is_reported_without_retrying() {
    let mut failed = update(vec![], true);
    failed.failure = Some("boom".into());
    let gateway = streaming(vec![Ok(failed)]);
    let batch = search(&gateway, "data")
        .poll(Duration::ZERO)
        .expect("batch");
    assert!(batch.done);
    assert_eq!(batch.failure.as_deref(), Some("boom"));
    assert_eq!(*gateway.searches_started.lock().expect("lock"), 1);
}

#[test]
fn a_runtime_that_cannot_stream_answers_in_one_piece() {
    let gateway = FakeGateway::default();
    gateway
        .link_replies
        .lock()
        .expect("lock")
        .push_back(Ok(PlayableLinks {
            success: true,
            links: vec![link("whole", 720)],
            subtitles: Vec::new(),
        }));
    let gateway = Arc::new(gateway);
    let batch = search(&gateway, "data")
        .poll(Duration::ZERO)
        .expect("batch");
    assert!(batch.done && batch.links.len() == 1);
}

#[test]
fn cancelling_stops_the_search_in_the_runtime() {
    let gateway = streaming(vec![]);
    search(&gateway, "data").cancel();
    assert_eq!(*gateway.searches_cancelled.lock().expect("lock"), 1);
}
