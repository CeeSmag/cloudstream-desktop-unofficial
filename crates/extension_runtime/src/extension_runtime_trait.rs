use std::path::Path;

use crate::ExtensionLoadError;

/// Seam between the app and whichever engine executes extensions
/// (Wasm host or JVM sidecar). The app only ever depends on this trait.
pub trait ExtensionRuntime {
    /// Loads the extension at `path` and returns its display name.
    fn load_extension(&mut self, path: &Path) -> Result<String, ExtensionLoadError>;

    /// Unloads a previously loaded extension, calling its unload hook.
    fn unload_extension(&mut self, name: &str);
}
