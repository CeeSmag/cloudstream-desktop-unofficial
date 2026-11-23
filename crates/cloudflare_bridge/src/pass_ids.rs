//! Names for saved passes and for the app run that saved them.
//!
//! A [`PassId`] travels through the sidecar's exchange folder (`pass=` in a result, `rejected=` in a request)
//! and so through untrusted plugin code: it is checked like a request id and says nothing about the pass
//! (it is a counter, never derived from a cookie). A [`RunId`] marks which run of the app saved a pass, so a
//! session cookie is not carried into the next run (a browser forgets those when it closes).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The longest id accepted from a file.
const MAX_ID_LEN: usize = 64;

fn is_safe_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_ID_LEN
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// One saved pass.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PassId(String);

impl PassId {
    /// Reads an id from the exchange; None unless it is 1-64 letters, digits or dashes.
    pub fn parse(text: &str) -> Option<Self> {
        is_safe_id(text).then(|| Self(text.to_owned()))
    }

    /// The id as written to files.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One run of the app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId(String);

impl RunId {
    /// Reads a run id from a saved file.
    pub fn parse(text: &str) -> Option<Self> {
        is_safe_id(text).then(|| Self(text.to_owned()))
    }

    /// A run id for a run started at `started` by process `pid`: two runs never share one.
    pub fn for_run(started: SystemTime, pid: u32) -> Self {
        let nanos = started
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        Self(format!("r{nanos:x}-{pid:x}"))
    }

    /// The id as written to files.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Hands out the ids of the passes this run saves: `<run>-<n>`, unique across runs.
#[derive(Debug)]
pub struct PassIds {
    run: RunId,
    next: AtomicU64,
}

impl PassIds {
    /// Ids for `run`, counting from 1.
    pub fn new(run: RunId) -> Self {
        Self {
            run,
            next: AtomicU64::new(1),
        }
    }

    /// The run these ids belong to.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// A fresh id.
    pub fn next_id(&self) -> PassId {
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        PassId(format!("{}-{number}", self.run.0))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn parse_accepts_only_short_letters_digits_and_dashes() {
        assert_eq!(
            PassId::parse("r1a-2f-7").map(|id| id.to_string()),
            Some("r1a-2f-7".into())
        );
        for bad in [
            "",
            "../x",
            "a b",
            "a\nb",
            "a_b",
            "é",
            &"a".repeat(MAX_ID_LEN + 1),
        ] {
            assert_eq!(PassId::parse(bad), None, "{bad:?}");
            assert_eq!(RunId::parse(bad), None, "{bad:?}");
        }
        assert!(PassId::parse(&"a".repeat(MAX_ID_LEN)).is_some());
        assert_eq!(
            RunId::parse("r1-2").map(|run| run.as_str().to_owned()),
            Some("r1-2".into())
        );
    }

    #[test]
    fn runs_started_apart_or_by_another_process_differ_and_parse_back() {
        let at = UNIX_EPOCH + Duration::from_nanos(0x1234);
        let run = RunId::for_run(at, 0xab);
        assert_eq!(run.as_str(), "r1234-ab");
        assert_ne!(run, RunId::for_run(at + Duration::from_nanos(1), 0xab));
        assert_ne!(run, RunId::for_run(at, 0xac));
        assert_eq!(RunId::parse(run.as_str()), Some(run));
        assert_eq!(
            RunId::for_run(UNIX_EPOCH - Duration::from_secs(1), 1).as_str(),
            "r0-1"
        );
    }

    #[test]
    fn pass_ids_count_up_within_their_run_and_parse_back() {
        let ids = PassIds::new(RunId::parse("r9").expect("run"));
        let first = ids.next_id();
        assert_eq!(first.as_str(), "r9-1");
        assert_eq!(ids.next_id().as_str(), "r9-2");
        assert_eq!(ids.run().as_str(), "r9");
        assert_eq!(PassId::parse(first.as_str()), Some(first));
    }

    #[test]
    fn pass_ids_handed_out_from_many_threads_are_all_different() {
        let ids = Arc::new(PassIds::new(RunId::parse("r1").expect("run")));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let ids = Arc::clone(&ids);
                std::thread::spawn(move || (0..500).map(|_| ids.next_id()).collect::<Vec<_>>())
            })
            .collect();
        let mut seen = HashSet::new();
        for thread in threads {
            for id in thread.join().expect("thread") {
                assert!(seen.insert(id), "an id was handed out twice");
            }
        }
        assert_eq!(seen.len(), 8 * 500);
    }
}
