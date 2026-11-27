//! Decoupled sidecar poller and parallel ranked stream-checking pool.
//!
//! Polling never blocks on HTTP stream probes, so newly discovered subtitles reach the UI within
//! one poll interval (~200 ms) even while slow or dead video hosts are being checked.

use std::sync::atomic::Ordering;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use app_services::{
    check_worker_count, link_rank, LinkBatch, LinkHealth, LinkSearch, StreamChecker,
};
use core_model::{PlayableLink, SubtitleTrack};

use crate::open_gate::{
    beats, best_after_working, no_streams_timed_out, should_announce_ready, ReadyWhen,
};
use crate::open_run::{finish, OpenState, FIRST_WORKING_SECS, TOTAL_SECS};
use crate::open_types::{CheckedOpen, OpenError, OpenHooks, OpenProgress};
use crate::subtitle_priority::{CheckGate, SUBTITLE_PRIORITY_GRACE};

/// Upper bound on concurrent HTTP stream checks during an open so low-end routers and CDNs are not
/// flooded while still checking multiple streams in parallel.
pub(crate) const MAX_OPEN_CHECK_WORKERS: usize = 6;
const POLL_WAIT: Duration = Duration::from_millis(200);
const WORKER_IDLE_WAIT: Duration = Duration::from_millis(100);

/// Wiring for [`run_search_pool`].
pub(crate) struct PoolConfig<'a> {
    pub(crate) checker: &'a StreamChecker,
    pub(crate) hooks: &'a OpenHooks<'a>,
    pub(crate) gate: &'a CheckGate,
    pub(crate) workers: usize,
    pub(crate) grace: Duration,
}

impl<'a> PoolConfig<'a> {
    pub(crate) fn standard(
        checker: &'a StreamChecker,
        hooks: &'a OpenHooks<'a>,
        gate: &'a CheckGate,
    ) -> Self {
        let cores = std::thread::available_parallelism().map_or(4, usize::from);
        let workers = check_worker_count(cores).min(MAX_OPEN_CHECK_WORKERS);
        Self {
            checker,
            hooks,
            gate,
            workers,
            grace: SUBTITLE_PRIORITY_GRACE,
        }
    }
}

struct SharedPool {
    state: OpenState,
    claimed: Vec<bool>,
    stopped: bool,
}

enum Claim {
    Job(usize, Box<PlayableLink>),
    Wait,
    Done,
}

struct Emit {
    progress: OpenProgress,
    snapshot: CheckedOpen,
    announce_index: Option<usize>,
}

impl SharedPool {
    fn new(state: OpenState) -> Self {
        let claimed = state
            .ordered
            .iter()
            .map(|(_, health)| health.is_some())
            .collect();
        Self {
            state,
            claimed,
            stopped: false,
        }
    }

    fn absorb(&mut self, links: Vec<PlayableLink>, tracks: Vec<SubtitleTrack>) -> Option<Emit> {
        let mut added = false;
        for link in links {
            if !self
                .state
                .ordered
                .iter()
                .any(|(known, _)| known.url == link.url)
            {
                self.state.ordered.push((link, None));
                self.claimed.push(false);
                added = true;
            }
        }
        for track in tracks {
            if !self
                .state
                .subtitles
                .iter()
                .any(|known| known.url == track.url)
            {
                self.state.subtitles.push(track);
                added = true;
            }
        }
        added.then(|| Emit {
            progress: self.state.progress(),
            snapshot: self.state.snapshot(),
            announce_index: None,
        })
    }

    fn claim_next(&mut self, cancelled: bool) -> Claim {
        if cancelled || self.stopped {
            return Claim::Done;
        }
        if self.state.timed_out() && self.state.play_index.is_some() {
            return Claim::Done;
        }
        if let Some(index) = pick_best_unclaimed(&self.state.ordered, &self.claimed) {
            if let Some(slot) = self.claimed.get_mut(index) {
                *slot = true;
            }
            if let Some((link, _)) = self.state.ordered.get(index) {
                return Claim::Job(index, Box::new(link.clone()));
            }
        }
        if self.state.search_done {
            Claim::Done
        } else {
            Claim::Wait
        }
    }

