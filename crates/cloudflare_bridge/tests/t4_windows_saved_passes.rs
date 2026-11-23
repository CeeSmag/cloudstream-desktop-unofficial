//! T4 (Windows, real WebView2): full saved-pass lifecycle (see `docs/CLOUDFLARE_PASS_PLAN.md`)
//! using the real `cloudflare_bridge::spawn_watcher`, real `DiskPassBook`, real
//! `cloudstream-verify.exe` (WebView2), and a local multi-subdomain HTTP fixture (`*.localhost`).
//!
//! Ignored in default `cargo test --workspace` because it needs the Windows WebView2 runtime and a
//! built `cloudstream-verify.exe`. Run with:
//!   `cargo build -p cloudflare_window`
//!   `cargo test -p cloudflare_bridge --test t4_windows_saved_passes -- --ignored --nocapture`
#![cfg(windows)]
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cloudflare_bridge::{
    spawn_watcher, AllowedHosts, HelperLaunch, SavedPasses, WatchConfig, WatchHandle,
};
use tracing_subscriber::fmt::MakeWriter;

const PATIENCE: Duration = Duration::from_secs(20);

/// Local HTTP fixture serving `*.localhost` challenges with monotonically increasing clearances.
struct ChallengeServer {
    port: u16,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ChallengeServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let dom_gen = Arc::new(AtomicUsize::new(0));
        let solo_gen = Arc::new(AtomicUsize::new(0));
        let join = thread::spawn(move || serve_loop(listener, stop_flag, dom_gen, solo_gen));
        Self {
            port,
            stop,
            join: Some(join),
        }
    }
}

impl Drop for ChallengeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn serve_loop(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    dom_gen: Arc<AtomicUsize>,
    solo_gen: Arc<AtomicUsize>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let (d, s) = (Arc::clone(&dom_gen), Arc::clone(&solo_gen));
                thread::spawn(move || handle_connection(stream, &d, &s));
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(mut stream: TcpStream, dom_gen: &AtomicUsize, solo_gen: &AtomicUsize) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0u8; 4096];
    let Ok(read) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..read]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let host = request
        .lines()
        .find_map(|line| {
            let (name, val) = line.split_once(':')?;
            name.eq_ignore_ascii_case("host")
                .then(|| val.trim().split(':').next().unwrap_or_default().to_owned())
        })
        .unwrap_or_default();
    let response = build_http_response(&host, target, dom_gen, solo_gen);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn build_http_response(
    host: &str,
    target: &str,
    dom_gen: &AtomicUsize,
    solo_gen: &AtomicUsize,
) -> String {
    if !target.contains("solved=1") {
        let body = r#"<title>Challenge</title><script>setTimeout(() => { location.search = "?solved=1"; }, 40);</script>"#;
        return format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/html\r\ncf-mitigated: challenge\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }
    let cookies = if host.ends_with("solo.localhost") {
        let n = solo_gen.fetch_add(1, Ordering::SeqCst) + 1;
        format!("Set-Cookie: cf_clearance=solo-v{n}; Path=/; Max-Age=3600; HttpOnly\r\n")
    } else {
        let n = dom_gen.fetch_add(1, Ordering::SeqCst) + 1;
        format!(
            "Set-Cookie: cf_clearance=dom-v{n}; Domain=sub.localhost; Path=/; Max-Age=3600; HttpOnly\r\n\
             Set-Cookie: sess_cookie=sess-secret-{n}; Domain=sub.localhost; Path=/\r\n\
             Set-Cookie: host_only=ho-secret-{n}; Path=/; Max-Age=3600\r\n"
        )
    };
    let body = "<title>Verified</title>";
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-store\r\n{cookies}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[derive(Clone, Default)]
struct LogSink(Arc<Mutex<Vec<u8>>>);

impl LogSink {
    fn snapshot(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("logs")).into_owned()
    }

    fn clear(&self) {
        self.0.lock().expect("logs").clear();
    }
}

struct SinkWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("logs").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogSink {
    type Writer = SinkWriter;
    fn make_writer(&'a self) -> Self::Writer {
        SinkWriter(Arc::clone(&self.0))
    }
}

