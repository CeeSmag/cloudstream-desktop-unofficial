use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use extension_api::GatewayError;
use serde_json::Value;

use crate::reply_wait::{await_reply, DeadlineHold, PendingCall};
use crate::rpc_message::encode_request;
use crate::sidecar_process_kill::discard_running;
use crate::sidecar_process_start::{start_sidecar, RunningSidecar};
use crate::{RpcTransport, SidecarLaunchConfig};

/// `RpcTransport` that talks JSON lines to a child process.
///
/// The child is started on first use, killed on timeout or transport failure,
/// and started again on the next call, so one bad call never wedges the app.
pub struct SidecarProcessTransport {
    config: SidecarLaunchConfig,
    running: Mutex<Option<RunningSidecar>>,
    next_request_id: AtomicU64,
    /// Keeps calls in flight from timing out while a verification window is open.
    hold: DeadlineHold,
    /// Calls repeated on every fresh start, for example loading the installed extensions.
    startup_calls: Mutex<Vec<StartupCall>>,
    /// Process id of the running sidecar (0 when none), readable without the lock a call holds.
    pid: AtomicU32,
    /// Set when the app is closing: no sidecar is started any more.
    closed: AtomicBool,
}

/// A call to repeat whenever a fresh sidecar is started.
struct StartupCall {
    method: String,
    params: Value,
    timeout: Duration,
}

impl SidecarProcessTransport {
    /// Creates a transport; nothing is started until the first call.
    pub fn new(config: SidecarLaunchConfig) -> Self {
        Self {
            config,
            running: Mutex::new(None),
            next_request_id: AtomicU64::new(1),
            hold: DeadlineHold::default(),
            startup_calls: Mutex::new(Vec::new()),
            pid: AtomicU32::new(0),
            closed: AtomicBool::new(false),
        }
    }

    /// Repeats the remembered startup calls on a sidecar that has just been started again.
    #[tracing::instrument(level = "debug", skip_all)]
    fn replay_startup_calls(&self, running: &mut RunningSidecar) {
        let calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for call in calls.iter() {
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let pending = PendingCall {
                id,
                method: &call.method,
                params: &call.params,
                timeout: call.timeout,
                hold: &self.hold,
            };
            match exchange(running, &pending) {
                Ok(_) => {
                    tracing::info!(method = %call.method, "repeated a startup call on the new sidecar")
                }
                Err(error) => {
                    tracing::warn!(method = %call.method, %error, "a startup call failed on the new sidecar")
                }
            }
        }
    }

    fn lock_running(&self) -> MutexGuard<'_, Option<RunningSidecar>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl RpcTransport for SidecarProcessTransport {
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(GatewayError::Unavailable("the app is closing".to_owned()));
        }
        let mut slot = self.lock_running();
        if slot.is_none() {
            let mut fresh = start_sidecar(&self.config)?;
            self.pid.store(fresh.pid, Ordering::SeqCst);
            self.replay_startup_calls(&mut fresh);
            *slot = Some(fresh);
        }
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let pending = PendingCall {
            id,
            method,
            params: &params,
            timeout,
            hold: &self.hold,
        };
        let outcome = match slot.as_mut() {
            Some(running) => exchange(running, &pending),
            None => Err(GatewayError::Unavailable("sidecar vanished".to_owned())),
        };
        if matches!(
            outcome,
            Err(GatewayError::Timeout { .. } | GatewayError::Transport(_))
        ) {
            tracing::warn!(method, "discarding sidecar process after failed call");
            discard(&mut slot);
            self.pid.store(0, Ordering::SeqCst);
        }
        outcome
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let pid = self.pid.swap(0, Ordering::SeqCst);
        if pid == 0 {
            tracing::info!("closing: the sidecar was not running, nothing to stop");
            return;
        }
        tracing::info!(
            pid,
            "closing: stopping the sidecar and everything it started"
        );
        crate::sidecar_process_kill::kill_process_tree(pid);
        match self.running.try_lock() {
            Ok(mut slot) => discard(&mut slot),
            Err(_) => tracing::warn!(
                pid,
                "closing: a call was still running; the sidecar was ended by its process id"
            ),
        }
    }

    fn extend_call_deadline(&self, extra: Duration) {
        self.hold.extend(extra);
    }

    fn remember_setting_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let mut calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        calls.retain(|call| call.method != method);
        calls.push(StartupCall {
            method: method.to_owned(),
            params,
            timeout,
        });
    }

    fn remember_for_restart(&self, method: &str, params: Value, timeout: Duration) {
        let mut calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !calls
            .iter()
            .any(|call| call.method == method && call.params == params)
        {
            calls.push(StartupCall {
                method: method.to_owned(),
                params,
                timeout,
            });
        }
    }

    fn forget_for_restart(&self, method: &str, params: &Value) {
        let mut calls = self
            .startup_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        calls.retain(|call| !(call.method == method && same_extension_load(&call.params, params)));
    }
}

fn same_extension_load(left: &Value, right: &Value) -> bool {
    let path = |value: &Value| {
        value
            .get("path")
            .and_then(Value::as_str)
            .map(|text| text.replace('\\', "/").to_ascii_lowercase())
    };
    match (path(left), path(right)) {
        (Some(a), Some(b)) => a == b,
        _ => left == right,
    }
}

impl Drop for SidecarProcessTransport {
    fn drop(&mut self) {
        discard(&mut self.lock_running());
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn discard(slot: &mut Option<RunningSidecar>) {
    if let Some(mut running) = slot.take() {
        discard_running(&mut running);
    }
}

fn exchange(running: &mut RunningSidecar, call: &PendingCall<'_>) -> Result<Value, GatewayError> {
    let line = encode_request(call.id, call.method, call.params);
    writeln!(running.stdin, "{line}")
        .and_then(|()| running.stdin.flush())
        .map_err(|source| GatewayError::Transport(format!("cannot write request: {source}")))?;
    await_reply(&running.replies, call)
}
