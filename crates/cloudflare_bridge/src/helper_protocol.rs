//! Lines the app and the verification helper (`cloudstream-verify`) exchange over its stdin/stdout.
//!
//! One command or status per line, fields separated by tabs (none of them can hold a tab: ids,
//! hosts and web addresses are checked or have their tabs replaced).

use crate::issued_pass::{decode_pass, encode_pass, IssuedPass, PassWireError};
use crate::VerifyRequest;

/// What the app asks the helper to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperCommand {
    /// Start or join a background check and answer request `id` when the check is passed.
    Verify(VerifyRequest),
    /// Start a check at the viewer's request and present its window.
    Present(VerifyRequest),
    /// Present an ongoing check for this host, if it still exists.
    Show {
        /// The site named by the viewer.
        host: String,
        /// A request belonging to the check the viewer saw.
        id: String,
    },
    /// Activate the shown check after the parent grants this helper foreground permission.
    Activate {
        /// Site of the shown check.
        host: String,
        /// Request associated with the viewer action.
        id: String,
    },
    /// Retire a request after the provider or viewer cancels it.
    Cancel {
        /// The pending request to retire.
        id: String,
    },
    /// Close every window and exit.
    Quit,
}

/// What the helper reports back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperStatus {
    /// The window for the site is open and waits for the viewer.
    WindowOpen {
        /// The request waiting for this check.
        id: String,
        /// The site.
        host: String,
    },
    /// The check was passed; the window is hidden and kept for browser fetches.
    Passed {
        /// The site.
        host: String,
        /// The completed request.
        id: String,
        /// The cookies and identity handed to the sidecar, so the app can keep them; None when there are
        /// none to keep (the browser was not challenged) or the line came from an older helper.
        issued: Option<IssuedPass>,
    },
    /// The check was not passed (closed, timed out, could not open).
    Failed {
        /// The site.
        host: String,
        /// The completed request.
        id: String,
        /// Why, for the viewer.
        reason: String,
    },
    /// A browser fetch was answered for the sidecar. The app never sees these requests (the helper
    /// answers them itself), so this is how it learns the helper is busy and must not be closed.
    Fetched {
        /// The site.
        host: String,
        /// The fetch request.
        id: String,
    },
}

fn field(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// The line for `command`, newline included.
pub fn format_command(command: &HelperCommand) -> String {
    match command {
        HelperCommand::Verify(request) | HelperCommand::Present(request) => format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            if matches!(command, HelperCommand::Present(_)) {
                "present"
            } else {
                "verify"
            },
            field(&request.id),
            field(&request.host),
            field(&request.url),
            field(&request.method),
            u8::from(request.standalone)
        ),
        HelperCommand::Show { host, id } => format!("show\t{}\t{}\n", field(host), field(id)),
        HelperCommand::Activate { host, id } => {
            format!("activate\t{}\t{}\n", field(host), field(id))
        }
        HelperCommand::Cancel { id } => format!("cancel\t{}\n", field(id)),
        HelperCommand::Quit => "quit\n".to_owned(),
    }
}

/// Reads a command line; None for anything else.
pub fn parse_command(line: &str) -> Option<HelperCommand> {
    let mut parts = line.trim_end_matches(['\n', '\r']).split('\t');
    match parts.next()? {
        "activate" => Some(HelperCommand::Activate {
            host: parts.next()?.into(),
            id: parts.next()?.into(),
        }),
        "quit" => Some(HelperCommand::Quit),
        "cancel" => Some(HelperCommand::Cancel {
            id: parts.next()?.to_owned(),
        }),
        "show" => Some(HelperCommand::Show {
            host: parts.next()?.to_owned(),
            id: parts.next()?.to_owned(),
        }),
        verb @ ("verify" | "present") => {
            let text = format!(
                "id={}\nhost={}\nurl={}\nmethod={}\n",
                parts.next()?,
                parts.next()?,
                parts.next()?,
                parts.next()?
            );
            let mut request = crate::parse_request(&text)?;
            request.standalone = parts.next() == Some("1");
            Some(if verb == "present" {
                HelperCommand::Present(request)
            } else {
                HelperCommand::Verify(request)
            })
        }
        _ => None,
    }
}

