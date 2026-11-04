//! Many episodes asked about from many threads at once: the shared memory must stay in bounds
//! and every caller must get the same answer for the same episode.

use std::time::Duration;

use super::*;
use crate::external_ids::ExternalIds;
use crate::skip_cache::MAX_CACHED;
use crate::test_fetcher::ScriptedFetcher;

const ANSWER: &str = r#"{"intro":[{"start_ms":1000,"end_ms":2000}]}"#;

fn query(episode: u32) -> SkipQuery {
    SkipQuery {
        ids: ExternalIds::new(None, Some("603"), None),
        kind: TitleKind::Series,
        season: Some(1),
        episode: Some(episode),
        duration_ms: 1_000_000,
    }
}

#[test]
fn threads_asking_for_many_episodes_get_consistent_answers_and_the_memory_stays_bounded() {
    let fetcher = Arc::new(ScriptedFetcher::answering(ANSWER).slow(Duration::from_millis(1)));
    let lookup = Arc::new(SkipLookup::new(Arc::clone(&fetcher) as Arc<dyn SkipFetcher>));
    let handles: Vec<_> = (0..8_u32)
        .map(|thread| {
            let lookup = Arc::clone(&lookup);
            std::thread::spawn(move || {
                for round in 0..150_u32 {
                    let episode = (thread * 31 + round) % 200;
                    let found = lookup.find(&query(episode)).expect("answered");
                    assert_eq!(found.len(), 1, "episode {episode}");
                    assert_eq!(found[0].start_ms, 1000);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("no thread panicked");
    }
    assert!(lookup.remembered() <= MAX_CACHED, "{}", lookup.remembered());
    assert!(
        fetcher.calls() >= 200,
        "every episode was asked at least once"
    );
}

#[test]
fn the_same_episode_asked_by_many_threads_at_once_is_answered_the_same_for_all() {
    let fetcher = Arc::new(ScriptedFetcher::answering(ANSWER).slow(Duration::from_millis(20)));
    let lookup = Arc::new(SkipLookup::new(Arc::clone(&fetcher) as Arc<dyn SkipFetcher>));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let lookup = Arc::clone(&lookup);
            std::thread::spawn(move || lookup.find(&query(1)).expect("answered"))
        })
        .collect();
    let answers: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("no thread panicked"))
        .collect();
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(lookup.remembered(), 1);
    // Once the first answer is remembered, nobody asks again.
    let before = fetcher.calls();
    lookup.find(&query(1)).expect("answered");
    assert_eq!(fetcher.calls(), before);
}
