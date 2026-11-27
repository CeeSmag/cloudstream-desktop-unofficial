//! Per-session circuit breaker for subtitle hosts that fail repeatedly (e.g. HTTP 403 or
//! timeouts), so remaining queued files from a broken host fail fast without stalling workers.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// Consecutive failures on one host before remaining files on that host are skipped.
pub const HOST_FAIL_THRESHOLD: usize = 3;

#[derive(Default)]
struct HostState {
    consecutive_failures: usize,
    tripped: bool,
}

/// Extracts the normalized ASCII lowercase host from an HTTP(S) URL.
pub fn host_of(url: &str) -> Option<String> {
    let (_, after_scheme) = url.trim().split_once("://")?;
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, rest)| rest);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']').map(|(ipv6, _)| ipv6)?
    } else {
        host_port.split_once(':').map_or(host_port, |(h, _)| h)
    };
    let trimmed = host.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

/// Tracks per-host consecutive failures for one subtitle prefetch session.
#[derive(Default)]
pub struct HostCircuit {
    threshold: usize,
    hosts: Mutex<HashMap<String, HostState>>,
}

impl HostCircuit {
    /// Creates a circuit breaker with the default [`HOST_FAIL_THRESHOLD`].
    pub fn new() -> Self {
        Self::with_threshold(HOST_FAIL_THRESHOLD)
    }

    /// Creates a circuit breaker that trips after `threshold` consecutive failures.
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            threshold: threshold.max(1),
            hosts: Mutex::default(),
        }
    }

    /// True once `url`'s host has tripped the circuit breaker.
    pub fn is_tripped(&self, url: &str) -> bool {
        let Some(host) = host_of(url) else {
            return false;
        };
        let guard = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        guard.get(&host).is_some_and(|state| state.tripped)
    }

    /// Resets the consecutive-failure streak for `url`'s host after a successful download.
    pub fn record_success(&self, url: &str) {
        let Some(host) = host_of(url) else {
            return;
        };
        let mut guard = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(state) = guard.get_mut(&host) {
            state.consecutive_failures = 0;
            state.tripped = false;
        }
    }

    /// Records a failure for `url`'s host. Returns `Some(host)` on the exact call that trips it.
    pub fn record_failure(&self, url: &str) -> Option<String> {
        let host = host_of(url)?;
        let mut guard = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        let state = guard.entry(host.clone()).or_default();
        if state.tripped {
            return None;
        }
        state.consecutive_failures += 1;
        if state.consecutive_failures >= self.threshold {
            state.tripped = true;
            Some(host)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "host_circuit_tests.rs"]
mod tests;
