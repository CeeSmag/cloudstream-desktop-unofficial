//! Waiting for the reply to one call, and the "hold" that lets a call outlive its timeout while the
//! viewer is busy with something the call is waiting for (a Cloudflare verification window).

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use extension_api::GatewayError;
use serde_json::Value;

use crate::rpc_message::parse_reply;

/// A time before which calls in flight are not given up on, on top of their own timeout.
///
/// Whoever is working for the call (the verification window) keeps pushing it a little way ahead
/// while it works; when it stops, the call's own timeout applies again within seconds.
#[derive(Debug, Default)]
pub struct DeadlineHold {
    until: Mutex<Option<Instant>>,
}

impl DeadlineHold {
    /// Lets calls in flight run until at least `extra` from now.
    pub fn extend(&self, extra: Duration) {
        let candidate = Instant::now() + extra;
        let mut until = self.until.lock().unwrap_or_else(PoisonError::into_inner);
        if until.is_none_or(|current| candidate > current) {
            *until = Some(candidate);
        }
    }

    fn latest(&self) -> Option<Instant> {
        *self.until.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One request in flight.
pub(crate) struct PendingCall<'a> {
    pub id: u64,
    pub method: &'a str,
    pub params: &'a Value,
    pub timeout: Duration,
    pub hold: &'a DeadlineHold,
}

/// Waits for the reply with `call.id`; replies to other calls are stale and skipped.
pub(crate) fn await_reply(
    replies: &Receiver<String>,
    call: &PendingCall<'_>,
) -> Result<Value, GatewayError> {
    let own_deadline = Instant::now() + call.timeout;
    loop {
        let deadline = call
            .hold
            .latest()
            .map_or(own_deadline, |held| held.max(own_deadline));
        match replies.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => match parse_reply(&line) {
                Ok(reply) if reply.id == Some(call.id) => return reply.into_value(),
                Ok(stale) => {
                    tracing::warn!(expected = call.id, got = ?stale.id, "ignoring stale reply")
                }
                Err(error) => tracing::warn!(%error, "ignoring unreadable sidecar line"),
            },
            // The hold may have been pushed ahead while this waited: look again before giving up.
            Err(RecvTimeoutError::Timeout)
                if Instant::now() < held_deadline(call, own_deadline) => {}
            Err(RecvTimeoutError::Timeout) => {
                return Err(GatewayError::Timeout {
                    method: call.method.to_owned(),
                    seconds: call.timeout.as_secs(),
                })
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(GatewayError::Transport(
                    "sidecar exited unexpectedly".to_owned(),
                ))
            }
        }
    }
}

fn held_deadline(call: &PendingCall<'_>, own_deadline: Instant) -> Instant {
    call.hold
        .latest()
        .map_or(own_deadline, |held| held.max(own_deadline))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    fn call<'a>(hold: &'a DeadlineHold, params: &'a Value, timeout_ms: u64) -> PendingCall<'a> {
        PendingCall {
            id: 7,
            method: "main_page",
            params,
            timeout: Duration::from_millis(timeout_ms),
            hold,
        }
    }

    fn reply_line(id: u64) -> String {
        json!({"id": id, "result": "done"}).to_string()
    }

    #[test]
    fn the_reply_to_the_call_is_returned_and_other_replies_are_skipped() {
        let (sender, replies) = mpsc::channel();
        sender.send(reply_line(3)).expect("stale");
        sender.send("not json".to_owned()).expect("garbage");
        sender.send(reply_line(7)).expect("reply");
        let (hold, params) = (DeadlineHold::default(), json!({}));
        let value = await_reply(&replies, &call(&hold, &params, 500)).expect("reply");
        assert_eq!(value, json!("done"));
    }

    #[test]
    fn without_a_reply_the_call_times_out() {
        let (_sender, replies) = mpsc::channel::<String>();
        let (hold, params) = (DeadlineHold::default(), json!({}));
        let started = Instant::now();
        let error = await_reply(&replies, &call(&hold, &params, 80)).expect_err("timeout");
        assert!(matches!(error, GatewayError::Timeout { .. }), "{error:?}");
        assert!(started.elapsed() >= Duration::from_millis(80));
    }

    #[test]
    fn a_closed_channel_means_the_sidecar_died() {
        let (sender, replies) = mpsc::channel::<String>();
        drop(sender);
        let (hold, params) = (DeadlineHold::default(), json!({}));
        let error = await_reply(&replies, &call(&hold, &params, 500)).expect_err("gone");
        assert!(matches!(error, GatewayError::Transport(_)), "{error:?}");
    }

    /// The hold is pushed ahead while the call waits (a verification window is open); the reply comes
    /// long after the call's own timeout and must still be received. Run many times so every
    /// interleaving of "extend" and "time runs out" is met.
    #[test]
    fn a_held_call_outlives_its_timeout_and_gets_its_reply() {
        for _ in 0..20 {
            let hold = Arc::new(DeadlineHold::default());
            let (sender, replies) = mpsc::channel();
            let helper_hold = Arc::clone(&hold);
            let helper = std::thread::spawn(move || {
                for _ in 0..8 {
                    helper_hold.extend(Duration::from_millis(60));
                    std::thread::sleep(Duration::from_millis(20));
                }
                sender.send(reply_line(7)).expect("reply");
            });
            let params = json!({});
            let value = await_reply(&replies, &call(&hold, &params, 30)).expect("held call");
            assert_eq!(value, json!("done"));
            helper.join().expect("helper");
        }
    }

    #[test]
    fn when_the_hold_stops_the_call_times_out_soon_after() {
        let hold = DeadlineHold::default();
        hold.extend(Duration::from_millis(100));
        let (_sender, replies) = mpsc::channel::<String>();
        let params = json!({});
        let started = Instant::now();
        let error = await_reply(&replies, &call(&hold, &params, 10)).expect_err("timeout");
        assert!(matches!(error, GatewayError::Timeout { .. }));
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(100) && waited < Duration::from_millis(600),
            "{waited:?}"
        );
    }

    #[test]
    fn an_earlier_extension_never_shortens_a_later_one() {
        let hold = DeadlineHold::default();
        hold.extend(Duration::from_millis(500));
        let later = hold.latest().expect("set");
        hold.extend(Duration::from_millis(10));
        assert_eq!(hold.latest(), Some(later));
    }
}
