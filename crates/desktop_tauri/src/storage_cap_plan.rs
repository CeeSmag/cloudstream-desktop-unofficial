//! Deciding what to delete when the data folder is over the viewer's size limit. Pure: it only sorts and
//! adds up sizes, so it is tested without touching a disk (`storage_cap_scan` finds the entries,
//! `storage_cap` deletes them).
//!
//! Only things that can be made again are ever offered for deletion. The database, the installed
//! plugins, the settings and the program that is running are never entries.

use std::path::PathBuf;

/// What kind of thing an entry is. The order is the order of deletion: the cheapest to lose first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// An unpacked program folder of an older version (`runtime-*`), not the one running.
    OldRuntime,
    /// A debug log file that is not being written any more.
    DebugLog,
    /// A downloaded and converted subtitle.
    Subtitle,
    /// Saved stream links of a title.
    Sources,
    /// A browser cache folder of the Cloudflare check's browser (cookies stay).
    BrowserCache,
}

impl Kind {
    /// A short name for the log and the page.
    pub fn label(self) -> &'static str {
        match self {
            Self::OldRuntime => "old program folder",
            Self::DebugLog => "debug log",
            Self::Subtitle => "subtitle",
            Self::Sources => "saved stream sources",
            Self::BrowserCache => "browser cache",
        }
    }
}

/// One file or folder that may be deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    /// Seconds since it was last changed.
    pub age_secs: u64,
}

/// What to delete, and where the folder ends up.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub delete: Vec<Entry>,
    /// The folder's size once everything in `delete` is gone.
    pub total_after: u64,
}

impl Plan {
    /// Whether the folder is still over the limit after the plan (what is left cannot be deleted).
    pub fn still_over(&self, cap_bytes: u64) -> bool {
        self.total_after > cap_bytes
    }
}

/// Picks what to delete so that a folder of `total` bytes gets down to `cap_bytes`: kinds in order,
/// oldest first inside a kind, and no more than needed.
pub fn plan(mut entries: Vec<Entry>, total: u64, cap_bytes: u64) -> Plan {
    if total <= cap_bytes {
        return Plan {
            delete: Vec::new(),
            total_after: total,
        };
    }
    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.age_secs.cmp(&a.age_secs)));
    let mut remaining = total;
    let mut delete = Vec::new();
    for entry in entries {
        if remaining <= cap_bytes {
            break;
        }
        remaining = remaining.saturating_sub(entry.bytes);
        delete.push(entry);
    }
    Plan {
        delete,
        total_after: remaining,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: Kind, bytes: u64, age_secs: u64) -> Entry {
        Entry {
            path: PathBuf::from(name),
            kind,
            bytes,
            age_secs,
        }
    }

    fn names(plan: &Plan) -> Vec<&str> {
        plan.delete
            .iter()
            .map(|e| e.path.to_str().unwrap_or(""))
            .collect()
    }

    #[test]
    fn nothing_is_deleted_while_the_folder_fits() {
        let plan = plan(vec![entry("a", Kind::DebugLog, 10, 5)], 100, 100);
        assert!(plan.delete.is_empty());
        assert_eq!(plan.total_after, 100);
    }

    #[test]
    fn kinds_go_in_order_and_only_as_much_as_needed() {
        let entries = vec![
            entry("sources", Kind::Sources, 50, 1),
            entry("old-runtime", Kind::OldRuntime, 300, 1),
            entry("log", Kind::DebugLog, 40, 1),
        ];
        let plan = plan(entries, 1000, 690);
        // 1000 - 300 = 700 is still over 690, so the log goes too; the sources stay.
        assert_eq!(names(&plan), vec!["old-runtime", "log"]);
        assert_eq!(plan.total_after, 660);
    }

    #[test]
    fn inside_a_kind_the_oldest_goes_first() {
        let entries = vec![
            entry("new", Kind::Subtitle, 10, 5),
            entry("old", Kind::Subtitle, 10, 500),
            entry("middle", Kind::Subtitle, 10, 50),
        ];
        let plan = plan(entries, 100, 80);
        assert_eq!(names(&plan), vec!["old", "middle"]);
    }

    #[test]
    fn when_everything_deletable_is_gone_the_folder_can_still_be_over() {
        let plan = plan(vec![entry("a", Kind::Sources, 10, 1)], 500, 100);
        assert_eq!(plan.total_after, 490);
        assert!(plan.still_over(100));
    }
}
