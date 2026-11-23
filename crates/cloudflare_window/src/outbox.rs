//! Everything the helper hands back: answers for the sidecar (files) and statuses for the app (stdout).

use std::io::Write;
use std::path::PathBuf;

use cloudflare_bridge::{
    format_status, write_fetch_result, write_result, FetchOutcome, HelperStatus, IssuedPass,
    Outcome, VerifyRequest,
};

/// Where answers go.
pub(crate) struct Outbox {
    pub(crate) exchange: PathBuf,
}

impl Outbox {
    pub(crate) fn answer(&self, request: &VerifyRequest, outcome: &Outcome) -> Result<(), String> {
        if request.standalone {
            return Ok(());
        }
        write_result(&self.exchange, &request.id, outcome).map(|_| ()).map_err(|error| {
            tracing::error!(%error, id = %request.id, "could not deliver the verification result");
            "the verification result could not be delivered".to_owned()
        })
    }

    /// Answers one request and says how it went, for the status the app is told.
    pub(crate) fn deliver(&self, request: &VerifyRequest, outcome: &Outcome) -> HelperStatus {
        let (host, id) = (request.host.clone(), request.id.clone());
        match (outcome, self.answer(request, outcome)) {
            (_, Err(reason)) => HelperStatus::Failed { host, id, reason },
            (Outcome::Declined(reason) | Outcome::Waiting(reason), Ok(())) => {
                HelperStatus::Failed {
                    host,
                    id,
                    reason: reason.clone(),
                }
            }
            (
                Outcome::Passed {
                    user_agent,
                    cookies,
                },
                Ok(()),
            ) => HelperStatus::Passed {
                host,
                id,
                issued: Some(IssuedPass {
                    user_agent: user_agent.clone(),
                    cookies: cookies.clone(),
                }),
            },
            (Outcome::BrowserClear { .. } | Outcome::Redirect { .. }, Ok(())) => {
                HelperStatus::Passed {
                    host,
                    id,
                    issued: None,
                }
            }
        }
    }

    /// Hands a browser fetch's outcome to the sidecar.
    pub(crate) fn fetch_result(&self, id: &str, outcome: &FetchOutcome) {
        if let Err(error) = write_fetch_result(&self.exchange, id, outcome) {
            tracing::warn!(%error, %id, "could not answer a browser fetch");
        }
    }

    /// Tells the app how a site's check goes.
    pub(crate) fn status(&self, status: &HelperStatus) {
        send_status(&mut std::io::stdout().lock(), status);
    }
}

/// Writes one status line to the app's pipe and flushes it (the app acts on each line as it comes).
fn send_status(pipe: &mut impl Write, status: &HelperStatus) {
    let written = pipe
        .write_all(format_status(status).as_bytes())
        .and_then(|()| pipe.flush());
    if let Err(error) = written {
        tracing::warn!(%error, "the app no longer reads the helper's statuses");
    }
}

