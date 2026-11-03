//! The debug logging switch: while on, everything our code logs is written to text files, one per
//! part of the app, plus one file with every warning and error. While off, the filters of those
//! files reject everything, so logging calls cost nothing but a level check.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use tracing::{Level, Metadata};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::MakeWriter;

use crate::log_category::{LogCategory, OWN_TARGETS, PROBLEMS_FILE};
use crate::log_file::{lock, LogFile, RoutedWriter, SharedLogFile};

/// Applies the switch to the logging filters; installed once the subscriber exists.
type FilterSwitch = Box<dyn Fn(bool) + Send + Sync>;

/// One file of the debug log folder, for the settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFileInfo {
    /// What the file covers.
    pub label: &'static str,
    /// File name inside the folder.
    pub file_name: &'static str,
    /// Size in bytes; `None` when the file does not exist yet.
    pub size_bytes: Option<u64>,
}

struct Inner {
    enabled: AtomicBool,
    directory: PathBuf,
    categories: HashMap<LogCategory, SharedLogFile>,
    problems: SharedLogFile,
    switch: OnceLock<FilterSwitch>,
}

/// Handle on the debug log files; cheap to clone.
#[derive(Clone)]
pub struct DebugLogs {
    inner: Arc<Inner>,
}

impl DebugLogs {
    /// Debug logs live in `directory`; nothing is created until logging is switched on.
    pub fn new(directory: PathBuf) -> Self {
        let categories = LogCategory::ALL
            .iter()
            .map(|category| {
                (
                    *category,
                    LogFile::shared(directory.join(category.file_name())),
                )
            })
            .collect();
        let problems = LogFile::shared(directory.join(PROBLEMS_FILE));
        Self {
            inner: Arc::new(Inner {
                enabled: AtomicBool::new(false),
                directory,
                categories,
                problems,
                switch: OnceLock::new(),
            }),
        }
    }

    /// The folder the files are in.
    pub fn directory(&self) -> &Path {
        &self.inner.directory
    }

    /// Whether debug logging is on.
    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::SeqCst)
    }

    /// Turns debug logging on or off. Files are opened before the filters let anything through
    /// and closed after they stop it.
    pub fn set_enabled(&self, enabled: bool) -> io::Result<()> {
        if enabled {
            self.open_all()?;
        }
        self.inner.enabled.store(enabled, Ordering::SeqCst);
        if let Some(switch) = self.inner.switch.get() {
            switch(enabled);
        }
        if !enabled {
            self.close_all();
        }
        tracing::info!(enabled, folder = %self.inner.directory.display(), "debug logging switched");
        Ok(())
    }

    /// The files with their sizes, problems file last.
    pub fn files(&self) -> Vec<LogFileInfo> {
        let mut files: Vec<LogFileInfo> = LogCategory::ALL
            .iter()
            .filter_map(|category| {
                let file = self.inner.categories.get(category)?;
                Some(LogFileInfo {
                    label: category.label(),
                    file_name: category.file_name(),
                    size_bytes: self.size_of(file),
                })
            })
            .collect();
        files.push(LogFileInfo {
            label: "Warnings and errors from everywhere",
            file_name: PROBLEMS_FILE,
            size_bytes: self.size_of(&self.inner.problems),
        });
        files
    }

    /// Writers that route each message to the file of its category.
    pub fn category_writer(&self) -> CategoryWriter {
        CategoryWriter(self.clone())
    }

    /// Writer of the file that collects every warning and error.
    pub fn problems_writer(&self) -> ProblemsWriter {
        ProblemsWriter(self.clone())
    }

    /// Registers how the filters are switched; done once, right after the subscriber is built.
    pub fn install_switch(&self, switch: FilterSwitch) {
        if self.inner.switch.set(switch).is_err() {
            tracing::warn!("debug log switch was installed twice");
        }
    }

    fn size_of(&self, file: &SharedLogFile) -> Option<u64> {
        lock(file).size()
    }

    fn open_all(&self) -> io::Result<()> {
        for file in self.inner.categories.values() {
            lock(file).open()?;
        }
        lock(&self.inner.problems).open()
    }

    fn close_all(&self) {
        for file in self.inner.categories.values() {
            lock(file).close();
        }
        lock(&self.inner.problems).close();
    }

    fn file_for(&self, target: &str) -> SharedLogFile {
        let category = LogCategory::for_target(target);
        self.inner
            .categories
            .get(&category)
            .map_or_else(|| Arc::clone(&self.inner.problems), Arc::clone)
    }
}

/// Picks the file of a message's category.
#[derive(Clone)]
pub struct CategoryWriter(DebugLogs);

impl<'a> MakeWriter<'a> for CategoryWriter {
    type Writer = RoutedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RoutedWriter(self.0.file_for("app"))
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        RoutedWriter(self.0.file_for(meta.target()))
    }
}

/// Always the file with warnings and errors.
#[derive(Clone)]
pub struct ProblemsWriter(DebugLogs);

impl<'a> MakeWriter<'a> for ProblemsWriter {
    type Writer = RoutedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RoutedWriter(Arc::clone(&self.0.inner.problems))
    }
}

/// What the category files record: everything of ours, warnings and errors of anyone else.
/// Off means nothing at all, which also lets the program skip building the log arguments.
pub fn category_filter(enabled: bool) -> Targets {
    if !enabled {
        return Targets::new().with_default(LevelFilter::OFF);
    }
    OWN_TARGETS
        .iter()
        .fold(Targets::new().with_default(Level::WARN), |targets, own| {
            targets.with_target(*own, Level::TRACE)
        })
}

/// What the problems file records: warnings and errors, from every module.
pub fn problems_filter(enabled: bool) -> Targets {
    let level = if enabled {
        LevelFilter::WARN
    } else {
        LevelFilter::OFF
    };
    Targets::new().with_default(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_enabled_until_switched_on() {
        let logs = DebugLogs::new(std::env::temp_dir().join("cs_debug_logs_new"));
        assert!(!logs.is_enabled());
        assert!(logs.files().iter().all(|file| file.size_bytes.is_none()));
    }

    #[test]
    fn every_category_and_the_problems_file_are_listed() {
        let logs = DebugLogs::new(std::env::temp_dir().join("cs_debug_logs_list"));
        let names: Vec<&str> = logs.files().iter().map(|file| file.file_name).collect();
        assert_eq!(names.len(), LogCategory::ALL.len() + 1);
        assert_eq!(names.last(), Some(&PROBLEMS_FILE));
    }
}