    fn record_check(&mut self, index: usize, health: LinkHealth, skip: bool) -> Emit {
        let is_ok = health.is_working();
        if let Some((_, slot)) = self.state.ordered.get_mut(index) {
            *slot = Some(health);
        }
        self.state.checked += 1;
        if is_ok {
            self.state.working += 1;
            self.state.play_index = Some(best_after_working(
                self.state.play_index,
                index,
                &self.state.ordered,
            ));
        }
        let announce_index = self.take_ready_if_due(skip);
        Emit {
            progress: self.state.progress(),
            snapshot: self.state.snapshot(),
            announce_index,
        }
    }

    fn take_ready_if_due(&mut self, skip: bool) -> Option<usize> {
        if self.state.ready_announced {
            return None;
        }
        let ready = should_announce_ready(ReadyWhen {
            has_working: self.state.play_index.is_some(),
            search_done: self.state.search_done,
            all_checked: self.state.all_checked(),
            skip,
            total_timed_out: self.state.timed_out(),
        });
        if ready {
            self.state.ready_announced = true;
            return self.state.play_index;
        }
        None
    }
}

/// Picks the highest-ranked unchecked, unclaimed link (earlier index breaks ties).
fn pick_best_unclaimed(
    ordered: &[(PlayableLink, Option<LinkHealth>)],
    claimed: &[bool],
) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for (index, (link, health)) in ordered.iter().enumerate() {
        if health.is_some() || claimed.get(index).copied().unwrap_or(true) {
            continue;
        }
        let rank = link_rank(link);
        if best.is_none_or(|(other_i, other_r)| beats(rank, index, other_r, other_i)) {
            best = Some((index, rank));
        }
    }
    best.map(|(index, _)| index)
}

fn emit_update(emit: &Emit, hooks: &OpenHooks<'_>) {
    (hooks.on_progress)(emit.progress);
    (hooks.on_update)(&emit.snapshot);
    if let Some(index) = emit.announce_index {
        (hooks.on_ready)(index);
    }
}

/// Runs sidecar polling and parallel stream checking concurrently until done, cancelled, or capped.
pub(crate) fn run_search_pool(
    mut search: LinkSearch,
    initial: OpenState,
    config: &PoolConfig<'_>,
) -> Result<CheckedOpen, OpenError> {
    let shared = Mutex::new(SharedPool::new(initial));
    let wake = Condvar::new();
    let poll_result = std::thread::scope(|scope| {
        for _ in 0..config.workers.max(1) {
            scope.spawn(|| worker_loop(&shared, &wake, config));
        }
        let result = poll_loop(&mut search, &shared, &wake, config);
        {
            let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
            guard.state.search_done = true;
            if result.is_err() || config.hooks.cancel.load(Ordering::SeqCst) {
                guard.stopped = true;
            }
        }
        config.gate.mark_search_done();
        wake.notify_all();
        result
    });
    search.cancel();
    poll_result?;
    let mut final_state = shared
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .state;
    final_state.maybe_announce_ready(config.hooks);
    finish(final_state)
}

fn should_stop_polling(guard: &SharedPool, hooks: &OpenHooks<'_>) -> bool {
    hooks.cancel.load(Ordering::SeqCst)
        || guard.state.started.elapsed() >= Duration::from_secs(TOTAL_SECS)
        || no_streams_timed_out(
            guard.state.ordered.len(),
            guard.state.started.elapsed().as_secs(),
            FIRST_WORKING_SECS,
        )
}

fn poll_loop(
    search: &mut LinkSearch,
    shared: &Mutex<SharedPool>,
    wake: &Condvar,
    config: &PoolConfig<'_>,
) -> Result<(), OpenError> {
    loop {
        let stop = {
            let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
            let ready = guard.take_ready_if_due(config.hooks.skip.load(Ordering::SeqCst));
            let stop = should_stop_polling(&guard, config.hooks);
            let snap = ready.map(|idx| (guard.state.snapshot(), idx));
            drop(guard);
            if let Some((snapshot, index)) = snap {
                (config.hooks.on_update)(&snapshot);
                (config.hooks.on_ready)(index);
            }
            stop
        };
        if stop {
            break;
        }
        let batch = search
            .poll(POLL_WAIT)
            .map_err(|error| OpenError::Search(error.to_string()))?;
        if handle_batch(batch, shared, wake, config)? {
            break;
        }
    }
    check_empty_timeout(shared)
}

