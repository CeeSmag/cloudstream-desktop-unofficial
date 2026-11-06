//! The file format of one title's saved sources, and reading and writing it safely.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::cached_title::CachedTitle;
use crate::title_cache_error::TitleCacheError;

/// The format written now. A file with another number is discarded, never guessed at.
pub(crate) const FORMAT_VERSION: u32 = 2;

/// Marks a file that is still being written (or was left behind by a crash).
pub(crate) const TEMPORARY_MARK: &str = ".tmp";

/// What is in the file besides the title's content.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StoredFile {
    pub(crate) version: u32,
    /// Seconds since 1970 when the sources were saved.
    pub(crate) saved_at: u64,
    #[serde(flatten)]
    pub(crate) title: CachedTitle,
}

#[derive(Deserialize)]
struct VersionOnly {
    version: u32,
}

/// Why a file is not usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// Nothing was saved for the title.
    NotSaved,
    /// Saved, but too long ago; the file was deleted.
    Expired,
    /// Saved before the viewer switched provider; the file was deleted.
    Invalidated,
    /// The file exists but cannot be read (permissions, a lock); it is left alone.
    Unreadable,
    /// The file is empty, truncated or not what it should be; it was deleted.
    Corrupt,
    /// Written by another version of the app; it was deleted.
    WrongVersion(u32),
    /// Bigger than the limit; it was deleted.
    TooLarge,
}

impl Miss {
    /// Whether the file is broken for good and should be removed.
    pub(crate) fn is_discardable(self) -> bool {
        !matches!(self, Self::NotSaved | Self::Unreadable)
    }
}

/// Reads and checks the file at `path`, which may be at most `max_bytes` big.
pub(crate) fn read_stored(path: &Path, max_bytes: u64) -> Result<StoredFile, Miss> {
    let meta = fs::metadata(path).map_err(|error| miss_for(&error))?;
    if meta.len() > max_bytes {
        return Err(Miss::TooLarge);
    }
    let bytes = fs::read(path).map_err(|error| miss_for(&error))?;
    let probe: VersionOnly = serde_json::from_slice(&bytes).map_err(|_| Miss::Corrupt)?;
    if probe.version != FORMAT_VERSION {
        return Err(Miss::WrongVersion(probe.version));
    }
    serde_json::from_slice(&bytes).map_err(|_| Miss::Corrupt)
}

fn miss_for(error: &std::io::Error) -> Miss {
    if error.kind() == std::io::ErrorKind::NotFound {
        Miss::NotSaved
    } else {
        Miss::Unreadable
    }
}

/// Writes `stored` to `path` through a temporary file and a rename, so a crash or a second
/// writer never leaves a half-written file under the real name. Nothing is written when the file
/// would be over `max_bytes`. Returns the bytes written.
pub(crate) fn write_stored(
    path: &Path,
    stored: &StoredFile,
    counter: &AtomicU64,
    max_bytes: u64,
) -> Result<u64, TitleCacheError> {
    let bytes = serde_json::to_vec(stored).map_err(TitleCacheError::Encode)?;
    if bytes.len() as u64 > max_bytes {
        return Err(TitleCacheError::TooLarge {
            bytes: bytes.len() as u64,
            limit: max_bytes,
        });
    }
    let aside = path.with_extension(format!(
        "json{TEMPORARY_MARK}{}-{}",
        std::process::id(),
        counter.fetch_add(1, Ordering::SeqCst)
    ));
    fs::write(&aside, &bytes).map_err(|error| TitleCacheError::disk("write", &aside, error))?;
    if let Err(error) = fs::rename(&aside, path) {
        if let Err(cleanup) = fs::remove_file(&aside) {
            tracing::debug!(%cleanup, "could not remove the temporary file after a failed save");
        }
        return Err(TitleCacheError::disk("replace", path, error));
    }
    Ok(bytes.len() as u64)
}
