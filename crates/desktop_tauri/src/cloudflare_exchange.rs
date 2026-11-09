//! The files the sidecar and the app use to hand a Cloudflare verification over.
//!
//! The sidecar writes `request-<id>.txt` (`id`, `host`, `url`, one `key=value` per line) in
//! `<data>/sidecar/cloudflare` when a site answers a plugin with a "verify you are human" page, and
//! waits for `result-<id>.txt` (`ok=true|false`, `userAgent`, one `cookie=name=value` per cookie).
//! Plain text on purpose: the sidecar side needs no JSON library for it.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A site the sidecar wants verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyRequest {
    /// Names the files of this request.
    pub id: String,
    /// The site's host.
    pub host: String,
    /// The page that answered with the challenge; the window opens on it.
    pub url: String,
}

/// What the viewer did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The check was passed: the browser's cookies and `User-Agent`.
    Passed {
        /// The `User-Agent` the browser sent; Cloudflare ties its cookie to it.
        user_agent: String,
        /// Every cookie the browser holds for the site.
        cookies: Vec<(String, String)>,
    },
    /// Nothing was passed (window closed, time ran out, window could not open).
    Declined(String),
    /// Not asked for yet: the viewer has not said this site may open a window. The sidecar fails the
    /// request without holding it against the site, and asks again next time.
    Waiting(String),
}

/// Requests older than this were left by a sidecar that is gone.
const REQUEST_MAX_AGE: Duration = Duration::from_secs(10 * 60);

/// Reads a request file; None when it lacks a field or names something that is not a web page.
pub fn parse_request(text: &str) -> Option<VerifyRequest> {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let request = VerifyRequest {
        id: field("id=")?,
        host: field("host=")?,
        url: field("url=")?,
    };
    let safe_id = request
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-');
    let web_page = request.url.starts_with("https://") || request.url.starts_with("http://");
    (safe_id && web_page).then_some(request)
}

/// The text of a result file.
pub fn format_result(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Declined(reason) => format!("ok=false\nreason={}\n", reason.replace('\n', " ")),
        Outcome::Waiting(reason) => format!(
            "ok=false\nreason={}\nretry=true\n",
            reason.replace('\n', " ")
        ),
        Outcome::Passed {
            user_agent,
            cookies,
        } => {
            let mut text = format!("ok=true\nuserAgent={}\n", user_agent.replace('\n', " "));
            for (name, value) in cookies {
                text.push_str(&format!("cookie={name}={value}\n"));
            }
            text
        }
    }
}

/// Recent files of the exchange folder named `<prefix>*.txt`, oldest first: their text and file name.
pub fn fresh_files(dir: &Path, prefix: &str) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let wanted = name.starts_with(prefix) && name.ends_with(".txt");
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
            let fresh = modified.elapsed().is_ok_and(|age| age < REQUEST_MAX_AGE);
            let text = std::fs::read_to_string(entry.path()).ok()?;
            (wanted && fresh).then_some((modified, text, name))
        })
        .collect();
    found.sort_by_key(|(modified, _, _)| *modified);
    found
        .into_iter()
        .map(|(_, text, name)| (text, name))
        .collect()
}

/// Verification requests that are new since `seen` (which is updated), oldest first.
pub fn pending_requests(dir: &Path, seen: &mut HashSet<String>) -> Vec<VerifyRequest> {
    fresh_files(dir, "request-")
        .into_iter()
        .filter_map(|(text, _)| parse_request(&text))
        .filter(|request| seen.insert(request.id.clone()))
        .collect()
}

/// Hands an answer to the sidecar: written whole, then renamed, so the sidecar never reads half of it.
pub fn write_named(dir: &Path, name: &str, text: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let partial = dir.join(format!("{name}.tmp"));
    let done = dir.join(format!("{name}.txt"));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, &done)?;
    Ok(done)
}

/// Hands the verification answer for request `id` to the sidecar.
pub fn write_result(dir: &Path, id: &str, outcome: &Outcome) -> io::Result<PathBuf> {
    write_named(dir, &format!("result-{id}"), &format_result(outcome))
}

/// Empties the folder at start-up: whatever is in it belongs to a sidecar that no longer runs.
pub fn clear_folder(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_cfx_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create folder");
        dir
    }

    #[test]
    fn a_request_needs_an_id_a_host_and_a_web_page() {
        let good = parse_request("id=abc-123\nhost=site.test\nurl=https://site.test/api?x=1\n");
        assert_eq!(good.map(|r| r.host), Some("site.test".to_owned()));
        assert_eq!(parse_request("id=a\nhost=h\n"), None);
        assert_eq!(parse_request("id=a\nhost=h\nurl=file:///c:/secret\n"), None);
        assert_eq!(parse_request("id=../x\nhost=h\nurl=https://h/\n"), None);
    }

    #[test]
    fn results_carry_the_cookies_and_the_agent_or_the_reason() {
        let passed = Outcome::Passed {
            user_agent: "Agent/1".to_owned(),
            cookies: vec![("cf_clearance".to_owned(), "a=b".to_owned())],
        };
        assert_eq!(
            format_result(&passed),
            "ok=true\nuserAgent=Agent/1\ncookie=cf_clearance=a=b\n"
        );
        assert_eq!(
            format_result(&Outcome::Declined("closed".into())),
            "ok=false\nreason=closed\n"
        );
    }

    #[test]
    fn a_waiting_answer_tells_the_sidecar_to_ask_again_next_time() {
        let text = format_result(&Outcome::Waiting("press Verify".into()));
        assert_eq!(text, "ok=false\nreason=press Verify\nretry=true\n");
    }

    #[test]
    fn new_requests_are_found_once_oldest_first_and_stray_files_are_ignored() {
        let dir = folder("scan");
        std::fs::write(
            dir.join("request-one.txt"),
            "id=one\nhost=a.test\nurl=https://a.test/\n",
        )
        .expect("one");
        std::fs::write(dir.join("request-bad.txt"), "garbage").expect("bad");
        std::fs::write(dir.join("result-one.txt"), "ok=true").expect("result");
        let mut seen = HashSet::new();
        let first = pending_requests(&dir, &mut seen);
        assert_eq!(
            first.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["one"]
        );
        assert!(
            pending_requests(&dir, &mut seen).is_empty(),
            "already handled"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_result_is_written_whole_and_the_start_up_clean_up_empties_the_folder() {
        let dir = folder("write");
        let path = write_result(&dir, "abc", &Outcome::Declined("closed".into())).expect("write");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "ok=false\nreason=closed\n"
        );
        assert!(!dir.join("result-abc.tmp").exists());
        clear_folder(&dir);
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