#[cfg(test)]
mod tests {
    use cloudflare_bridge::{CookieExpiry, CookieParts, CookieValue, PassCookie};

    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_outbox_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir
    }

    fn request(standalone: bool) -> VerifyRequest {
        VerifyRequest {
            id: "r1".into(),
            host: "site.test".into(),
            url: "https://site.test/".into(),
            method: "GET".into(),
            standalone,
            reason: Default::default(),
        }
    }

    fn passed() -> Outcome {
        let (user_agent, cookies) = pass_parts();
        Outcome::Passed {
            user_agent,
            cookies,
        }
    }

    fn pass_parts() -> (String, Vec<PassCookie>) {
        let cookie = PassCookie::new(CookieParts {
            name: "cf_clearance".into(),
            value: CookieValue::new("v"),
            domain: ".site.test".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: CookieExpiry::Session,
        })
        .expect("cookie");
        ("Agent".into(), vec![cookie])
    }

    #[test]
    fn a_passed_check_reports_its_pass_and_answers_the_sidecar() {
        let dir = folder("passed");
        let out = Outbox {
            exchange: dir.clone(),
        };
        let status = out.deliver(&request(false), &passed());
        let (user_agent, cookies) = pass_parts();
        assert_eq!(
            status,
            HelperStatus::Passed {
                host: "site.test".into(),
                id: "r1".into(),
                issued: Some(IssuedPass {
                    user_agent,
                    cookies
                })
            }
        );
        let answer = std::fs::read_to_string(dir.join("result-r1.txt")).expect("answer");
        assert!(answer.starts_with("ok=true\n"), "{answer}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_browser_clear_check_has_no_pass_to_keep() {
        let dir = folder("clear");
        let out = Outbox {
            exchange: dir.clone(),
        };
        let status = out.deliver(
            &request(false),
            &Outcome::BrowserClear {
                user_agent: "Agent".into(),
            },
        );
        assert!(matches!(status, HelperStatus::Passed { issued: None, .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_redirect_check_has_no_pass_to_keep_and_writes_its_location() {
        let dir = folder("redirect");
        let out = Outbox {
            exchange: dir.clone(),
        };
        let status = out.deliver(
            &request(false),
            &Outcome::Redirect {
                status: 301,
                location: "https://moved.test/path".into(),
            },
        );
        assert!(matches!(status, HelperStatus::Passed { issued: None, .. }));
        let answer = std::fs::read_to_string(dir.join("result-r1.txt")).expect("answer");
        assert_eq!(
            answer,
            "ok=true\ntransport=redirect\nstatus=301\nlocation=https://moved.test/path\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_declined_or_undeliverable_check_is_a_failure() {
        let dir = folder("declined");
        let out = Outbox {
            exchange: dir.clone(),
        };
        let declined = out.deliver(&request(false), &Outcome::Declined("closed".into()));
        assert!(matches!(declined, HelperStatus::Failed { ref reason, .. } if reason == "closed"));
        let waiting = out.deliver(&request(false), &Outcome::Waiting("later".into()));
        assert!(matches!(waiting, HelperStatus::Failed { ref reason, .. } if reason == "later"));
        let blocked = dir.join("not-a-folder");
        std::fs::write(&blocked, "file").expect("file");
        let broken = Outbox { exchange: blocked };
        let failed = broken.deliver(&request(false), &passed());
        assert!(
            matches!(failed, HelperStatus::Failed { ref reason, .. } if reason.contains("could not be delivered"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pipe that records what was written and flushed, or fails.
    #[derive(Default)]
    struct Pipe {
        written: Vec<u8>,
        flushed: usize,
        broken: bool,
    }

    impl Write for Pipe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.broken {
                return Err(std::io::Error::other("closed"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushed += 1;
            Ok(())
        }
    }

    #[test]
    fn a_status_is_sent_as_one_flushed_line_and_a_closed_pipe_is_survived() {
        let status = HelperStatus::Fetched {
            host: "site.test".into(),
            id: "f1".into(),
        };
        let mut pipe = Pipe::default();
        send_status(&mut pipe, &status);
        assert_eq!(pipe.written, format_status(&status).into_bytes());
        assert_eq!(pipe.flushed, 1, "the app reads it at once");
        let mut closed = Pipe {
            broken: true,
            ..Pipe::default()
        };
        send_status(&mut closed, &status);
        assert!(closed.written.is_empty());
    }

    #[test]
    fn a_fetch_result_is_handed_to_the_sidecar() {
        let dir = folder("fetch");
        let out = Outbox {
            exchange: dir.clone(),
        };
        out.fetch_result("f1", &FetchOutcome::failed("no window"));
        let answer = std::fs::read_to_string(dir.join("fetchresult-f1.txt")).expect("answer");
        assert!(answer.contains("no window"), "{answer}");
        let blocked = Outbox {
            exchange: dir.join("result-missing").join("x"),
        };
        std::fs::write(dir.join("result-missing"), "file").expect("file");
        blocked.fetch_result("f2", &FetchOutcome::failed("x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_standalone_retry_writes_no_answer_file() {
        let dir = folder("standalone");
        let out = Outbox {
            exchange: dir.clone(),
        };
        let status = out.deliver(&request(true), &passed());
        assert!(matches!(
            status,
            HelperStatus::Passed {
                issued: Some(_),
                ..
            }
        ));
        assert!(!dir.join("result-r1.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