struct BatchActions {
    emitted: Option<Emit>,
    ready_snap: Option<(CheckedOpen, usize)>,
    done_emit: Option<Emit>,
}

fn apply_batch_locked(
    guard: &mut SharedPool,
    batch: LinkBatch,
    skip: bool,
) -> Result<BatchActions, OpenError> {
    if let Some(failure) = &batch.failure {
        if guard.state.ordered.is_empty() {
            return Err(OpenError::Search(failure.clone()));
        }
    }
    let emitted = guard.absorb(batch.links, batch.subtitles);
    let ready_now = guard.take_ready_if_due(skip);
    if batch.done {
        guard.state.search_done = true;
    }
    let done_emit = batch.done.then(|| Emit {
        progress: guard.state.progress(),
        snapshot: guard.state.snapshot(),
        announce_index: ready_now.or_else(|| guard.take_ready_if_due(skip)),
    });
    let ready_snap = done_emit
        .is_none()
        .then(|| ready_now.map(|idx| (guard.state.snapshot(), idx)))
        .flatten();
    Ok(BatchActions {
        emitted,
        ready_snap,
        done_emit,
    })
}

fn handle_batch(
    batch: LinkBatch,
    shared: &Mutex<SharedPool>,
    wake: &Condvar,
    config: &PoolConfig<'_>,
) -> Result<bool, OpenError> {
    let skip = config.hooks.skip.load(Ordering::SeqCst);
    let actions = {
        let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
        apply_batch_locked(&mut guard, batch, skip)?
    };
    wake.notify_all();
    if let Some(emit) = actions.emitted {
        emit_update(&emit, config.hooks);
    }
    if let Some((snapshot, index)) = actions.ready_snap {
        (config.hooks.on_update)(&snapshot);
        (config.hooks.on_ready)(index);
    }
    if let Some(emit) = actions.done_emit {
        emit_update(&emit, config.hooks);
        config.gate.mark_search_done();
        return Ok(true);
    }
    Ok(false)
}

fn check_empty_timeout(shared: &Mutex<SharedPool>) -> Result<(), OpenError> {
    let guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.state.ordered.is_empty()
        && no_streams_timed_out(
            0,
            guard.state.started.elapsed().as_secs(),
            FIRST_WORKING_SECS,
        )
    {
        return Err(OpenError::Empty);
    }
    Ok(())
}

fn worker_loop(shared: &Mutex<SharedPool>, wake: &Condvar, config: &PoolConfig<'_>) {
    loop {
        if config.hooks.cancel.load(Ordering::SeqCst) {
            break;
        }
        if !config.gate.wait_until_runnable(config.grace) {
            break;
        }
        let Some((index, link)) = next_job(shared, wake, config.hooks) else {
            break;
        };
        let health = config.checker.check(&link);
        let skip = config.hooks.skip.load(Ordering::SeqCst);
        let emit = {
            let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
            guard.record_check(index, health, skip)
        };
        emit_update(&emit, config.hooks);
    }
}

fn next_job(
    shared: &Mutex<SharedPool>,
    wake: &Condvar,
    hooks: &OpenHooks<'_>,
) -> Option<(usize, PlayableLink)> {
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        let cancelled = hooks.cancel.load(Ordering::SeqCst);
        match guard.claim_next(cancelled) {
            Claim::Job(index, link) => return Some((index, *link)),
            Claim::Done => return None,
            Claim::Wait => {
                let (next, _) = wake
                    .wait_timeout(guard, WORKER_IDLE_WAIT)
                    .unwrap_or_else(PoisonError::into_inner);
                guard = next;
            }
        }
    }
}

#[cfg(test)]
#[path = "check_pool_tests.rs"]
mod tests;