/// The line for `status`, newline included.
pub fn format_status(status: &HelperStatus) -> String {
    match status {
        HelperStatus::WindowOpen { host, id } => format!("open\t{}\t{}\n", field(host), field(id)),
        HelperStatus::Passed { host, id, issued } => match issued {
            None => format!("passed\t{}\t{}\n", field(host), field(id)),
            Some(pass) => format!(
                "passed\t{}\t{}\t{}\n",
                field(host),
                field(id),
                field(&encode_pass(pass))
            ),
        },
        HelperStatus::Failed { host, id, reason } => {
            format!(
                "failed\t{}\t{}\t{}\n",
                field(host),
                field(id),
                field(reason)
            )
        }
        HelperStatus::Fetched { host, id } => format!("fetched\t{}\t{}\n", field(host), field(id)),
    }
}

/// What one line of the helper's stdout is.
#[derive(Debug)]
pub enum StatusLine {
    /// A status.
    Status(HelperStatus),
    /// A `passed` line whose pass cannot be read. The request did pass (the helper has answered the
    /// sidecar); only the pass is not kept.
    PassUnreadable {
        /// The status, without its pass.
        status: HelperStatus,
        /// Why the pass was refused (never quotes the payload).
        reason: PassWireError,
    },
    /// Starts with a status word but is no valid status. It may hold a pass, so it is never logged as text.
    Malformed {
        /// The status word.
        verb: &'static str,
    },
    /// Anything else: the helper's own log output.
    Log,
}

/// The status words; a line starting with one of them is a status, valid or not.
const STATUS_VERBS: [&str; 4] = ["open", "passed", "fetched", "failed"];

/// Sorts one stdout line of the helper.
pub fn classify_status_line(line: &str) -> StatusLine {
    let mut parts = line.trim_end_matches(['\n', '\r']).split('\t');
    let Some(verb) = parts
        .next()
        .and_then(|word| STATUS_VERBS.into_iter().find(|known| *known == word))
    else {
        return StatusLine::Log;
    };
    let host = parts.next().filter(|host| !host.is_empty());
    let id = parts.next().filter(|id| !id.is_empty());
    let (Some(host), Some(id)) = (host, id) else {
        return StatusLine::Malformed { verb };
    };
    let (host, id) = (host.to_owned(), id.to_owned());
    match verb {
        "open" => StatusLine::Status(HelperStatus::WindowOpen { host, id }),
        "fetched" => StatusLine::Status(HelperStatus::Fetched { host, id }),
        "failed" => StatusLine::Status(HelperStatus::Failed {
            host,
            id,
            reason: parts.next().unwrap_or("not verified").to_owned(),
        }),
        _ => passed_line(host, id, parts.next()),
    }
}

fn passed_line(host: String, id: String, payload: Option<&str>) -> StatusLine {
    match payload.filter(|text| !text.is_empty()).map(decode_pass) {
        None => StatusLine::Status(HelperStatus::Passed {
            host,
            id,
            issued: None,
        }),
        Some(Ok(pass)) => StatusLine::Status(HelperStatus::Passed {
            host,
            id,
            issued: Some(pass),
        }),
        Some(Err(reason)) => StatusLine::PassUnreadable {
            status: HelperStatus::Passed {
                host,
                id,
                issued: None,
            },
            reason,
        },
    }
}

