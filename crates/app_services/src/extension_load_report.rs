//! What loading the installed extensions reports while it runs and after it ends.

use core_model::ProviderInfo;

/// What happened when the installed extensions were loaded into the runtime.
#[derive(Debug, Default)]
pub struct ExtensionLoadReport {
    /// Providers that are now available.
    pub providers: Vec<ProviderInfo>,
    /// Extensions that failed to load, with a readable reason; the rest still loaded.
    pub failures: Vec<(String, String)>,
}

/// Where the loading of installed extensions stands, reported before each one starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadProgress {
    /// Extensions already done, whether they loaded or failed.
    pub finished: usize,
    /// Extensions to load in all.
    pub total: usize,
    /// Name of the extension being loaded now; empty once all are done.
    pub current: String,
}
