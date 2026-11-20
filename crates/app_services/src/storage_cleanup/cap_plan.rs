//! Pure storage-cap eviction planning: cheapest category first, oldest first within each kind.

use std::path::PathBuf;
use std::time::SystemTime;

/// Removable categories ordered from cheapest to re-create to most noticeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvictionKind {
    /// Older unpacked portable `runtime-*` folders.
    OldRuntime,
    /// Stale update download attempts under `updates/`.
    UpdateAttempt,
    /// Rotated or inactive debug log files under `logs/debug/`.
    DebugLog,
    /// Cached poster artwork files under `artwork/`.
    Artwork,
    /// Cached downloaded subtitles under `subtitles/`.
    Subtitle,
    /// Cached stream source lists under `sources/`.
    Sources,
    /// Cloudflare verification Chromium cache directories (never cookies).
    BrowserCache,
}

/// One candidate file or directory that storage-cap enforcement may remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovableItem {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Eviction priority category.
    pub kind: EvictionKind,
    /// Bytes reclaimed if this item is removed.
    pub bytes: u64,
    /// Last-modified timestamp (oldest is removed first).
    pub modified: SystemTime,
    /// Whether `path` is a directory.
    pub is_dir: bool,
}

/// Selects the smallest prefix of `items` (cheapest kind first, then oldest) needed to bring
/// `total_bytes` down to `cap_bytes` or below.
pub fn plan_evictions(
    total_bytes: u64,
    cap_bytes: u64,
    mut items: Vec<RemovableItem>,
) -> Vec<RemovableItem> {
    if total_bytes <= cap_bytes {
        return Vec::new();
    }
    items.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.modified.cmp(&right.modified))
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut remaining = total_bytes;
    let mut chosen = Vec::new();
    for item in items {
        if remaining <= cap_bytes {
            break;
        }
        remaining = remaining.saturating_sub(item.bytes);
        chosen.push(item);
    }
    chosen
}
