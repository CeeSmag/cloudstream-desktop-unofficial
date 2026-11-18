//! Tests that look for the ways this app can freeze.
//!
//! 1. A scan of every Tauri command: a *synchronous* command runs on the thread that draws the window, so
//!    if it calls something that waits for that same thread (reading the browser's cookies, building a
//!    window or view, running a script and waiting for its answer) or simply blocks (`recv`, `join`,
//!    `sleep`, `block_on`), the whole app freezes. That is how `report_image` once froze the app.
//! 2. A watchdog (`finishes_within`) that fails a test when a piece of code does not come back in time,
//!    used on the shared state the commands touch.
//!
//! The scan only sees the command's own body, not the functions it calls; keep commands thin.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Calls that wait for the window's own thread, or block the thread they run on.
const FREEZERS: &[&str] = &[
    "cookies_for_url",
    ".cookies()",
    "WebviewWindowBuilder",
    "WebviewBuilder",
    "add_child",
    "create_webview",
    "eval_with_callback",
    ".recv(",
    ".recv_timeout(",
    ".join()",
    "thread::sleep",
    "block_on",
];

/// A synchronous Tauri command and the freezing calls in its body.
#[derive(Debug, PartialEq, Eq)]
struct Finding {
    command: String,
    call: &'static str,
}

/// The body of the item whose header starts at `start` (from its first `{` to the matching `}`).
fn body_from(source: &str, start: usize) -> &str {
    let rest = source.get(start..).unwrap_or_default();
    let Some(open) = rest.find('{') else {
        return "";
    };
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return rest.get(open..=index).unwrap_or_default();
                }
            }
            _ => {}
        }
    }
    rest.get(open..).unwrap_or_default()
}

/// Every synchronous command in `source` that makes a freezing call.
fn freezing_commands(source: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    for (at, _) in source.match_indices("#[tauri::command]") {
        let after = source.get(at..).unwrap_or_default();
        let header_end = after.find('{').unwrap_or(after.len());
        let header = after.get(..header_end).unwrap_or_default();
        if header.contains("async fn") {
            continue;
        }
        let name = header
            .split("fn ")
            .nth(1)
            .and_then(|rest| rest.split('(').next())
            .unwrap_or("?")
            .trim()
            .to_owned();
        let body = body_from(source, at);
        for call in FREEZERS {
            if body.contains(call) {
                found.push(Finding {
                    command: name.clone(),
                    call,
                });
            }
        }
    }
    found
}

/// Runs `work` on its own thread and says whether it came back within `limit`.
pub fn finishes_within<F: FnOnce() + Send + 'static>(limit: Duration, work: F) -> bool {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        work();
        let _ = done.send(());
    });
    finished.recv_timeout(limit).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const FROZEN_COMMAND: &str = r#"
#[tauri::command]
pub fn report_image(app: AppHandle) {
    let cookies = app.get_webview_window("main").map(|v| v.cookies_for_url(url));
}
"#;
    const ASYNC_COMMAND: &str = r#"
#[tauri::command]
pub async fn report_image(app: AppHandle) -> Result<(), String> {
    let cookies = app.get_webview_window("main").map(|v| v.cookies_for_url(url));
    Ok(())
}
"#;

    #[test]
    fn a_sync_command_reading_cookies_is_found() {
        assert_eq!(
            freezing_commands(FROZEN_COMMAND),
            vec![Finding {
                command: "report_image".to_owned(),
                call: "cookies_for_url"
            }]
        );
    }

    #[test]
    fn the_same_work_in_an_async_command_is_fine() {
        assert!(freezing_commands(ASYNC_COMMAND).is_empty());
    }

    #[test]
    fn only_the_command_body_is_judged() {
        let source = "fn helper() { x.recv(); }\n#[tauri::command]\npub fn quick() { 1 + 1; }\n";
        assert!(freezing_commands(source).is_empty());
    }

    #[test]
    fn no_command_of_the_app_can_freeze_the_window() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut problems = Vec::new();
        let mut checked = 0;
        for entry in std::fs::read_dir(folder).expect("src folder") {
            let path = entry.expect("entry").path();
            let is_source = path.extension().is_some_and(|ext| ext == "rs");
            // This file holds sample commands as text; it is not judged.
            if !is_source || path.ends_with("deadlock_tests.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("readable source");
            checked += source.matches("#[tauri::command]").count();
            for finding in freezing_commands(&source) {
                problems.push(format!(
                    "{}: sync command `{}` calls `{}`; make it `async` or move the work to a thread",
                    path.display(),
                    finding.command,
                    finding.call
                ));
            }
        }
        assert!(checked > 30, "the scan found only {checked} commands");
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn the_watchdog_notices_code_that_never_comes_back() {
        // Two threads take two locks in opposite order: a real deadlock.
        let (first, second) = (Arc::new(Mutex::new(())), Arc::new(Mutex::new(())));
        let (a, b) = (Arc::clone(&first), Arc::clone(&second));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let barrier_b = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let _held = b.lock();
            barrier_b.wait();
            let _other = a.lock();
        });
        let finished = finishes_within(Duration::from_millis(300), move || {
            let _held = first.lock();
            barrier.wait();
            let _other = second.lock();
        });
        assert!(
            !finished,
            "a real deadlock must be reported as not finished"
        );
        assert!(finishes_within(Duration::from_secs(5), || {}));
    }

    #[test]
    fn the_gate_does_not_deadlock_under_many_threads() {
        let gate = Arc::new(crate::cloudflare_gate::Gate::default());
        let finished = finishes_within(Duration::from_secs(10), move || {
            let workers: Vec<_> = (0..8)
                .map(|n| {
                    let gate = Arc::clone(&gate);
                    std::thread::spawn(move || {
                        for i in 0..500 {
                            gate.allow(&format!("site-{n}-{i}.test"));
                            let _ = gate.is_allowed("site-0-0.test");
                        }
                    })
                })
                .collect();
            for worker in workers {
                let _ = worker.join();
            }
        });
        assert!(finished, "the gate stopped answering");
    }
}
