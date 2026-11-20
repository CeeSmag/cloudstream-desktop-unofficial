//! Runs extensions out-of-process and exposes them through the `ProviderGateway` trait.
//! Unsafe FFI is confined to the Windows Job/AppContainer and Linux Landlock/rlimit modules.
#![deny(unsafe_code)]

mod acl_session_cache;
mod call_timeouts;
#[cfg(feature = "test-support")]
pub mod fake_link_search;
#[cfg(feature = "test-support")]
pub mod fake_search_runtime;
mod plugin_worker_limit;
mod plugin_worker_maps;
mod plugin_worker_pool;
mod plugin_worker_resident;
mod reply_wait;
mod rpc_message;
mod rpc_provider_gateway;
mod rpc_transport;
#[cfg(windows)]
mod sidecar_app_container_acl;
#[cfg(windows)]
mod sidecar_app_container_attrs;
#[cfg(windows)]
mod sidecar_app_container_cmdline;
#[cfg(windows)]
mod sidecar_app_container_ffi;
#[cfg(windows)]
mod sidecar_app_container_launch;
#[cfg(all(windows, test))]
mod sidecar_app_container_launch_tests;
#[cfg(windows)]
mod sidecar_app_container_mirror;
#[cfg(windows)]
mod sidecar_app_container_profile;
#[cfg(windows)]
mod sidecar_job_object;
mod sidecar_launch_config;
#[cfg(target_os = "linux")]
mod sidecar_linux_landlock;
#[cfg(all(target_os = "linux", test))]
mod sidecar_linux_landlock_tests;
#[cfg(target_os = "linux")]
mod sidecar_linux_limits;
#[cfg(target_os = "linux")]
mod sidecar_linux_paths;
mod sidecar_output;
mod sidecar_process_kill;
mod sidecar_process_start;
mod sidecar_process_transport;
mod sidecar_spawn_mode;
mod sidecar_spawn_thread;

pub use plugin_worker_limit::{format_limit, parse_limit};
pub use plugin_worker_pool::PluginWorkerPool;
pub use reply_wait::DeadlineHold;
pub use rpc_provider_gateway::RpcProviderGateway;
pub use rpc_transport::RpcTransport;
pub use sidecar_launch_config::SidecarLaunchConfig;
pub use sidecar_process_transport::SidecarProcessTransport;
pub use sidecar_spawn_thread::SidecarSpawner;

/// Removes the Windows AppContainer profile created for the plugin sidecar, if any.
/// On non-Windows platforms this is a no-op.
pub fn delete_sidecar_app_container() -> Result<(), extension_api::GatewayError> {
    #[cfg(windows)]
    {
        sidecar_app_container_profile::delete_sidecar_container()
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}
