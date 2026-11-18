//! Runs extensions out-of-process and exposes them through the `ProviderGateway` trait.
//! Unsafe FFI is confined to the Windows Job Object and AppContainer modules.
#![deny(unsafe_code)]

mod call_timeouts;
#[cfg(feature = "test-support")]
pub mod fake_link_search;
#[cfg(feature = "test-support")]
pub mod fake_search_runtime;
mod reply_wait;
mod rpc_message;
mod rpc_provider_gateway;
mod rpc_transport;
#[cfg(windows)]
mod sidecar_app_container_acl;
#[cfg(windows)]
mod sidecar_app_container_cmdline;
#[cfg(windows)]
mod sidecar_app_container_ffi;
#[cfg(windows)]
mod sidecar_app_container_launch;
#[cfg(windows)]
mod sidecar_app_container_profile;
#[cfg(windows)]
mod sidecar_job_object;
mod sidecar_launch_config;
mod sidecar_output;
mod sidecar_process_kill;
mod sidecar_process_start;
mod sidecar_process_transport;

pub use reply_wait::DeadlineHold;
pub use rpc_provider_gateway::RpcProviderGateway;
pub use rpc_transport::RpcTransport;
pub use sidecar_launch_config::SidecarLaunchConfig;
pub use sidecar_process_transport::SidecarProcessTransport;
