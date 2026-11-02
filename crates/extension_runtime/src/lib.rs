//! Loads and executes extensions behind the ExtensionRuntime trait.
// unsafe is allowed in this crate (runtime FFI); every block needs a // SAFETY: comment.

mod extension_load_error;
mod extension_runtime_trait;

pub use extension_load_error::ExtensionLoadError;
pub use extension_runtime_trait::ExtensionRuntime;
