//! Single-worker storage cleanup scheduler: post-Home trigger, hourly cadence, and manual reset.

use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::CleanupReport;

use super::runner::{CleanupMode, CleanupTask};

/// Timing parameters for automatic storage cleanup.
#[derive(Debug, Clone, Copy)]
pub struct CleanupSchedule {
    /// Wait after arriving at Home so shelf poster loading settles before the first pass.
    pub post_home_delay: Duration,
    /// Interval between automatic cleanup passes (reset whenever the user cleans manually).
    pub hourly_interval: Duration,
    /// Delay before retrying an automatic pass that was deferred during active video playback.
    pub busy_retry_delay: Duration,
}

impl Default for CleanupSchedule {
    fn default() -> Self {
        Self {
            post_home_delay: Duration::from_secs(15),
            hourly_interval: Duration::from_secs(60 * 60),
            busy_retry_delay: Duration::from_secs(5 * 60),
        }
    }
}

struct SchedulerState {
    shutdown: bool,
    home_ready: bool,
    next_auto_at: Option<Instant>,
    reset_generation: u64,
    manual_waiters: Vec<SyncSender<CleanupReport>>,
    cap_waiters: Vec<SyncSender<CleanupReport>>,
}

struct Shared {
    schedule: CleanupSchedule,
    state: Mutex<SchedulerState>,
    wake: Condvar,
}

/// Cloneable UI/bridge handle for notifying Home readiness, manual cleanup, and cap checks.
#[derive(Clone)]
pub struct CleanupHandle {
    shared: Arc<Shared>,
}

