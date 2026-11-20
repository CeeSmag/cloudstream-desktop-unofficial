//! Property-based tests for `title_cache` link caps, title eviction, and `playing_order`.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use core_model::PlayableLink;
use proptest::prelude::*;
use title_cache::{CacheLimits, CachedLink, CachedTitle, Clock, LinkHealth, Lookup, TitleCache};

fn step_clock(start_secs: u64) -> Clock {
    let state = Arc::new(Mutex::new(start_secs));
    Arc::new(move || {
        let mut g = state.lock().expect("clock");
        *g += 10;
        UNIX_EPOCH + Duration::from_secs(*g)
    })
}

fn make_link(idx: usize, quality: i32, health: LinkHealth) -> CachedLink {
    CachedLink {
        link: PlayableLink {
            source: "Prop".into(),
            name: format!("L{idx}"),
            url: format!("https://cdn.example/{idx}.mp4"),
            quality,
            link_type: "VIDEO".into(),
            ..PlayableLink::default()
        },
        health,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn title_cache_prune_enforces_max_titles_and_max_links(
        max_titles in 1usize..6,
        max_links in 1usize..10,
        title_count in 1usize..12,
        links_per_title in 1usize..16,
    ) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "cs_prop_tc_{}_{}_{}_{}",
            std::process::id(),
            nanos,
            max_titles,
            title_count
        ));
        let limits = CacheLimits {
            max_titles,
            max_links,
            ..CacheLimits::default()
        };
        let clock = step_clock(100_000);
        let cache = TitleCache::with_clock(dir.clone(), limits, clock);

        for t in 0..title_count {
            let links = (0..links_per_title)
                .map(|i| make_link(i, (i as i32) * 100, LinkHealth::Working))
                .collect();
            let title = CachedTitle {
                complete: true,
                links,
                last_good: None,
            };
            cache.save(&format!("title_{t}"), &title).expect("save");
        }

        let _ = cache.prune();
        let mut surviving = 0usize;
        for t in 0..title_count {
            if let Lookup::Hit(hit) = cache.load(&format!("title_{t}")) {
                surviving += 1;
                prop_assert!(hit.title.links.len() <= max_links);
            }
        }
        prop_assert!(surviving <= max_titles);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cached_title_playing_order_puts_working_before_broken_and_keeps_all_links(
        specs in prop::collection::vec((0i32..2160i32, 0u8..3u8), 1..25),
    ) {
        let links: Vec<CachedLink> = specs
            .iter()
            .enumerate()
            .map(|(idx, (q, h))| {
                let health = match h {
                    0 => LinkHealth::Working,
                    1 => LinkHealth::Unchecked,
                    _ => LinkHealth::Broken,
                };
                make_link(idx, *q, health)
            })
            .collect();
        let title = CachedTitle {
            complete: true,
            links,
            last_good: None,
        };
        let ordered = title.playing_order();
        prop_assert_eq!(ordered.len(), title.links.len());
        let first_broken = ordered.iter().position(|c| c.health == LinkHealth::Broken);
        if let Some(broken_pos) = first_broken {
            prop_assert!(ordered[broken_pos..]
                .iter()
                .all(|c| c.health == LinkHealth::Broken));
        }
    }
}
