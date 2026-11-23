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
            (Outcome::BrowserClear { .. }, Ok(())) => HelperStatus::Passed {
                host,
                id,
                issued: None,
            },
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
        let mut stdout = std::io::stdout().lock();
        let written = stdout
            .write_all(format_status(status).as_bytes())
            .and_then(|()| stdout.flush());
        if let Err(error) = written {
            tracing::warn!(%error, "the app no longer reads the helper's statuses");
        }
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
