//! Keeping the folder small: removing expired, broken and surplus files, measuring it, emptying it.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::cache_limits::Verdict;
use crate::stored_file::{read_stored, TEMPORARY_MARK};
use crate::title_cache::TitleCache;

/// A temporary file younger than this may belong to a save in progress (in another process), so
/// it is left alone.
const LEFTOVER_AFTER: Duration = Duration::from_secs(60);

/// What a cleanup did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PruneReport {
    /// Files removed because they were saved too long ago.
    pub expired: usize,
    /// Files removed because they were saved before the viewer switched provider.
    pub invalidated: usize,
    /// Files removed because they could not be understood (empty, truncated, other version, too big).
    pub corrupt: usize,
    /// Temporary files of crashed saves that were removed.
    pub leftovers: usize,
    /// Files removed because more titles were saved than the limit.
    pub evicted: usize,
    /// Titles still saved afterwards.
    pub kept: usize,
    /// Size of what is still saved.
    pub bytes: u64,
}

struct Keeper {
    saved_at: u64,
    path: PathBuf,
    bytes: u64,
}

impl TitleCache {
    /// Removes expired, unusable and surplus files (the oldest saves go first) and leftovers of
    /// crashed saves.
    pub fn prune(&self) -> PruneReport {
        let _lock = self.write_lock();
        self.prune_locked()
    }

    /// [`TitleCache::prune`] for a caller that holds the write lock.
    pub(crate) fn prune_locked(&self) -> PruneReport {
        let mut report = PruneReport::default();
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return report;
        };
        let mut keepers = Vec::new();
        for entry in entries.flatten() {
            self.sort_out(entry.path(), &mut report, &mut keepers);
        }
        self.evict_surplus(&mut keepers, &mut report);
        report.kept = keepers.len();
        report.bytes = keepers.iter().map(|keeper| keeper.bytes).sum();
        log_report(&report);
        report
    }

    /// Decides what happens to one file of the folder.
    fn sort_out(&self, path: PathBuf, report: &mut PruneReport, keepers: &mut Vec<Keeper>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.contains(TEMPORARY_MARK) {
            if is_old(&path) && remove(&path) {
                report.leftovers += 1;
            }
            return;
        }
        if !name.ends_with(".json") {
            return;
        }
        match read_stored(&path, self.limits.max_file_bytes) {
            Ok(stored) => match self.verdict(stored.saved_at) {
                Verdict::Expired => report.expired += usize::from(remove(&path)),
                Verdict::Invalidated => report.invalidated += usize::from(remove(&path)),
                Verdict::Usable(..) => keepers.push(Keeper {
                    saved_at: stored.saved_at,
                    bytes: fs::metadata(&path).map_or(0, |meta| meta.len()),
                    path,
                }),
            },
            Err(miss) if miss.is_discardable() => report.corrupt += usize::from(remove(&path)),
            Err(_) => {}
        }
    }

    fn evict_surplus(&self, keepers: &mut Vec<Keeper>, report: &mut PruneReport) {
        keepers.sort_by(|a, b| {
            b.saved_at
                .cmp(&a.saved_at)
                .then_with(|| a.path.cmp(&b.path))
        });
        while keepers.len() > self.limits.max_titles {
            let Some(oldest) = keepers.pop() else { break };
            report.evicted += usize::from(remove(&oldest.path));
        }
    }

    /// The size of everything saved, temporary files included.
    pub fn size_bytes(&self) -> u64 {
        fs::read_dir(&self.folder)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.metadata().ok())
                    .filter(|meta| meta.is_file())
                    .map(|meta| meta.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Removes every saved title; returns how many files went.
    pub fn clear(&self) -> usize {
        let _lock = self.write_lock();
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return 0;
        };
        let removed = entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter(|entry| remove(&entry.path()))
            .count();
        tracing::info!(removed, "saved sources cleared");
        removed
    }
}

fn is_old(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= LEFTOVER_AFTER)
}

fn remove(path: &std::path::Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "cannot remove a saved-sources file");
            false
        }
    }
}

fn log_report(report: &PruneReport) {
    let removed =
        report.expired + report.invalidated + report.corrupt + report.leftovers + report.evicted;
    if removed > 0 {
        tracing::info!(
            expired = report.expired,
            invalidated = report.invalidated,
            corrupt = report.corrupt,
            leftovers = report.leftovers,
            evicted = report.evicted,
            kept = report.kept,
            bytes = report.bytes,
            "saved sources cleaned up"
        );
    } else {
        tracing::debug!(
            kept = report.kept,
            bytes = report.bytes,
            "saved sources checked"
        );
    }
}

#[cfg(test)]
#[path = "title_prune_tests.rs"]
mod tests;
