use super::*;
use crate::test_titles::TestTitles;
use title_cache::Lookup;

const KEY: &str = "P\nmovie";

fn link(url: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        ..PlayableLink::default()
    }
}

fn saved(url: &str, quality: i32, health: LinkHealth) -> CachedLink {
    CachedLink {
        link: link(url, quality),
        health,
    }
}

fn ledger(titles: &TestTitles, rest: CachedTitle) -> LinkLedger {
    LinkLedger::new(Arc::clone(&titles.cache), KEY.to_owned(), rest)
}

fn loaded(titles: &TestTitles) -> CachedTitle {
    match titles.cache.load(KEY) {
        Lookup::Hit(hit) => hit.title,
        Lookup::Miss(miss) => panic!("nothing saved: {miss:?}"),
    }
}

fn urls(title: &CachedTitle) -> Vec<&str> {
    title.links.iter().map(|c| c.link.url.as_str()).collect()
}

#[test]
fn only_streams_not_seen_before_are_added_in_the_order_they_came() {
    let titles = TestTitles::new();
    let ledger = ledger(&titles, CachedTitle::default());
    ledger.plant(&[saved("a", 720, LinkHealth::Working)]);
    let added = ledger.fresh(vec![
        link("a", 720),
        link("b", 480),
        link("b", 480),
        link("c", 1080),
    ]);
    let urls: Vec<_> = added.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(urls, ["b", "c"]);
    assert!(ledger.fresh(vec![link("c", 1080)]).is_empty());
}

#[test]
fn a_finished_search_saves_every_stream_with_its_answer_as_a_complete_list() {
    let titles = TestTitles::new();
    let ledger = ledger(&titles, CachedTitle::default());
    ledger.fresh(vec![link("a", 480), link("b", 1080), link("c", 720)]);
    ledger.answered(0, true);
    ledger.answered(1, false);
    ledger.answered(2, true);
    ledger.search_ended(true);

    ledger.save(Some("c".to_owned()), false);

    let found = loaded(&titles);
    assert!(found.complete);
    assert_eq!(
        urls(&found),
        ["c", "a", "b"],
        "last good, then working, broken last"
    );
    assert_eq!(found.last_good.as_deref(), Some("c"));
    assert_eq!(found.links[2].health, LinkHealth::Broken);
}

#[test]
fn a_video_closed_early_saves_what_is_known_as_partial_and_keeps_older_streams() {
    let titles = TestTitles::new();
    let older = CachedTitle {
        complete: true,
        links: vec![
            saved("old-1", 1080, LinkHealth::Working),
            saved("old-2", 720, LinkHealth::Unchecked),
        ],
        last_good: Some("old-1".to_owned()),
    };
    let ledger = ledger(&titles, older);
    ledger.fresh(vec![link("new", 480)]);
    ledger.answered(0, true);

    ledger.save(None, true);

    let found = loaded(&titles);
    assert!(!found.complete, "closed before the search finished");
    assert_eq!(found.links.len(), 3, "{:?}", urls(&found));
    assert_eq!(
        found.last_good.as_deref(),
        Some("old-1"),
        "the earlier last good is kept"
    );
}

#[test]
fn a_finished_search_does_not_keep_streams_the_provider_no_longer_offers() {
    let titles = TestTitles::new();
    let older = CachedTitle {
        links: vec![saved("gone", 1080, LinkHealth::Working)],
        ..CachedTitle::default()
    };
    let ledger = ledger(&titles, older);
    ledger.fresh(vec![link("now", 480)]);
    ledger.answered(0, true);
    ledger.search_ended(true);

    ledger.save(None, false);

    assert_eq!(urls(&loaded(&titles)), ["now"]);
}

#[test]
fn a_search_that_failed_is_saved_as_partial() {
    let titles = TestTitles::new();
    let ledger = ledger(&titles, CachedTitle::default());
    ledger.fresh(vec![link("a", 480)]);
    ledger.answered(0, true);
    ledger.search_ended(false);
    ledger.save(None, false);
    assert!(!loaded(&titles).complete);
}

#[test]
fn nothing_working_after_a_finished_search_removes_the_dead_saved_sources() {
    let titles = TestTitles::new();
    titles
        .cache
        .save(
            KEY,
            &CachedTitle {
                links: vec![saved("dead", 720, LinkHealth::Working)],
                ..CachedTitle::default()
            },
        )
        .expect("saved");
    let ledger = ledger(&titles, CachedTitle::default());
    ledger.plant(&[saved("dead", 720, LinkHealth::Working)]);
    ledger.answered(0, false);
    ledger.search_ended(true);

    ledger.save(None, false);

    assert!(matches!(titles.cache.load(KEY), Lookup::Miss(_)));
}

#[test]
fn nothing_working_when_the_video_was_closed_early_keeps_what_was_saved() {
    let titles = TestTitles::new();
    titles
        .cache
        .save(
            KEY,
            &CachedTitle {
                links: vec![saved("kept", 720, LinkHealth::Working)],
                ..CachedTitle::default()
            },
        )
        .expect("saved");
    let ledger = ledger(&titles, CachedTitle::default());
    ledger.fresh(vec![link("slow", 480)]);

    ledger.save(None, true);

    assert_eq!(urls(&loaded(&titles)), ["kept"]);
}

#[test]
fn a_cache_that_cannot_write_is_survived_without_a_panic() {
    let titles = TestTitles::new();
    let broken = Arc::new(title_cache::TitleCache::new(
        std::env::temp_dir().join("cs_ledger_no_folder_target"),
        title_cache::CacheLimits::default(),
    ));
    std::fs::write(
        std::env::temp_dir().join("cs_ledger_no_folder_target"),
        b"x",
    )
    .expect("file");
    let ledger = LinkLedger::new(broken, KEY.to_owned(), CachedTitle::default());
    ledger.fresh(vec![link("a", 480)]);
    ledger.answered(0, true);
    ledger.search_ended(true);

    ledger.save(None, false);

    assert!(std::fs::remove_file(std::env::temp_dir().join("cs_ledger_no_folder_target")).is_ok());
    drop(titles);
}
