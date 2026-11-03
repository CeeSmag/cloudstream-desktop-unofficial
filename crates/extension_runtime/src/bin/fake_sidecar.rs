//! Test double for the JVM sidecar: speaks the same JSON-lines protocol with scripted behavior.
//! Methods: `ping`, `echo` (returns params), `fail` (error reply), `noise` (junk line then reply),
//! `hang` (never replies), `crash` (exits), `handled` (how many requests this process has seen).
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

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

fn main() {
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
                if let Some(reply) = reply_for(
                    other,
                    &field(&request, "id"),
                    &field(&request, "params"),
                    handled,
                ) {
                    let _ = writeln!(out, "{reply}");
                }
            }
        }
        let _ = out.flush();
    }
}
