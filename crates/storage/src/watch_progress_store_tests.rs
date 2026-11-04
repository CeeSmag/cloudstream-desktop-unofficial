use super::*;

fn progress(key: &str, position_ms: i64, updated_at_ms: i64) -> WatchProgress {
    WatchProgress {
        provider: "P".to_owned(),
        media_key: key.to_owned(),
        title: format!("title {key}"),
        position_ms,
        duration_ms: 100_000,
        updated_at_ms,
        ..WatchProgress::default()
    }
}

fn store() -> WatchProgressStore {
    WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db")))
}

#[test]
fn save_replaces_the_position_for_the_same_item() {
    let store = store();
    store.save(&progress("a", 10, 1)).expect("save");
    store.save(&progress("a", 55, 2)).expect("save again");
    assert_eq!(
        store
            .get("P", "a")
            .expect("get")
            .expect("present")
            .position_ms,
        55
    );
}

#[test]
fn remove_forgets_only_that_item_and_says_whether_it_was_there() {
    let store = store();
    store.save(&progress("a", 10, 1)).expect("save");
    store.save(&progress("b", 10, 2)).expect("save");
    assert!(store.remove("P", "a").expect("remove"));
    assert!(!store.remove("P", "a").expect("remove again"));
    assert!(!store.remove("Other", "b").expect("other provider"));
    assert_eq!(store.get("P", "a").expect("get"), None);
    assert!(store.get("P", "b").expect("get").is_some());
}

#[test]
fn get_returns_none_for_unwatched_item() {
    assert_eq!(store().get("P", "nope").expect("get"), None);
}

#[test]
fn recent_orders_newest_first_and_honours_the_limit() {
    let store = store();
    for (key, at) in [("a", 1), ("b", 3), ("c", 2)] {
        store.save(&progress(key, 1, at)).expect("save");
    }
    let keys: Vec<_> = store
        .recent(2)
        .expect("recent")
        .into_iter()
        .map(|p| p.media_key)
        .collect();
    assert_eq!(keys, ["b", "c"]);
}

#[test]
fn episodes_for_show_and_remove_for_show_work_together() {
    let store = store();
    let mut p1 = progress("ep1", 10, 1);
    p1.detail_url = "https://show/1".to_owned();
    let mut p2 = progress("ep2", 20, 2);
    p2.detail_url = "https://show/1".to_owned();
    let mut p3 = progress("other", 30, 3);
    p3.detail_url = "https://show/2".to_owned();
    store.save(&p1).expect("save 1");
    store.save(&p2).expect("save 2");
    store.save(&p3).expect("save 3");

    let eps = store
        .episodes_for_show("P", "https://show/1")
        .expect("episodes");
    assert_eq!(eps.len(), 2);
    assert!(eps.contains(&"ep1".to_owned()));
    assert!(eps.contains(&"ep2".to_owned()));

    let deleted = store
        .remove_for_show("P", "https://show/1")
        .expect("remove show");
    assert_eq!(deleted, 2);
    let remaining = store
        .episodes_for_show("P", "https://show/1")
        .expect("empty");
    assert!(remaining.is_empty());
    assert!(store.get("P", "other").expect("get").is_some());
}

#[test]
fn all_media_keys_and_clear_work_correctly() {
    let store = store();
    store.save(&progress("k1", 10, 1)).expect("save");
    store.save(&progress("k2", 20, 2)).expect("save");
    let keys = store.all_media_keys().expect("keys");
    assert_eq!(keys.len(), 2);

    store.clear().expect("clear");
    let after = store.all_media_keys().expect("keys empty");
    assert!(after.is_empty());
}

#[test]
fn for_show_returns_the_positions_of_that_show_only() {
    let store = store();
    let mut first = progress("ep1", 10, 1);
    first.detail_url = "https://show/1".to_owned();
    let mut second = progress("ep2", 99_000, 2);
    second.detail_url = "https://show/1".to_owned();
    let mut other = progress("x", 5, 3);
    other.detail_url = "https://show/2".to_owned();
    for item in [&second, &other, &first] {
        store.save(item).expect("save");
    }

    let found = store.for_show("P", "https://show/1").expect("list");

    let keys: Vec<_> = found.iter().map(|p| p.media_key.as_str()).collect();
    assert_eq!(keys, ["ep1", "ep2"], "oldest first, other show left out");
    assert_eq!(found[1].position_ms, 99_000);
    assert!(store
        .for_show("P", "https://none")
        .expect("empty")
        .is_empty());
}
