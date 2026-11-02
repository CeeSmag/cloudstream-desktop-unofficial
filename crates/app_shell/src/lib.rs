//! Wiring only: builds the real implementations; the `cloudstream` binary and the end-to-end tests
//! share this code.
#![forbid(unsafe_code)]

mod app_paths;
mod logging_setup;
mod service_wiring;
mod sidecar_discovery;
mod unavailable_gateway;

pub use app_paths::AppPaths;
pub use logging_setup::start_logging;
pub use service_wiring::{build_services, ServiceOptions};