/// Reads a status line; None for anything else (the helper's own log lines, for example). A `passed`
/// line with an unreadable pass is still a pass of the check, without the pass.
pub fn parse_status(line: &str) -> Option<HelperStatus> {
    match classify_status_line(line) {
        StatusLine::Status(status) | StatusLine::PassUnreadable { status, .. } => Some(status),
        StatusLine::Malformed { .. } | StatusLine::Log => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_and_background_commands_have_distinct_wire_intent() {
        let background = HelperCommand::Verify(request());
        let line = format_command(&background).replacen("verify", "present", 1);
        let interactive = parse_command(&line).expect("presentation command");
        assert_ne!(interactive, background);
        assert_eq!(format_command(&interactive), line);
    }

    fn request() -> VerifyRequest {
        VerifyRequest {
            id: "ab-1".into(),
            host: "site.test".into(),
            url: "https://site.test/a?b=c".into(),
            method: "GET".into(),
            standalone: false,
            reason: Default::default(),
        }
    }

    #[test]
    fn command_round_trip() {
        for command in [
            HelperCommand::Verify(request()),
            HelperCommand::Activate {
                host: "test.example".into(),
                id: "r1".into(),
            },
            HelperCommand::Quit,
        ] {
            assert_eq!(parse_command(&format_command(&command)), Some(command));
        }
    }

    #[test]
    fn command_unsafe_verify_is_refused() {
        assert_eq!(parse_command("verify\t../x\th\thttps://h/\n"), None);
        assert_eq!(parse_command("verify\ta\th\tfile:///etc/passwd\n"), None);
        assert_eq!(parse_command("verify\ta\th\n"), None);
        assert_eq!(parse_command("dance\n"), None);
    }

    #[test]
    fn status_round_trip_and_tabs_in_reason_are_flattened() {
        let statuses = [
            HelperStatus::WindowOpen {
                id: "r1".into(),
                host: "a.test".into(),
            },
            HelperStatus::Passed {
                host: "a.test".into(),
                id: "r1".into(),
                issued: None,
            },
            HelperStatus::Failed {
                host: "a.test".into(),
                id: "r1".into(),
                reason: "closed\tby viewer".into(),
            },
            HelperStatus::Fetched {
                host: "a.test".into(),
                id: "f1".into(),
            },
        ];
        let parsed: Vec<_> = statuses
            .iter()
            .map(|status| parse_status(&format_status(status)))
            .collect();
        assert_eq!(parsed[0], Some(statuses[0].clone()));
        assert_eq!(parsed[1], Some(statuses[1].clone()));
        assert_eq!(parsed[3], Some(statuses[3].clone()));
        assert_eq!(
            parsed[2],
            Some(HelperStatus::Failed {
                host: "a.test".into(),
                id: "r1".into(),
                reason: "closed by viewer".into()
            })
        );
    }

    fn issued() -> IssuedPass {
        IssuedPass {
            user_agent: "Agent\t/1".into(),
            cookies: vec![crate::PassCookie::new(crate::CookieParts {
                name: "cf_clearance".into(),
                value: crate::CookieValue::new("secret-value"),
                domain: ".a.test".into(),
                path: "/".into(),
                secure: true,
                http_only: true,
                expires: crate::CookieExpiry::Session,
            })
            .expect("cookie")],
        }
    }

    #[test]
    fn a_passed_status_carries_its_pass_on_one_line() {
        let status = HelperStatus::Passed {
            host: "a.test".into(),
            id: "r1".into(),
            issued: Some(issued()),
        };
        let line = format_status(&status);
        assert_eq!(line.matches('\n').count(), 1);
        assert_eq!(line.matches('\t').count(), 3);
        assert_eq!(parse_status(&line), Some(status));
    }

    #[test]
    fn an_older_helper_s_passed_line_has_no_pass() {
        let expected = HelperStatus::Passed {
            host: "a.test".into(),
            id: "r1".into(),
            issued: None,
        };
        assert_eq!(parse_status("passed\ta.test\tr1\n"), Some(expected.clone()));
        assert_eq!(parse_status("passed\ta.test\tr1\t\n"), Some(expected));
    }

    #[test]
    fn an_unreadable_pass_still_passes_the_check_without_the_pass() {
        let line = "passed\ta.test\tr1\t{not json secret-value";
        match classify_status_line(line) {
            StatusLine::PassUnreadable { status, reason } => {
                assert_eq!(
                    status,
                    HelperStatus::Passed {
                        host: "a.test".into(),
                        id: "r1".into(),
                        issued: None
                    }
                );
                assert!(!reason.to_string().contains("secret-value"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_status(line),
            Some(HelperStatus::Passed { issued: None, .. })
        ));
    }

    fn malformed_verb(line: &str) -> Option<&'static str> {
        match classify_status_line(line) {
            StatusLine::Malformed { verb } => Some(verb),
            _ => None,
        }
    }

    #[test]
    fn a_status_word_without_host_or_id_is_malformed_never_a_log_line() {
        let lines = STATUS_VERBS
            .into_iter()
            .flat_map(|verb| [verb.to_owned(), format!("{verb}\t"), format!("{verb}\th\t")]);
        for line in lines {
            let verb = line.split('\t').next().unwrap_or_default().to_owned();
            assert_eq!(malformed_verb(&line), Some(verb.as_str()), "{line:?}");
            assert_eq!(parse_status(&line), None);
        }
    }

    #[test]
    fn a_line_not_starting_with_a_status_word_is_the_helper_s_log() {
        for line in ["2026 INFO passed", "passedx\th\ti", ""] {
            assert!(
                matches!(classify_status_line(line), StatusLine::Log),
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_failed_status_without_a_reason_says_not_verified() {
        assert_eq!(
            parse_status("failed\th\ti"),
            Some(HelperStatus::Failed {
                host: "h".into(),
                id: "i".into(),
                reason: "not verified".into()
            })
        );
    }

    #[test]
    fn status_log_noise_is_ignored() {
        assert_eq!(parse_status("2026-01-01 INFO something\n"), None);
        assert_eq!(parse_status("passed\t\n"), None);
    }
}
