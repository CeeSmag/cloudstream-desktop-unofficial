//! Test double for the JVM sidecar: speaks the same JSON-lines protocol with scripted behavior.
//! Methods: `ping`, `echo` (returns params), `fail` (error reply), `noise` (junk line then reply),
//! `hang` (never replies), `crash` (exits), `handled` (how many requests this process has seen),
//! and the streaming link search (`start_link_search` with data `"<count>,<gap ms>,<tail ms>"`,
//! `poll_link_search`, `cancel_link_search`, `search_stats`).
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

use extension_runtime::fake_link_search::{FakeLinkSearches, SearchScript};
use serde_json::{json, Value};

/// Reads a top-level field, yielding `null` when absent.
fn field(request: &Value, name: &str) -> Value {
    request.get(name).cloned().unwrap_or(Value::Null)
}

fn reply_for(method: &str, id: &Value, params: &Value, handled: u64) -> Option<Value> {
    match method {
        "handled" => Some(json!({"id": id, "result": handled})),
        "ping" => Some(json!({"id": id, "result": "pong"})),
        "set_debug_logging" => Some(json!({"id": id, "result": params.get("enabled")})),
        "echo" => Some(json!({"id": id, "result": params})),
        "fail" => Some(json!({"id": id, "error": {"type": "TestFailure", "message": "scripted"}})),
        _ => None,
    }
}

/// The streaming link search methods, answered like the real sidecar answers them.
fn search_reply(
    method: &str,
    id: &Value,
    params: &Value,
    searches: &FakeLinkSearches,
) -> Option<Value> {
    let text = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let number = |name: &str| params.get(name).and_then(Value::as_u64).unwrap_or_default();
    match method {
        "start_link_search" => Some(json!({"id": id, "result": searches.start(&text("data"))})),
        "poll_link_search" => {
            let polled = searches.poll(
                &text("id"),
                core_model::LinkSearchCursor {
                    links: usize::try_from(number("fromLink")).unwrap_or(0),
                    subtitles: usize::try_from(number("fromSubtitle")).unwrap_or(0),
                },
                std::time::Duration::from_millis(number("waitMs")),
            );
            Some(match polled {
                Ok(update) => json!({"id": id, "result": update}),
                Err(message) => {
                    json!({"id": id, "error": {"type": "IllegalStateException", "message": message}})
                }
            })
        }
        "cancel_link_search" => {
            searches.cancel(&text("id"));
            Some(json!({"id": id, "result": null}))
        }
        "search_stats" => {
            let stats = searches.stats();
            Some(json!({"id": id, "result": {
                "started": stats.started, "cancelled": stats.cancelled, "active": stats.active
            }}))
        }
        _ => None,
    }
}

fn main() {
    let searches = FakeLinkSearches::new(SearchScript::from_data);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut handled: u64 = 0;
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = field(&request, "method")
            .as_str()
            .unwrap_or_default()
            .to_owned();
        handled += 1;
        let mut out = stdout.lock();
        match method.as_str() {
            "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
            "crash" => std::process::exit(3),
            "noise" => {
                let _ = writeln!(out, "[main] INFO this is not json");
                let _ = writeln!(
                    out,
                    "{}",
                    json!({"id": field(&request, "id"), "result": "after-noise"})
                );
            }
            other => {
                let (id, params) = (field(&request, "id"), field(&request, "params"));
                let reply = search_reply(other, &id, &params, &searches)
                    .or_else(|| reply_for(other, &id, &params, handled));
                if let Some(reply) = reply {
                    let _ = writeln!(out, "{reply}");
                }
            }
        }
        let _ = out.flush();
    }
}
