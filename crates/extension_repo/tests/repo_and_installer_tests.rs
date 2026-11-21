//! Integration tests for `extension_repo` manifest loading, safe filenames, installer, and probe.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use extension_repo::{
    plugin_file_name, plugin_path_in, ExtensionInstaller, HttpFetcher, HttpProbe, RepoError,
    RepositoryClient, UreqFetcher, UreqProbe,
};

struct LocalHttp {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LocalHttp {
    fn start(routes: Vec<(String, u16, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        let _ = s.set_nonblocking(false);
                        let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 2048];
                        let n = s.read(&mut buf).unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let path = req
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let (status, body) = routes
                            .iter()
                            .find(|(p, _, _)| p == path)
                            .map(|(_, st, b)| (*st, b.as_slice()))
                            .unwrap_or((404, b"not found"));
                        let reason = if status == 200 { "OK" } else { "Not Found" };
                        let header = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = s.write_all(header.as_bytes());
                        let _ = s.write_all(body);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url: format!("http://{addr}"),
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for LocalHttp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct MockFetcher(Vec<(String, Vec<u8>)>);

impl HttpFetcher for MockFetcher {
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
        self.0
            .iter()
            .find(|(u, _)| u == url)
            .map(|(_, b)| b.clone())
            .ok_or_else(|| RepoError::Download {
                url: url.into(),
                reason: "404".into(),
            })
    }
}

#[test]
fn repository_client_and_installer_work_end_to_end() {
    let repo_url = "https://repo.example/repo.json";
    let plugins_url = "https://repo.example/plugins.json";
    let cs3_url = "https://repo.example/DemoPlugin.cs3";
    let zip_bytes = b"PK\x03\x04fake-cs3-payload".to_vec();

    let repo_json = format!(r#"{{"name":"Demo Repo","pluginLists":["{plugins_url}"]}}"#);
    let plugins_json = format!(
        r#"[{{"name":"Demo Plugin","internalName":"DemoPlugin","version":4,"url":"{cs3_url}","fileSize":{}}}]"#,
        zip_bytes.len()
    );

    let client = RepositoryClient::new(MockFetcher(vec![
        (repo_url.into(), repo_json.into_bytes()),
        (plugins_url.into(), plugins_json.into_bytes()),
    ]));
    let manifest = client.fetch_manifest(repo_url).expect("fetch manifest");
    let plugins = client.fetch_plugins(&manifest);
    assert_eq!(manifest.name, "Demo Repo");
    assert_eq!(plugins.len(), 1);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cs_ext_repo_{}_{}", std::process::id(), nanos));
    let installer =
        ExtensionInstaller::new(MockFetcher(vec![(cs3_url.into(), zip_bytes)]), dir.clone());
    let installed_path = installer
        .install(&plugins[0], repo_url)
        .expect("install plugin");
    assert_eq!(
        installed_path.file_name().and_then(|n| n.to_str()),
        Some("DemoPlugin.cs3")
    );
    assert!(installed_path.exists());

    assert!(plugin_file_name("../evil").is_err());
    assert_eq!(plugin_file_name("GoodName").expect("ok"), "GoodName.cs3");
    assert!(plugin_path_in(&dir, "../escape.cs3").is_err());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ureq_fetcher_and_probe_handle_local_http_ok_and_404() {
    let server = LocalHttp::start(vec![
        ("/manifest.json".into(), 200, br#"{"ok":true}"#.to_vec()),
        ("/video.mp4".into(), 200, b"00000000ftypisom".to_vec()),
    ]);

    let fetcher = UreqFetcher::default();
    let body = fetcher
        .get_bytes(&format!("{}/manifest.json", server.base_url))
        .expect("fetch ok");
    assert_eq!(body, br#"{"ok":true}"#);
    assert!(fetcher
        .get_bytes(&format!("{}/missing.json", server.base_url))
        .is_err());

    let probe = UreqProbe::default();
    let ok = probe
        .probe(&format!("{}/video.mp4", server.base_url), &[])
        .expect("probe ok");
    assert_eq!(ok.status, 200);
    let miss = probe
        .probe(&format!("{}/missing.mp4", server.base_url), &[])
        .expect("probe returns status");
    assert_eq!(miss.status, 404);
}
