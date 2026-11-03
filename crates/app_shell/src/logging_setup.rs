use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::Subscriber;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter, Layer, Registry};

use crate::debug_logs::{category_filter, problems_filter, DebugLogs};

/// Folder (next to the main log file) that holds the optional debug logs.
const DEBUG_FOLDER: &str = "debug";

/// Starts logging: the normal log file always, and the debug log files that the returned handle
/// switches on and off. Falls back to stderr when the log file cannot be opened.
pub fn start_logging(log_file: &Path) -> DebugLogs {
    let directory = log_file.parent().map_or_else(
        || PathBuf::from(DEBUG_FOLDER),
        |parent| parent.join(DEBUG_FOLDER),
    );
    let debug = DebugLogs::new(directory);
    let (main_layer, open_error) = main_layer(log_file);
    let (category_layer, category_handle) = category_layer(&debug);
    let (problems_layer, problems_handle) = problems_layer(&debug);

    let started = Registry::default()
        .with(main_layer)
        .with(category_layer)
        .with(problems_layer)
        .try_init();
    debug.install_switch(Box::new(move |enabled| {
        if let Err(error) = category_handle.reload(category_filter(enabled)) {
            tracing::warn!(%error, "could not switch the debug log filter");
        }
        if let Err(error) = problems_handle.reload(problems_filter(enabled)) {
            tracing::warn!(%error, "could not switch the problems log filter");
        }
    }));
    if started.is_err() {
        tracing::debug!("a logger was already installed; keeping it");
    }
    if let Some(error) = open_error {
        tracing::warn!(%error, path = %log_file.display(), "cannot open log file, logging to stderr");
    }
    debug
}

/// The normal log: what the `RUST_LOG` variable asks for, else info and above.
fn main_layer(
    log_file: &Path,
) -> (
    Box<dyn Layer<Registry> + Send + Sync>,
    Option<std::io::Error>,
) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match open_log_file(log_file) {
        Ok(file) => (
            fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .with_filter(filter)
                .boxed(),
            None,
        ),
        Err(error) => (fmt::layer().with_filter(filter).boxed(), Some(error)),
    }
}

/// One file per part of the app; every call of a function that is instrumented also leaves a
/// line with how long it took.
fn category_layer<S>(debug: &DebugLogs) -> (impl Layer<S> + Send + Sync, reload::Handle<Targets, S>)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let (switch, handle) = reload::Layer::new(category_filter(false));
    let layer = fmt::layer()
        .with_ansi(false)
        .with_thread_names(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(debug.category_writer())
        .with_filter(switch);
    (layer, handle)
}

/// The file with every warning and error.
fn problems_layer<S>(debug: &DebugLogs) -> (impl Layer<S> + Send + Sync, reload::Handle<Targets, S>)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let (switch, handle) = reload::Layer::new(problems_filter(false));
    let layer = fmt::layer()
        .with_ansi(false)
        .with_thread_names(true)
        .with_writer(debug.problems_writer())
        .with_filter(switch);
    (layer, handle)
}

fn open_log_file(log_file: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(log_file)
}
