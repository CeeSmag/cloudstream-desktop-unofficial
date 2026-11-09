//! Runs extensions out-of-process and exposes them through the `ProviderGateway` trait.
#![forbid(unsafe_code)]

mod call_timeouts;
#[cfg(feature = "test-support")]
pub mod fake_link_search;
#[cfg(feature = "test-support")]
pub mod fake_search_runtime;
mod reply_wait;
mod rpc_message;
mod rpc_provider_gateway;
mod rpc_transport;
mod sidecar_launch_config;
mod sidecar_output;
mod sidecar_process_transport;

pub use reply_wait::DeadlineHold;
pub use rpc_provider_gateway::RpcProviderGateway;
pub use rpc_transport::RpcTransport;
pub use sidecar_launch_config::SidecarLaunchConfig;
pub use sidecar_process_transport::SidecarProcessTransport;
