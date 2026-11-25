//! Wiring only: builds the real implementations; the `cloudstream` binary and the end-to-end tests
//! share this code.
#![forbid(unsafe_code)]

mod app_paths;
mod cleanup_wiring;
mod cloudflare_wiring;
mod data_wipe;
mod debug_logs;
mod log_category;
mod log_file;
mod logging_setup;
mod service_wiring;
mod sidecar_discovery;
mod stream_proxy_router;
mod system_probe;
mod unavailable_gateway;
mod webview_wiring;
mod wipe_helper;

pub use app_paths::AppPaths;
pub use cleanup_wiring::start_storage_cleanup;
pub use cloudflare_wiring::start_cloudflare_watch;
pub use data_wipe::{request_wipe, running_folder_name, wipe_if_requested, RetryPlan, WipeOutcome};
pub use debug_logs::{DebugLogs, LogFileInfo};
pub use logging_setup::start_logging;
pub use service_wiring::{build_services, ServiceOptions};
pub use webview_wiring::{build_webview, PRETEND_VARIABLE};
pub use wipe_helper::{spawn_wipe_helper, validate_wipe_target};