/// Owns the background `"storage-cleanup"` worker thread and shuts it down cleanly on drop.
pub struct CleanupScheduler {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

enum NextAction {
    Stop,
    Manual(Vec<SyncSender<CleanupReport>>),
    CapOnly(Vec<SyncSender<CleanupReport>>),
    Periodic { generation: u64 },
}

impl CleanupScheduler {
    /// Starts the background cleanup worker with the supplied task and schedule.
    pub fn start(task: Arc<dyn CleanupTask>, schedule: CleanupSchedule) -> Self {
        let shared = Arc::new(Shared {
            schedule,
            state: Mutex::new(SchedulerState {
                shutdown: false,
                home_ready: false,
                next_auto_at: None,
                reset_generation: 0,
                manual_waiters: Vec::new(),
                cap_waiters: Vec::new(),
            }),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = match thread::Builder::new()
            .name("storage-cleanup".into())
            .spawn(move || worker_loop(&worker_shared, task.as_ref()))
        {
            Ok(handle) => Some(handle),
            Err(error) => {
                tracing::warn!(%error, "could not start storage cleanup worker");
                lock_state(&shared.state).shutdown = true;
                None
            }
        };
        Self { shared, worker }
    }

    /// Returns a lightweight handle that can be shared with the UI bridge.
    pub fn handle(&self) -> CleanupHandle {
        CleanupHandle {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for CleanupScheduler {
    fn drop(&mut self) {
        {
            let mut state = lock_state(&self.shared.state);
            state.shutdown = true;
            state.manual_waiters.clear();
            state.cap_waiters.clear();
        }
        self.shared.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl CleanupHandle {
    /// Signals that launch has finished and the app arrived at Home. Idempotent.
    pub fn notify_home_ready(&self) {
        let mut state = lock_state(&self.shared.state);
        if state.shutdown || state.home_ready {
            return;
        }
        state.home_ready = true;
        state.next_auto_at = Some(Instant::now() + self.shared.schedule.post_home_delay);
        state.reset_generation = state.reset_generation.wrapping_add(1);
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Runs a full manual cleanup on the serialized worker and resets the hourly schedule.
    pub fn run_manual_now(&self) -> CleanupReport {
        let (tx, rx) = mpsc::sync_channel(1);
        {
            let mut state = lock_state(&self.shared.state);
            if state.shutdown {
                return CleanupReport::default();
            }
            state.manual_waiters.push(tx);
        }
        self.shared.wake.notify_all();
        rx.recv().unwrap_or_default()
    }

    /// Enforces the current storage limit on the serialized worker without resetting the hourly timer.
    pub fn enforce_cap_now(&self) -> CleanupReport {
        let (tx, rx) = mpsc::sync_channel(1);
        {
            let mut state = lock_state(&self.shared.state);
            if state.shutdown {
                return CleanupReport::default();
            }
            state.cap_waiters.push(tx);
        }
        self.shared.wake.notify_all();
        rx.recv().unwrap_or_default()
    }
}

fn worker_loop(shared: &Shared, task: &dyn CleanupTask) {
    loop {
        match next_action(shared) {
            NextAction::Stop => break,
            NextAction::Manual(waiters) => run_manual_pass(shared, task, waiters),
            NextAction::CapOnly(waiters) => run_cap_pass(task, waiters),
            NextAction::Periodic { generation } => run_periodic_pass(shared, task, generation),
        }
    }
}

fn next_action(shared: &Shared) -> NextAction {
    let mut state = lock_state(&shared.state);
    loop {
        if state.shutdown {
            state.manual_waiters.clear();
            state.cap_waiters.clear();
            return NextAction::Stop;
        }
        if !state.manual_waiters.is_empty() {
            return NextAction::Manual(std::mem::take(&mut state.manual_waiters));
        }
        if !state.cap_waiters.is_empty() {
            return NextAction::CapOnly(std::mem::take(&mut state.cap_waiters));
        }
        match state.next_auto_at {
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    state.next_auto_at = None;
                    return NextAction::Periodic {
                        generation: state.reset_generation,
                    };
                }
                let wait = deadline.saturating_duration_since(now);
                state = wait_timeout(&shared.wake, state, wait);
            }
            None => state = wait(&shared.wake, state),
        }
    }
}

fn run_manual_pass(
    shared: &Shared,
    task: &dyn CleanupTask,
    waiters: Vec<SyncSender<CleanupReport>>,
) {
    let report = task.run(CleanupMode::Manual);
    {
        let mut state = lock_state(&shared.state);
        state.reset_generation = state.reset_generation.wrapping_add(1);
        if !state.shutdown && state.home_ready {
            state.next_auto_at = Some(Instant::now() + shared.schedule.hourly_interval);
        }
    }
    for waiter in waiters {
        let _ = waiter.send(report);
    }
}

fn run_cap_pass(task: &dyn CleanupTask, waiters: Vec<SyncSender<CleanupReport>>) {
    let report = task.run(CleanupMode::CapOnly);
    for waiter in waiters {
        let _ = waiter.send(report);
    }
}

fn run_periodic_pass(shared: &Shared, task: &dyn CleanupTask, generation: u64) {
    let next_delay = if task.is_playback_busy() {
        tracing::debug!("deferring automatic storage cleanup while playback is active");
        shared.schedule.busy_retry_delay
    } else {
        let _ = task.run(CleanupMode::Periodic);
        shared.schedule.hourly_interval
    };
    let mut state = lock_state(&shared.state);
    if !state.shutdown
        && state.home_ready
        && state.reset_generation == generation
        && state.next_auto_at.is_none()
    {
        state.next_auto_at = Some(Instant::now() + next_delay);
    }
}

fn lock_state(mutex: &Mutex<SchedulerState>) -> MutexGuard<'_, SchedulerState> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait<'a>(
    wake: &Condvar,
    guard: MutexGuard<'a, SchedulerState>,
) -> MutexGuard<'a, SchedulerState> {
    wake.wait(guard).unwrap_or_else(PoisonError::into_inner)
}

fn wait_timeout<'a>(
    wake: &Condvar,
    guard: MutexGuard<'a, SchedulerState>,
    duration: Duration,
) -> MutexGuard<'a, SchedulerState> {
    wake.wait_timeout(guard, duration)
        .unwrap_or_else(PoisonError::into_inner)
        .0
}
