//! Cross-platform scripted verification helper binary for `cloudflare_bridge` T2 tests.
//!
//! Built only when the `test-support` feature is enabled (via `[dev-dependencies]`), never in
//! production builds. Replaces POSIX `sh -c` scripts so T2 watcher, saved-pass, and race tests run
//! identically on Windows and Linux.
#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Args {
    mode: String,
    exchange: PathBuf,
    extra: Vec<String>,
}

fn parse_args(mut raw: impl Iterator<Item = String>) -> Option<Args> {
    let (mut mode, mut exchange, mut extra) = (None, None, Vec::new());
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--mode" => mode = raw.next(),
            "--exchange" => exchange = raw.next().map(PathBuf::from),
            _ => extra.push(arg),
        }
    }
    Some(Args {
        mode: mode?,
        exchange: exchange?,
        extra,
    })
}

fn atomic_write(dir: &Path, name: &str, content: &str) -> io::Result<()> {
    let tmp = dir.join(format!("{name}.tmp"));
    let dest = dir.join(name);
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &dest)
}

fn send_line(line: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
}

fn pass_payload(domain: &str) -> String {
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis())
        .saturating_add(3_600_000);
    let clearance = format!(
        r#"{{"name":"cf_clearance","value":"clear-value","domain":"{domain}","path":"/","secure":true,"http_only":true,"expires_ms":{expires}}}"#
    );
    let session = format!(
        r#"{{"name":"sess","value":"session-value","domain":"{domain}","path":"/","secure":true,"http_only":true}}"#
    );
    format!(r#"{{"user_agent":"Fake/1","cookies":[{clearance},{session}]}}"#)
}

fn run_saved_passes(dir: &Path) -> ExitCode {
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let fields: Vec<&str> = line.split('\t').collect();
        let kind = fields.first().copied().unwrap_or_default();
        if kind == "quit" {
            break;
        }
        if matches!(kind, "verify" | "present") {
            let id = fields.get(1).copied().unwrap_or_default();
            let host = fields.get(2).copied().unwrap_or_default();
            let _ = fs::write(dir.join(format!("{kind}-{id}.received")), "");
            let body = "ok=true\nuserAgent=Fake/1\ncookie=cf_clearance=clear-value\n";
            let _ = atomic_write(dir, &format!("result-{id}.txt"), body);
            let domain = if host.ends_with("solo.test") {
                "www.solo.test"
            } else {
                ".site.test"
            };
            send_line(&format!("passed\t{host}\t{id}\t{}\n", pass_payload(domain)));
        }
    }
    ExitCode::SUCCESS
}

fn publish_watch_result(dir: &Path, id: &str) {
    let body = "ok=true\nuserAgent=Fake\ncookie=cf_clearance=x\n";
    let _ = atomic_write(dir, &format!("result-{id}.txt"), body);
}

fn finish_watch(dir: &Path, host: &str, id: &str) {
    publish_watch_result(dir, id);
    send_line(&format!("passed\t{host}\t{id}\n"));
}

fn handle_watch_verify(dir: &Path, host: &str, id: &str) -> Option<ExitCode> {
    send_line(&format!("open\t{host}\t{id}\n"));
    match id {
        "failed-first" => send_line(&format!("failed\t{host}\t{id}\tfixture failure\n")),
        "first" => {}
        other if other.starts_with("app-retry-") || other.starts_with("hold-") => {}
        "second" => {
            finish_watch(dir, host, id);
            finish_watch(dir, host, "first");
        }
        "committed" => {
            publish_watch_result(dir, id);
            return Some(ExitCode::FAILURE);
        }
        _ => finish_watch(dir, host, id),
    }
    None
}

fn run_watch(dir: &Path) -> ExitCode {
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let fields: Vec<&str> = line.split('\t').collect();
        let kind = fields.first().copied().unwrap_or_default();
        let id = fields.get(1).copied().unwrap_or_default();
        let host = fields.get(2).copied().unwrap_or_default();
        let url = fields.get(3).copied().unwrap_or_default();
        match kind {
            "quit" => break,
            "activate" | "show" => {}
            "cancel" => {
                let _ = fs::write(dir.join(format!("cancel-{id}.received")), "\n");
                send_line(&format!("failed\tsite.test\t{id}\tcancelled\n"));
            }
            "verify" | "present" => {
                let _ = fs::write(
                    dir.join(format!("{kind}-{id}.received")),
                    format!("{url}\n"),
                );
                if let Some(code) = handle_watch_verify(dir, host, id) {
                    return code;
                }
            }
            _ => {}
        }
    }
    ExitCode::SUCCESS
}

fn run_record_args(dir: &Path, extra: &[String]) -> ExitCode {
    let _ = atomic_write(dir, "args.txt", &format!("{}\n", extra.join(" ")));
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.split('\t').next() == Some("quit") {
            break;
        }
    }
    ExitCode::SUCCESS
}

fn spawn_beater(dir: PathBuf, host: String, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            send_line(&format!("fetched\t{host}\tf-1\n"));
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("beats.txt"))
            {
                let _ = file.write_all(b"beat\n");
            }
            thread::sleep(Duration::from_millis(200));
        }
    })
}

fn run_fetch_beats(dir: &Path) -> ExitCode {
    let stop = Arc::new(AtomicBool::new(false));
    let mut beaters = Vec::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let fields: Vec<&str> = line.split('\t').collect();
        let kind = fields.first().copied().unwrap_or_default();
        if kind == "quit" {
            break;
        }
        if matches!(kind, "verify" | "present") {
            let id = fields.get(1).copied().unwrap_or_default();
            let host = fields.get(2).copied().unwrap_or_default();
            finish_watch(dir, host, id);
            beaters.push(spawn_beater(
                dir.to_path_buf(),
                host.to_owned(),
                Arc::clone(&stop),
            ));
        }
    }
    stop.store(true, Ordering::Relaxed);
    for beater in beaters {
        let _ = beater.join();
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let Some(args) = parse_args(std::env::args().skip(1)) else {
        return ExitCode::FAILURE;
    };
    match args.mode.as_str() {
        "saved-passes" => run_saved_passes(&args.exchange),
        "watch" => run_watch(&args.exchange),
        "record-args" => run_record_args(&args.exchange, &args.extra),
        "fetch-beats" => run_fetch_beats(&args.exchange),
        _ => ExitCode::FAILURE,
    }
}
