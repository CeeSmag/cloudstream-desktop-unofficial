//! Lines the app and the verification helper (`cloudstream-verify`) exchange over its stdin/stdout.
//!
//! One command or status per line, fields separated by tabs (none of them can hold a tab: ids,
//! hosts and web addresses are checked or have their tabs replaced).

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
    },
    /// The check was not passed (closed, timed out, could not open).
    Failed {
        /// The site.
        host: String,
        /// Why, for the viewer.
        reason: String,
    },
}

fn field(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// The line for `command`, newline included.
pub fn format_command(command: &HelperCommand) -> String {
    match command {
        HelperCommand::Verify(request) | HelperCommand::Present(request) => format!(
            "{}\t{}\t{}\t{}\n",
            if matches!(command, HelperCommand::Present(_)) {
                "present"
            } else {
                "verify"
            },
            field(&request.id),
            field(&request.host),
            field(&request.url)
        ),
        HelperCommand::Show { host, id } => format!("show\t{}\t{}\n", field(host), field(id)),
        HelperCommand::Activate { host, id } => {
            format!("activate\t{}\t{}\n", field(host), field(id))
        }
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
        "show" => Some(HelperCommand::Show {
            host: parts.next()?.to_owned(),
            id: parts.next()?.to_owned(),
        }),
        verb @ ("verify" | "present") => {
            let text = format!(
                "id={}\nhost={}\nurl={}\n",
                parts.next()?,
                parts.next()?,
                parts.next()?
            );
            let request = crate::parse_request(&text)?;
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
        HelperStatus::Passed { host } => format!("passed\t{}\n", field(host)),
        HelperStatus::Failed { host, reason } => {
            format!("failed\t{}\t{}\n", field(host), field(reason))
        }
    }
}

/// Reads a status line; None for anything else (the helper's own log lines, for example).
pub fn parse_status(line: &str) -> Option<HelperStatus> {
    let mut parts = line.trim_end_matches(['\n', '\r']).split('\t');
    let kind = parts.next()?;
    let host = parts.next().filter(|host| !host.is_empty())?.to_owned();
    match kind {
        "open" => Some(HelperStatus::WindowOpen {
            host,
            id: parts.next()?.to_owned(),
        }),
        "passed" => Some(HelperStatus::Passed { host }),
        "failed" => Some(HelperStatus::Failed {
            host,
            reason: parts.next().unwrap_or("not verified").to_owned(),
        }),
        _ => None,
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
            },
            HelperStatus::Failed {
                host: "a.test".into(),
                reason: "closed\tby viewer".into(),
            },
        ];
        let parsed: Vec<_> = statuses
            .iter()
            .map(|status| parse_status(&format_status(status)))
            .collect();
        assert_eq!(parsed[0], Some(statuses[0].clone()));
        assert_eq!(parsed[1], Some(statuses[1].clone()));
        assert_eq!(
            parsed[2],
            Some(HelperStatus::Failed {
                host: "a.test".into(),
                reason: "closed by viewer".into()
            })
        );
    }

    #[test]
    fn status_log_noise_is_ignored() {
        assert_eq!(parse_status("2026-01-01 INFO something\n"), None);
        assert_eq!(parse_status("passed\t\n"), None);
    }
}
