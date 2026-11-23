//! Passes kept on disk, one file per scope, in a folder no plugin can reach.
//!
//! The folder (`<data>/cloudflare_passes`) is outside `<data>/sidecar`, the only part of the data folder an
//! untrusted plugin's sandbox may read (Landlock on Linux, AppContainer on Windows). Every write is a whole
//! file renamed into place, so a reader (another app instance, or this one after a crash) sees an old file or
//! a new one, never half of one. Changes take an exclusive lock on `.lock` (with a time limit), so two app
//! instances cannot lose each other's rejections. A file that cannot be read back is logged by name and
//! removed: a saved pass is only ever an optimisation, the check window is always the fallback.

use std::collections::hash_map::RandomState;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::hash::BuildHasher;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::pass_book::{PassBook, PassStoreError, PruneReport, MAX_PASSES};
use crate::pass_file::{decode_scope, encode_scope, scope_file_name};
use crate::pass_ids::PassId;
use crate::pass_scope::{scopes_over_cap, SaveOutcome, ScopeEntry};
use crate::saved_pass::SavedPass;

/// A scope file larger than this is not read (a real one is a few kilobytes).
pub const MAX_PASS_FILE_BYTES: u64 = 64 * 1024;

/// How long a change waits for another instance to release the lock.
const LOCK_WAIT: Duration = Duration::from_secs(2);

/// How often a waiting change tries the lock again.
const LOCK_RETRY: Duration = Duration::from_millis(5);

/// A temporary file older than this was left by a writer that stopped.
const STALE_TEMP_AGE: Duration = Duration::from_secs(60);

const LOCK_FILE: &str = ".lock";

/// Passes kept in a folder.
#[derive(Debug)]
pub struct DiskPassBook {
    dir: PathBuf,
    cap: usize,
    /// Random per book, so two books (two app instances, or two in one process) never pick one name.
    writer: u64,
    temp_names: AtomicU64,
}

impl DiskPassBook {
    /// Opens (and creates, readable by this user only) the folder `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, PassStoreError> {
        Self::open_with_cap(dir, MAX_PASSES)
    }

    /// Opens `dir`, keeping at most `cap` scopes.
    pub fn open_with_cap(dir: impl Into<PathBuf>, cap: usize) -> Result<Self, PassStoreError> {
        let dir = dir.into();
        create_private_dir(&dir).map_err(|source| PassStoreError::Io {
            operation: "create the pass folder",
            path: dir.clone(),
            source,
        })?;
        Ok(Self {
            dir,
            cap,
            writer: RandomState::new().hash_one(std::process::id()),
            temp_names: AtomicU64::new(0),
        })
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every scope file and what it holds; unreadable files are removed.
    fn scopes(&self) -> Vec<(PathBuf, ScopeEntry)> {
        let Ok(listing) = fs::read_dir(&self.dir) else {
            tracing::warn!(dir = %self.dir.display(), "cannot list the saved Cloudflare passes");
            return Vec::new();
        };
        listing
            .flatten()
            .map(|item| item.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| self.read_scope(&path).map(|entry| (path, entry)))
            .collect()
    }

    fn read_scope(&self, path: &Path) -> Option<ScopeEntry> {
        let text = match read_capped(path) {
            Ok(Some(text)) => text,
            Ok(None) => {
                discard(path, "the file is larger than a pass file can be");
                return None;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                tracing::warn!(file = %path.display(), %error, "cannot read a saved Cloudflare pass");
                return None;
            }
        };
        match decode_scope(&text) {
            Ok(entry) if path.file_name() == Some(scope_file_name(&entry.scope).as_ref()) => {
                Some(entry)
            }
            Ok(_) => {
                discard(path, "the file's name does not match its scope");
                None
            }
            Err(reason) => {
                discard(path, &reason.to_string());
                None
            }
        }
    }

    fn write_scope(&self, entry: &ScopeEntry) -> Result<(), PassStoreError> {
        let path = self.dir.join(scope_file_name(&entry.scope));
        if entry.is_empty() {
            return remove_if_present(&path);
        }
        let temp = self.dir.join(format!(
            ".{}.{:x}.{}.tmp",
            scope_file_name(&entry.scope),
            self.writer,
            self.temp_names.fetch_add(1, Ordering::Relaxed)
        ));
        let io_error = |operation, path: &Path| {
            let path = path.to_owned();
            move |source| PassStoreError::Io {
                operation,
                path,
                source,
            }
        };
        write_private(&temp, encode_scope(entry).as_bytes())
            .map_err(io_error("write a saved pass", &temp))?;
        fs::rename(&temp, &path).map_err(|source| {
            let _ = fs::remove_file(&temp); // best effort: the next prune removes it otherwise
            io_error("publish a saved pass", &path)(source)
        })
    }

    /// Runs `change` while holding the book's lock.
    fn locked<T>(
        &self,
        change: impl FnOnce() -> Result<T, PassStoreError>,
    ) -> Result<T, PassStoreError> {
        let path = self.dir.join(LOCK_FILE);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|source| PassStoreError::Io {
                operation: "open the pass book's lock",
                path: path.clone(),
                source,
            })?;
        acquire(&lock, &path)?;
        let result = change();
        // Dropping the file releases the lock, also if `unlock` fails.
        let _ = lock.unlock();
        result
    }

    fn remove_stale_temps(&self) {
        let Ok(listing) = fs::read_dir(&self.dir) else {
            return;
        };
        for path in listing.flatten().map(|item| item.path()) {
            let is_temp = path.extension().is_some_and(|ext| ext == "tmp");
            let age = fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok());
            if is_temp && age.is_some_and(|age| age > STALE_TEMP_AGE) {
                let _ = fs::remove_file(&path); // best effort: tried again at the next prune
            }
        }
    }

    fn evict(&self, scopes: &[(PathBuf, ScopeEntry)]) -> Result<usize, PassStoreError> {
        let listed: Vec<(PathBuf, Option<SystemTime>)> = scopes
            .iter()
            .map(|(path, entry)| (path.clone(), entry.obtained_at()))
            .collect();
        let evicted = scopes_over_cap(&listed, self.cap);
        for path in &evicted {
            remove_if_present(path)?;
        }
        Ok(evicted.len())
    }
}

