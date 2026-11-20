use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use extension_api::GatewayError;
use serde_json::Value;

use crate::reply_wait::{await_reply, DeadlineHold, PendingCall};
use crate::rpc_message::{
    encode_request, is_per_process_method, missing_sidecar_error, same_extension_load,
};
use crate::sidecar_process_kill::{discard_running, take_if_exited};
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

    /// Starts the sidecar if needed, then runs one JSON-lines exchange.
    fn run_call(
        &self,
        slot: &mut Option<RunningSidecar>,
        request: &CallRequest<'_>,
    ) -> Result<Value, GatewayError> {
        if take_if_exited(slot) {
            self.pid.store(0, Ordering::SeqCst);
        }
        if slot.is_none() {
            if let Some(err) = missing_sidecar_error(request.method) {
                return Err(err);
            }
            if self.closed.load(Ordering::SeqCst) {
                return Err(GatewayError::Unavailable("the app is closing".to_owned()));
            }
            let mut fresh = start_sidecar(&self.config)?;
            self.pid.store(fresh.pid, Ordering::SeqCst);
            self.replay_startup_calls(&mut fresh);
            *slot = Some(fresh);
        }
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let pending = PendingCall {
            id,
            method: request.method,
            params: request.params,
            timeout: request.timeout,
            hold: &self.hold,
        };
        match slot.as_mut() {
            Some(running) => exchange(running, &pending),
            None => Err(GatewayError::Unavailable("sidecar vanished".to_owned())),
        }
    }

    fn discard_slot(&self, slot: &mut Option<RunningSidecar>) {
        discard(slot);
        self.pid.store(0, Ordering::SeqCst);
    }

    /// After a Transport death: discard, restart once, and re-run. Timeouts stay unretried.
    fn retry_after_transport(
        &self,
        slot: &mut Option<RunningSidecar>,
        request: &CallRequest<'_>,
        error: GatewayError,
    ) -> Result<Value, GatewayError> {
        tracing::warn!(
            method = request.method,
            %error,
            "discarding sidecar after transport failure; retrying once"
        );
        self.discard_slot(slot);
        if self.closed.load(Ordering::SeqCst) {
            return Err(error);
        }
        let retry = self.run_call(slot, request);
        if matches!(
            retry,
            Err(GatewayError::Timeout { .. } | GatewayError::Transport(_))
        ) {
            tracing::warn!(
                method = request.method,
                "discarding sidecar process after failed retry"
            );
            self.discard_slot(slot);
        }
        retry
    }

    fn finish_call(
        &self,
        slot: &mut Option<RunningSidecar>,
        request: &CallRequest<'_>,
    ) -> Result<Value, GatewayError> {
        match self.run_call(slot, request) {
            Err(error) if matches!(error, GatewayError::Transport(_)) => {
                if is_per_process_method(request.method) {
                    tracing::warn!(
                        method = request.method,
                        %error,
                        "discarding sidecar after transport failure; not retrying per-process call"
                    );
                    self.discard_slot(slot);
                    Err(error)
                } else {
                    self.retry_after_transport(slot, request, error)
                }
            }
            Err(error) if matches!(error, GatewayError::Timeout { .. }) => {
                tracing::warn!(
                    method = request.method,
                    "discarding sidecar process after failed call"
                );
                self.discard_slot(slot);
                Err(error)
            }
            other => other,
        }
    }
}

/// Method, params, and deadline for one sidecar exchange.
struct CallRequest<'a> {
    method: &'a str,
    params: &'a Value,
    timeout: Duration,
}

impl RpcTransport for SidecarProcessTransport {
    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, GatewayError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(GatewayError::Unavailable("the app is closing".to_owned()));
        }
        let request = CallRequest {
            method,
            params: &params,
            timeout,
        };
        let mut slot = self.lock_running();
        self.finish_call(&mut slot, &request)
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let pid = self.pid.swap(0, Ordering::SeqCst);
        if pid != 0 {
            tracing::info!(
                pid,
                "closing: stopping the sidecar and everything it started"
            );
            crate::sidecar_process_kill::kill_process_tree(pid);
        } else {
            tracing::info!("closing: the sidecar was not running, nothing to stop");
        }
        // Block until any in-flight call releases the slot so the Child is always reaped.
        discard(&mut self.lock_running());
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
