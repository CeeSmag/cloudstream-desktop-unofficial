//! Test support: a local web server standing in for GitHub, and a release signed with a key made
//! for the test. It serves the release list, the signed checksum list and the files, and remembers
//! every path asked for.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use minisign::KeyPair;
use sha2::{Digest, Sha256};

use crate::update_verify::to_hex;

/// The version the test release is tagged with: always newer than the running one.
pub(crate) const RELEASE_TAG: &str = "v99.0.0";

/// Every file name a release carries; the updater picks one by the system it runs on.
pub(crate) fn release_file_names() -> [String; 3] {
    [
        format!("CloudStream-99.0.0-{}.AppImage", std::env::consts::ARCH),
        "CloudStream_99.0.0_x64-setup.exe".to_owned(),
        "CloudStream_99.0.0_x64_portable.exe".to_owned(),
    ]
}

/// The bytes a release file holds (each file different, so a mix-up shows).
pub(crate) fn file_bytes(name: &str) -> Vec<u8> {
    let mut bytes = format!("release file {name}\n").into_bytes();
    bytes.resize(200 * 1024, b'u');
    bytes
}

type Routes = HashMap<String, (u16, Vec<u8>)>;

/// The stand-in for GitHub; stops when dropped.
pub(crate) struct ReleaseServer {
    base: String,
    routes: Arc<Mutex<Routes>>,
    asked: Arc<Mutex<Vec<String>>>,
    agents: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReleaseServer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("non blocking");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let routes = Arc::new(Mutex::new(Routes::new()));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let agents = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let log = Log {
            paths: Arc::clone(&asked),
            agents: Arc::clone(&agents),
        };
        let (served, halt) = (Arc::clone(&routes), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => answer(stream, &served, &log),
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self {
            base,
            routes,
            asked,
            agents,
            stop,
            thread: Some(thread),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Answers `path` with `status` and `body`.
    pub(crate) fn serve(&self, path: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.routes
            .lock()
            .expect("routes")
            .insert(path.to_owned(), (status, body.into()));
    }

    /// Every path asked for, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("log").clone()
    }

    /// The `User-Agent` of every request, in order (empty when one had none).
    pub(crate) fn user_agents(&self) -> Vec<String> {
        self.agents.lock().expect("log").clone()
    }

    /// Whether any release file (not the list or the checksums) was downloaded.
    pub(crate) fn downloaded_a_release_file(&self) -> bool {
        let names = release_file_names();
        self.asked()
            .iter()
            .any(|path| names.iter().any(|name| path.ends_with(name.as_str())))
    }
}

impl Drop for ReleaseServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the server writes down about each request.
struct Log {
    paths: Arc<Mutex<Vec<String>>>,
    agents: Arc<Mutex<Vec<String>>>,
}

fn answer(stream: TcpStream, routes: &Mutex<Routes>, log: &Log) {
    let _ = stream.set_nonblocking(false);
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let mut line = String::new();
    let mut agent = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("user-agent") {
                value.trim().clone_into(&mut agent);
            }
        }
        line.clear();
    }
    log.agents.lock().expect("log").push(agent);
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    log.paths.lock().expect("log").push(path.clone());
    let (status, body) = routes
        .lock()
        .expect("routes")
        .get(&path)
        .cloned()
        .unwrap_or((404, b"not found".to_vec()));
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let stream = reader.get_mut();
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&body));
}

/// A key pair made for one test, and its public key file's text.
pub(crate) fn test_key() -> (KeyPair, String) {
    let keys = KeyPair::generate_unencrypted_keypair().expect("keys");
    let public_key = keys.pk.to_box().expect("public key").to_string();
    (keys, public_key)
}

/// How the test release is put together.
pub(crate) struct ReleasePlan<'a> {
    /// The key the checksum list is signed with.
    pub(crate) keys: &'a KeyPair,
    /// Whether the release carries `SHA256SUMS` and its signature at all.
    pub(crate) signed: bool,
}

/// Serves a published release tagged [`RELEASE_TAG`] with every file of [`release_file_names`],
/// their checksum list signed as `plan` says, and the release list at `/releases`.
pub(crate) fn publish(server: &ReleaseServer, plan: &ReleasePlan<'_>) {
    let mut assets = Vec::new();
    let mut sums = String::new();
    for name in release_file_names() {
        let bytes = file_bytes(&name);
        sums.push_str(&format!("{}  {name}\n", to_hex(&Sha256::digest(&bytes))));
        assets.push(asset_json(server, &name, bytes.len()));
        server.serve(&format!("/files/{name}"), 200, bytes);
    }
    if plan.signed {
        let signature = minisign::sign(
            Some(&plan.keys.pk),
            &plan.keys.sk,
            sums.as_bytes(),
            None,
            None,
        )
        .expect("signs")
        .to_string();
        assets.push(asset_json(server, "SHA256SUMS", sums.len()));
        assets.push(asset_json(server, "SHA256SUMS.minisig", signature.len()));
        server.serve("/files/SHA256SUMS", 200, sums);
        server.serve("/files/SHA256SUMS.minisig", 200, signature);
    }
    let releases = format!(
        r#"[{{"tag_name":"v100.0.0","draft":true,"assets":[]}},
            {{"tag_name":"{RELEASE_TAG}","draft":false,"name":"Big release","body":"notes",
              "assets":[{}]}}]"#,
        assets.join(",")
    );
    server.serve("/releases", 200, releases);
}

fn asset_json(server: &ReleaseServer, name: &str, size: usize) -> String {
    format!(
        r#"{{"name":"{name}","browser_download_url":"{}","size":{size}}}"#,
        server.url(&format!("/files/{name}"))
    )
}