impl PassBook for DiskPassBook {
    fn live_passes(&self, now: SystemTime) -> Vec<SavedPass> {
        self.scopes()
            .into_iter()
            .filter_map(|(_, entry)| entry.live_pass(now).cloned())
            .collect()
    }

    fn save(&self, pass: SavedPass, now: SystemTime) -> Result<SaveOutcome, PassStoreError> {
        self.locked(|| {
            let path = self.dir.join(scope_file_name(pass.scope()));
            let mut entry = self
                .read_scope(&path)
                .unwrap_or_else(|| ScopeEntry::empty(pass.scope().clone()));
            let outcome = entry.save(pass, now);
            if matches!(outcome, SaveOutcome::Saved { .. }) {
                self.write_scope(&entry)?;
                self.evict(&self.scopes())?;
            }
            Ok(outcome)
        })
    }

    fn reject(&self, id: &PassId, _now: SystemTime) -> Result<bool, PassStoreError> {
        self.locked(|| {
            for (_, mut entry) in self.scopes() {
                if entry.reject(id) {
                    self.write_scope(&entry)?;
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    fn prune(&self, now: SystemTime) -> Result<PruneReport, PassStoreError> {
        self.locked(|| {
            self.remove_stale_temps();
            let mut emptied = 0;
            let mut kept = Vec::new();
            for (path, mut entry) in self.scopes() {
                if entry.prune(now) {
                    self.write_scope(&entry)?;
                }
                if entry.is_empty() {
                    emptied += 1;
                } else {
                    kept.push((path, entry));
                }
            }
            let evicted = self.evict(&kept)?;
            Ok(PruneReport { emptied, evicted })
        })
    }
}

/// Takes the exclusive lock, waiting at most [`LOCK_WAIT`].
fn acquire(lock: &File, path: &Path) -> Result<(), PassStoreError> {
    let started = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) if started.elapsed() < LOCK_WAIT => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(TryLockError::WouldBlock) => {
                return Err(PassStoreError::LockTimeout {
                    path: path.to_owned(),
                    waited: started.elapsed(),
                })
            }
            Err(TryLockError::Error(source)) => {
                return Err(PassStoreError::Io {
                    operation: "lock the pass book",
                    path: path.to_owned(),
                    source,
                })
            }
        }
    }
}

/// The file's text, or None when it is larger than [`MAX_PASS_FILE_BYTES`].
fn read_capped(path: &Path) -> io::Result<Option<String>> {
    let mut text = String::new();
    File::open(path)?
        .take(MAX_PASS_FILE_BYTES + 1)
        .read_to_string(&mut text)?;
    let fits = u64::try_from(text.len()).is_ok_and(|len| len <= MAX_PASS_FILE_BYTES);
    Ok(fits.then_some(text))
}

/// Removes a file that cannot be used; logged by name and reason, never by content.
fn discard(path: &Path, reason: &str) {
    tracing::warn!(file = %path.display(), %reason, "removing a saved Cloudflare pass that cannot be read");
    if let Err(error) = fs::remove_file(path) {
        tracing::warn!(file = %path.display(), %error, "could not remove an unreadable saved pass");
    }
}

fn remove_if_present(path: &Path) -> Result<(), PassStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(PassStoreError::Io {
            operation: "remove a saved pass",
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    // On Windows the folder inherits the user profile's ACL; the plugin AppContainer is granted nothing here.
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
#[path = "disk_pass_book_tests.rs"]
mod tests;
