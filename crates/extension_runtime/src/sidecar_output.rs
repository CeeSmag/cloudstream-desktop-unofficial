//! Reading what the sidecar prints: protocol replies on stdout, logs on stderr.

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::Sender;

/// Lines the sidecar's Cloudflare handling prints carry this tag; they get their own log file and
/// are always kept in the main log (the whole story of a verification must be traceable).
const CLOUDFLARE_TAG: &str = "[cloudflare]";

/// Longest sidecar output line kept in the log, in characters.
const MAX_LOGGED_LINE_CHARS: usize = 400;

/// How many of the first words of a line are searched for its level.
const LEVEL_SEARCH_WORDS: usize = 4;

/// Importance of a line the sidecar or a plugin printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Passes the child's reply lines on to the caller waiting for them.
pub(crate) fn forward_stdout(stdout: impl Read, sender: &Sender<String>) {
    for line in lossy_lines(stdout) {
        if sender.send(line).is_err() {
            break;
        }
    }
}

/// Everything the runtime and the extensions print ends up in the app log with its own level,
/// so a failing extension leaves a trace. Debug and trace lines only reach the debug files.
pub(crate) fn log_stderr(stderr: impl Read) {
    let mut previous = LineLevel::Info;
    for line in lossy_lines(stderr) {
        let line = shortened(&line);
        if line.contains(CLOUDFLARE_TAG) {
            log_cloudflare_line(line, previous);
            continue;
        }
        previous = classify(line, previous);
        match previous {
            LineLevel::Error => tracing::error!(target: "sidecar", "{line}"),
            LineLevel::Warn => tracing::warn!(target: "sidecar", "{line}"),
            LineLevel::Info => tracing::info!(target: "sidecar", "{line}"),
            LineLevel::Debug => tracing::debug!(target: "sidecar", "{line}"),
            LineLevel::Trace => tracing::trace!(target: "sidecar", "{line}"),
        }
    }
}

/// Logs one Cloudflare line under its own target, never below info.
fn log_cloudflare_line(line: &str, previous: LineLevel) {
    match classify(line, previous) {
        LineLevel::Error => {
            tracing::error!(target: "extension_runtime::cloudflare_sidecar", "{line}")
        }
        LineLevel::Warn => {
            tracing::warn!(target: "extension_runtime::cloudflare_sidecar", "{line}")
        }
        _ => tracing::info!(target: "extension_runtime::cloudflare_sidecar", "{line}"),
    }
}

/// Reads lines from the child's output and never stops at bytes that are not valid UTF-8:
/// extensions print text in the Windows code page, and a reader that quit there would leave the
/// pipe undrained, freezing the child as soon as the pipe filled up.
pub(crate) fn lossy_lines(source: impl Read) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(source);
    std::iter::from_fn(move || {
        let mut bytes = Vec::new();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => None,
            Ok(_) => Some(
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches(['\r', '\n'])
                    .to_owned(),
            ),
            Err(error) => {
                tracing::warn!(%error, "cannot read the sidecar output any more");
                None
            }
        }
    })
}

/// Extensions sometimes print whole web pages; the log keeps only the start of such a line.
pub(crate) fn shortened(line: &str) -> &str {
    match line.char_indices().nth(MAX_LOGGED_LINE_CHARS) {
        Some((end, _)) => &line[..end],
        None => line,
    }
}

/// The level of a line: the level word it starts with (`[sidecar] WARN ...`, `DEBUG Name: ...`,
/// `[main] INFO ...`), else the level of the line before it (stack traces and page dumps follow
/// the line they belong to), raised to a warning when it reads like trouble.
pub(crate) fn classify(line: &str, previous: LineLevel) -> LineLevel {
    if let Some(level) = explicit_level(line) {
        return level;
    }
    if previous == LineLevel::Info && looks_like_trouble(line) {
        return LineLevel::Warn;
    }
    previous
}

fn explicit_level(line: &str) -> Option<LineLevel> {
    line.split_whitespace()
        .take(LEVEL_SEARCH_WORDS)
        .find_map(|word| match word.trim_matches(['[', ']', ':']) {
            "ERROR" | "SEVERE" => Some(LineLevel::Error),
            "WARN" | "WARNING" => Some(LineLevel::Warn),
            "INFO" => Some(LineLevel::Info),
            "DEBUG" => Some(LineLevel::Debug),
            "TRACE" => Some(LineLevel::Trace),
            _ => None,
        })
}

fn looks_like_trouble(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "exception",
        "error",
        "failed",
        "cannot",
        "challenge",
        "timed out",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_word_of_a_line_decides() {
        let info = LineLevel::Info;
        assert_eq!(
            classify("[sidecar] ERROR request failed", info),
            LineLevel::Error
        );
        assert_eq!(
            classify("DEBUG Fibwatch: searchUrl: x", info),
            LineLevel::Debug
        );
        assert_eq!(
            classify("[main] INFO CloudStreamDesktop - loaded", LineLevel::Debug),
            LineLevel::Info
        );
        assert_eq!(classify("WARN sidecar: slow", info), LineLevel::Warn);
        assert_eq!(classify("[sidecar] TRACE step", info), LineLevel::Trace);
    }

    #[test]
    fn lines_without_a_level_follow_the_line_before() {
        assert_eq!(
            classify("</body></html>", LineLevel::Debug),
            LineLevel::Debug
        );
        assert_eq!(
            classify("\tat java.base/Foo.bar", LineLevel::Error),
            LineLevel::Error
        );
    }

    #[test]
    fn unlabelled_trouble_after_a_normal_line_is_a_warning() {
        assert_eq!(
            classify(
                "java.lang.IllegalStateException: no provider",
                LineLevel::Info
            ),
            LineLevel::Warn
        );
        assert_eq!(
            classify("[sidecar] ready", LineLevel::Info),
            LineLevel::Info
        );
    }

    #[test]
    fn reading_goes_on_after_a_line_that_is_not_utf8() {
        let bytes: &[u8] = b"first\r\ncaf\xe9 au lait\nlast";
        let lines: Vec<String> = lossy_lines(bytes).collect();
        assert_eq!(lines.len(), 3, "the bad line must not end the reading");
        assert_eq!(lines[0], "first");
        assert!(lines[1].starts_with("caf") && lines[1].ends_with("au lait"));
        assert_eq!(lines[2], "last");
    }

    #[test]
    fn very_long_lines_are_cut_but_short_ones_are_kept() {
        let page = "<html>".repeat(500);
        assert_eq!(shortened(&page).chars().count(), 400);
        assert_eq!(shortened("[sidecar] ready"), "[sidecar] ready");
        assert_eq!(shortened("é".repeat(401).as_str()).chars().count(), 400);
    }
}
