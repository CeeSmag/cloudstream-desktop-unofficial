# extension_runtime

Runs each loaded `.cs3` in its own out-of-process JVM sidecar and exposes them through
`ProviderGateway` (`PluginWorkerPool`). JSON-lines RPC over stdin/stdout; on crash or
timeout the process is killed and restarted on the next call, replaying remembered
startup calls (`load_extension`, settings).

## PluginWorkerPool

`ProviderGateway` with one sidecar JVM per plugin file:

- Routes by provider name and by link-search id; `extract` fans out across live workers;
  `take_added_repositories` merges.
- **Resident limit** (`plugin_worker_limit`, default 1 in `PluginWorkerPool::new`; `0` =
  unlimited): excess workers are parked (JVM stopped; providers stay listed) and revived
  on first call. Workers with an open link search are not parked.
- Trusted plugins use `SidecarLaunchConfig::without_os_fs_sandbox` (skip AppContainer /
  Landlock; keep heap and Job/rlimit). Untrusted keep the OS FS jail.
- Forks every child on a long-lived `SidecarSpawner` thread so Linux
  `PR_SET_PDEATHSIG(SIGKILL)` tracks the pool, not a short-lived UI worker thread.

## Sidecar process

| Piece | Role |
|---|---|
| `SidecarLaunchConfig` | `CS_SIDECAR_HOME` / `CS_SIDECAR_JAVA_HOME`, `-Xmx512m`, data dir, UTF-8 I/O; `with_data_dir` enables `os_fs_sandbox` |
| `SidecarProcessTransport` | Lazy start, kill-on-timeout, restart + startup replay |
| `RpcProviderGateway` | Maps RPC replies to `ProviderGateway` |

**Windows:** untrusted → AppContainer (fail-closed) + Job Object (memory cap, kill-on-close);
trusted / tests → Job Object only.

**Linux:** untrusted + data dir → Landlock allow-list + `RLIMIT_AS` + process group +
headless AWT; trusted + data dir → rlimit/pgid/headless without Landlock. Tests without
`data.dir` stay plain spawn.

**Public API:** see `src/lib.rs` (`PluginWorkerPool`, `SidecarLaunchConfig`,
`SidecarProcessTransport`, `RpcProviderGateway`, `SidecarSpawner`, `delete_sidecar_app_container`).

**Test:** `cargo test -p extension_runtime` (feature `test-support` for `fake_sidecar`).