fn find_verify_helper() -> PathBuf {
    if let Some(path) = std::env::var_os("CS_VERIFY_HELPER").map(PathBuf::from) {
        assert!(
            path.is_file(),
            "CS_VERIFY_HELPER not found: {}",
            path.display()
        );
        return path;
    }
    let exe = std::env::current_exe().expect("current exe");
    let candidate = exe
        .parent()
        .and_then(Path::parent)
        .expect("target profile dir")
        .join("cloudstream-verify.exe");
    assert!(
        candidate.is_file(),
        "cloudstream-verify.exe missing at {}; build it first with: cargo build -p cloudflare_window",
        candidate.display()
    );
    candidate
}

struct Rig {
    root: PathBuf,
    helper_exe: PathBuf,
    port: u16,
}

impl Rig {
    fn new(port: u16) -> Self {
        let root = std::env::temp_dir().join(format!("cs_t4_win_passes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("exchange")).expect("exchange");
        Self {
            root,
            helper_exe: find_verify_helper(),
            port,
        }
    }

    fn start_watcher(&self) -> WatchHandle {
        let exchange = self.root.join("exchange");
        let profile = self.root.join("profile");
        spawn_watcher(WatchConfig {
            dir: exchange.clone(),
            allowed: AllowedHosts::new(["sub.localhost".into(), "solo.localhost".into()]),
            helper: Some(HelperLaunch {
                program: self.helper_exe.clone(),
                args: vec![
                    "--exchange".into(),
                    exchange.to_string_lossy().into_owned(),
                    "--profile".into(),
                    profile.to_string_lossy().into_owned(),
                ],
            }),
            passes: Some(
                SavedPasses::on_disk(self.root.join("cloudflare_passes")).expect("passes"),
            ),
        })
    }

    fn ask(&self, id: &str, host_path: &str, extra: &str) {
        let (host, path) = host_path.split_once('/').expect("host/path");
        let dir = self.root.join("exchange");
        let text = format!(
            "id={id}\nhost={host}\nurl=http://{host}:{}/{path}\nmethod=GET\n{extra}",
            self.port
        );
        std::fs::write(dir.join(format!("request-{id}.tmp")), text).expect("write");
        std::fs::rename(
            dir.join(format!("request-{id}.tmp")),
            dir.join(format!("request-{id}.txt")),
        )
        .expect("publish");
    }

    fn answer(&self, id: &str) -> String {
        let path = self.root.join("exchange").join(format!("result-{id}.txt"));
        wait_for(|| std::fs::read_to_string(&path).ok())
            .unwrap_or_else(|| panic!("timed out waiting for result-{id}.txt"))
    }

    fn wait_pass_file(&self, name: &str) {
        let path = self.root.join("cloudflare_passes").join(name);
        wait_for(|| path.is_file().then_some(()))
            .unwrap_or_else(|| panic!("pass file {name} not created"));
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < PATIENCE {
        if let Some(found) = probe() {
            return Some(found);
        }
        thread::sleep(Duration::from_millis(25));
    }
    None
}

fn pass_id(answer: &str) -> String {
    answer
        .lines()
        .find_map(|line| line.strip_prefix("pass="))
        .unwrap_or_else(|| panic!("expected pass=<id> in saved answer:\n{answer}"))
        .to_owned()
}

fn verify_step2_first_checks(rig: &Rig, sink: &LogSink) {
    let first = rig.start_watcher();
    first.set_debug_logging(true);

    rig.ask("r1", "www.sub.localhost/watch", "");
    let r1 = rig.answer("r1");
    assert!(
        r1.contains("ok=true\n") && r1.contains("cookie=cf_clearance=dom-v1\n"),
        "{r1}"
    );
    assert!(r1.contains("validUntil="), "{r1}");
    rig.wait_pass_file("domain-sub.localhost.json");

    rig.ask("r2", "solo.localhost/watch", "");
    let r2 = rig.answer("r2");
    assert!(
        r2.contains("ok=true\n") && r2.contains("cookie=cf_clearance=solo-v1\n"),
        "{r2}"
    );
    rig.wait_pass_file("host-solo.localhost.json");
    wait_for(|| {
        sink.snapshot()
            .contains("scope=solo.localhost")
            .then_some(())
    })
    .expect("solo pass log line");

    let logs = sink.snapshot();
    assert!(logs.contains("kept the check's Cloudflare pass"), "{logs}");
    assert!(logs.contains("scope=.sub.localhost"), "{logs}");
    assert!(logs.contains("scope=solo.localhost"), "{logs}");
    for secret in ["dom-v1", "sess-secret-1", "ho-secret-1", "solo-v1"] {
        assert!(
            !logs.contains(secret),
            "cookie value {secret} leaked to logs:\n{logs}"
        );
    }
    drop(first);
}

fn verify_step3_restart_and_sibling(rig: &Rig, sink: &LogSink) -> (WatchHandle, String) {
    sink.clear();
    let second = rig.start_watcher();
    second.set_debug_logging(true);

    rig.ask("r3", "www.sub.localhost/ep2", "");
    let r3 = rig.answer("r3");
    assert!(
        r3.contains("source=saved\n") && r3.contains("validUntil="),
        "{r3}"
    );
    assert!(r3.contains("cookie=cf_clearance=dom-v1\n"), "{r3}");
    assert!(
        !r3.contains("sess_cookie="),
        "session cookie must drop on restart:\n{r3}"
    );

    rig.ask("r4", "api.sub.localhost/feed", "");
    let r4 = rig.answer("r4");
    assert!(
        r4.contains("source=saved\n") && r4.contains("cookie=cf_clearance=dom-v1\n"),
        "{r4}"
    );
    assert!(
        !r4.contains("ho-secret-1"),
        "www host-only cookie must not leak to api:\n{r4}"
    );
    assert!(!r4.contains("sess_cookie="), "{r4}");

    let logs = sink.snapshot();
    assert!(
        logs.contains("answered from a saved Cloudflare pass"),
        "{logs}"
    );
    assert!(
        !logs.contains("verification helper started"),
        "helper must not start:\n{logs}"
    );
    let old_id = pass_id(&r3);
    (second, old_id)
}

fn verify_step4_refusal_and_replacement(rig: &Rig, sink: &LogSink, old_id: &str) {
    sink.clear();
    rig.ask(
        "r5",
        "www.sub.localhost/ep3",
        &format!("rejected={old_id}\n"),
    );
    let r5 = rig.answer("r5");
    assert!(
        r5.contains("ok=true\n") && r5.contains("cookie=cf_clearance=dom-v2\n"),
        "{r5}"
    );
    assert!(
        !r5.contains("source=saved\n") && !r5.contains("transport=browser\n"),
        "{r5}"
    );
    wait_for(|| {
        sink.snapshot()
            .contains("kept the check's Cloudflare pass")
            .then_some(())
    })
    .expect("replacement pass kept");
    let logs = sink.snapshot();
    assert!(
        logs.contains("is retired and the site is checked again") && !logs.contains("dom-v2"),
        "{logs}"
    );
    rig.ask("r6", "api.sub.localhost/feed2", "");
    let r6 = rig.answer("r6");
    assert!(
        r6.contains("source=saved\n") && r6.contains("cookie=cf_clearance=dom-v2\n"),
        "{r6}"
    );
    assert_ne!(
        pass_id(&r6),
        old_id,
        "retired pass id must never be served again"
    );
}

#[test]
#[ignore = "needs Windows WebView2 runtime and built cloudstream-verify.exe"]
fn windows_webview2_saved_passes_keep_share_restart_and_replace_end_to_end() {
    let sink = LogSink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(sink.clone())
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("global log subscriber");

    let server = ChallengeServer::start();
    let rig = Rig::new(server.port);
    verify_step2_first_checks(&rig, &sink);
    let (second, old_id) = verify_step3_restart_and_sibling(&rig, &sink);
    verify_step4_refusal_and_replacement(&rig, &sink, &old_id);
    drop(second);
}
