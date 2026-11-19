//! TitleCache seeds for a stream open: fresh play-at-once, stale short probe.

use std::time::{Duration, Instant};

use app_services::StreamChecker;
use core_model::PlayableLink;
use title_cache::{Freshness, Lookup};

use crate::open_run::{check_one, check_pending, OpenState};
use crate::open_types::{OpenCache, OpenHooks};

/// How many saved streams to plant when the cache is fresh.
const TRUSTED_RECHECK: usize = 8;
/// How many saved streams to test first when the cache is stale.
const STALE_TEST_FIRST: usize = 3;
/// Cap for probing stale saved streams before falling through to the live search.
const STALE_TEST_SECS: u64 = 3;

pub(crate) fn plant_saved(
    state: &mut OpenState,
    open_cache: &OpenCache,
    checker: &StreamChecker,
    hooks: &OpenHooks<'_>,
) {
    let Some((freshness, seeds)) = saved_seeds(open_cache) else {
        return;
    };
    tracing::info!(
        ?freshness,
        seeds = seeds.len(),
        "planting saved sources for open"
    );
    for link in seeds {
        state.ordered.push((link, None));
    }
    (hooks.on_progress)(state.progress());
    (hooks.on_update)(&state.snapshot());
    match freshness {
        Freshness::Fresh => {
            check_trusted_seed(state, checker, hooks);
            check_pending(state, checker, hooks);
        }
        Freshness::Stale => check_stale_seeds(state, checker, hooks),
    }
}

fn saved_seeds(open_cache: &OpenCache) -> Option<(Freshness, Vec<PlayableLink>)> {
    let Lookup::Hit(hit) = open_cache.cache.load(&open_cache.title_key) else {
        return None;
    };
    let take = match hit.freshness {
        Freshness::Fresh => TRUSTED_RECHECK,
        Freshness::Stale => STALE_TEST_FIRST,
    };
    let seeds: Vec<PlayableLink> = hit
        .title
        .playing_order()
        .into_iter()
        .filter(|cached| cached.health != title_cache::LinkHealth::Broken)
        .take(take)
        .map(|cached| cached.link.clone())
        .collect();
    (!seeds.is_empty()).then_some((hit.freshness, seeds))
}

fn check_trusted_seed(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    let Some((link, slot)) = state.ordered.first_mut() else {
        return;
    };
    let health = checker.check(link);
    let is_ok = health.is_working();
    *slot = Some(health);
    state.checked = 1;
    state.next_to_check = 1;
    if is_ok {
        state.working = 1;
        state.play_index = Some(0);
        state.announce_ready(hooks);
    }
}

fn check_stale_seeds(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    let deadline = Instant::now() + Duration::from_secs(STALE_TEST_SECS);
    while state.next_to_check < state.ordered.len() && Instant::now() < deadline {
        if state.ready_announced {
            break;
        }
        check_one(state, checker, hooks);
        if state.play_index.is_some() {
            state.announce_ready(hooks);
            break;
        }
    }
}
