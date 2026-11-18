# HANDOFF-LINUX.md — OS isolation for plugin sidecars on Linux

**Audience:** an AI (or human) continuing this work on a **Linux** machine.  
**Repo tip when written:** `c063ffe` (`v0.4.8` / `main`).  
**Windows work is done.** Do not re-implement AppContainer/Job Object; add a **Linux equivalent** behind the same spawn path.

Read first: [`AGENTS.md`](AGENTS.md) (Chunk 3 + plugin sandbox bullets), [`RULES.md`](RULES.md), then the Windows modules listed below. When docs and code disagree, **the code wins**.

---

## 1. Goal

Match the **threat model and fail-closed posture** Windows already has for production plugin workers:

| Capability | Windows today | Linux today | Linux target |
|---|---|---|---|
| One JVM per `.cs3` | Yes (`PluginWorkerPool`) | Yes (same code) | Keep |
| Bytecode / classloader sandbox | Yes (Kotlin) | Yes (same) | Keep |
| Install download gates + path jail | Yes | Yes | Keep |
| Memory cap on the process tree | Job Object **1536 MB** | None (`-Xmx512m` only) | Cap ~1536 MB, fail closed |
| Kill whole tree when worker drops | Job kill-on-close + `taskkill /T` | `kill -KILL` on one PID | Process-group / PDEATHSIG; reliable |
| Filesystem jail | AppContainer ACLs + optional JDK mirror | Full user FS (as app user) | Restrict to needed paths |
| Network | Allowed (`internetClient`) | Allowed | **Keep allowed** (plugins need HTTPS) |
| No process spawn from plugins | Bytecode denies `ProcessBuilder` | Same | Keep; OS layer is defense in depth |

**Do not claim** “AppContainer on Linux.” Claim concrete mechanisms (Landlock, rlimits, namespaces, bubblewrap, …) and only after they are wired and tested.

---

## 2. What Linux already has (do not redo)

These are **cross-platform** and already ship on Linux CI / AppImage:

### Rust host
- [`PluginWorkerPool`](crates/extension_runtime/src/plugin_worker_pool.rs) — one `SidecarProcessTransport` per loaded `.cs3`; routing, extract fan-out, shutdown.
- [`SidecarProcessTransport`](crates/extension_runtime/src/sidecar_process_transport.rs) — JSON-lines RPC, restart + `remember_for_restart`.
- [`start_sidecar`](crates/extension_runtime/src/sidecar_process_start.rs) — on non-Windows calls **`start_plain`**: plain `Command::spawn`, **no** job, **no** FS jail.
- Install hardening in `extension_repo` (`plugin_download_guard`, path sanitization).
- Wiring: [`app_shell::service_wiring::build_gateway`](crates/app_shell/src/service_wiring.rs) → `PluginWorkerPool::new(config)`.

### Kotlin sidecar (same on Linux)
- DEX→JVM, `PluginSecurityPolicy`, `RawSocketRemapper`, fail-closed `CompatPluginClassLoader`.
- [`ExtensionPathJail`](sidecar/cs-sidecar/src/main/kotlin/sidecar/ExtensionPathJail.kt) — load only under `<data>/extensions` when `cloudstream.data.dir` is `<data>/sidecar`.
- `-Xmx512m`, `-Dcloudstream.data.dir=…`, `NoOpPreferencesFactory` / file `PluginPermissionAPI` (prefs factory is mainly for Windows AppContainer registry denials; harmless on Linux).

### Already true in docs
> Linux: one JVM per plugin + bytecode sandbox; **no** OS FS / job isolation yet.

---

## 3. What is missing (your job)

Hook **production** Linux launches (when `JAVA_OPTS` contains `-Dcloudstream.data.dir=`, same gate Windows uses for AppContainer) into an OS sandbox. Tests / `fake_sidecar` without `data.dir` stay **plain spawn**.

### 3.1 Memory + tree lifetime (Job Object analogue)

Windows: [`sidecar_job_object.rs`](crates/extension_runtime/src/sidecar_job_object.rs) — 1536 MB job memory, 16 active processes, kill-on-job-close; Drop of the job ends the tree.

Linux needs something that:
1. Caps address space / memory high enough for StreamPlay-class DEX (Windows found **768 MB too low**; use **~1536 MB** budget, keep `-Xmx512m`).
2. On transport Drop / `shutdown` / discard, ends **java and helpers**, not only the shell script PID.
3. **Fails closed** if the cap cannot be applied in production mode.

Likely tools (pick and justify in code comments):
- `setrlimit(RLIMIT_AS)` and/or `RLIMIT_DATA` on the child after fork / via `pre_exec`
- `setpgid(0,0)` in the child + store PGID; kill with `kill(-pgid, SIGKILL)`
- `prctl(PR_SET_PDEATHSIG, SIGKILL)` so orphans die if the host dies
- Optional: cgroup v2 via systemd user (`systemd-run --user`) — powerful but flaky in AppImage / non-systemd; do not depend on it alone

Today [`sidecar_process_kill.rs`](crates/extension_runtime/src/sidecar_process_kill.rs) on Unix only `kill -KILL <pid>` and `Child::kill`. Strengthen this as part of the same change.

### 3.2 Filesystem isolation (AppContainer analogue)

Windows AppContainer profile `CloudStream.PluginSidecar.v1`:
- Modify: `<data>/sidecar`
- Read+execute: `<data>/extensions`, JDK, sidecar install home
- Mirror JDK under `<data>/sidecar/appcontainer/` when host path cannot be ACL’d (Program Files)

Linux target (minimum):
- Worker may **read** JDK + sidecar install + `<data>/extensions` (`.cs3`)
- Worker may **read/write** `<data>/sidecar` (cache, cloudflare exchange, plugin prefs files)
- Worker must **not** freely read/write the rest of `$HOME`, SSH keys, browser profiles, etc.

Practical options (prefer unprivileged, AppImage-friendly):

1. **Landlock** (kernel ≥ 5.13, no extra binary) — restrict FS access after spawn; good default.
2. **bubblewrap (`bwrap`)** — bind-mount only needed trees; strong but **optional** dependency; detect and fail closed or document fallback policy.
3. **User namespaces + mount namespace** — powerful; availability varies (`kernel.unprivileged_userns_clone`); treat as optional hardening, not the only path.

Network must remain available (HTTPS). Do not copy “no network” sandboxes.

### 3.3 Launch integration point

All Linux production entry is:

```text
start_sidecar → #[cfg(not(windows))] start_plain
```

Mirror the Windows split in [`sidecar_process_start.rs`](crates/extension_runtime/src/sidecar_process_start.rs):

```text
start_linux(config):
  if config has cloudstream.data.dir → start_sandboxed(config)  // NEW
  else → start_plain(config)   // fake_sidecar / tests
```

Keep Windows modules `#[cfg(windows)]`. Add Linux modules `#[cfg(target_os = "linux")]` (or `unix` only if macOS is intentionally same — **prefer linux-only** first; macOS has neither Landlock nor AppContainer).

### 3.4 Docs after the change

Update [`AGENTS.md`](AGENTS.md) Linux / threat-model bullets: replace “no OS FS isolation” with what you actually ship. Do not claim parity with AppContainer unless Landlock (or bwrap) is fail-closed in production.

---

## 4. Windows reference map (read these)

| Concern | Windows file(s) |
|---|---|
| Gate “production vs test spawn” | `sidecar_process_start.rs` (`config_requests_app_container`) |
| Launch + ACL + mirror + job | `sidecar_app_container_launch.rs` (+ `acl`, `attrs`, `cmdline`, `ffi`, `mirror`, `profile`) |
| Memory / kill-on-close | `sidecar_job_object.rs` |
| Pool / routing | `plugin_worker_pool.rs`, `plugin_worker_maps.rs` |
| Heap / JAVA_OPTS | `sidecar_launch_config.rs` (`with_data_dir`, `-Xmx512m`) |
| Discovery / bundled JRE | `app_shell/src/sidecar_discovery.rs` |

Unsafe is already allowed in those Windows modules (`#![allow(unsafe_code)]`). Linux Landlock/`pre_exec` will need the same **local** allow. Workspace `forbid(unsafe_code)` still applies elsewhere; do not scatter unsafe.

---

## 5. Suggested implementation plan

### Phase A — Job analogue (ship first)
1. `sidecar_linux_limits.rs` (name by role): after spawn / in `pre_exec`, apply memory rlimit (~1536 MB) + new process group + PDEATHSIG.
2. Hold a `LinuxGuards` on `RunningSidecar` (like `WindowsGuards`) so Drop kills the process group.
3. Fail closed when `data.dir` is set and limits cannot be applied.
4. Unit tests where possible; race tests if shared state; ignored live smoke with real JDK + sidecar if needed (same pattern as Windows AppContainer smoke).

### Phase B — FS jail
1. `sidecar_linux_landlock.rs` (or bwrap launcher): allow only  
   - RO: java home, sidecar home, `<data>/extensions`  
   - RW: `<data>/sidecar` (and maybe `/tmp` if the JVM requires it — **measure**; grant the minimum).
2. Apply after forks, before exec, or immediately in the child.
3. Fail closed in production if Landlock is unsupported **or** document an explicit env escape hatch (prefer fail closed to match Windows).
4. Confirm Cloudflare file exchange under `<data>/sidecar/cloudflare` still works (host watches that folder).

### Phase C — Polish
1. Logging: one info line per worker (“linux sandbox: landlock+rlimit …”) analogous to `sidecar started inside AppContainer`.
2. AGENTS.md + this handoff status.
3. CI: Linux job already compiles Unix paths; add tests that compile on `rust-linux`. Live Landlock tests may need `#[ignore]` or a capability check.

**Out of scope unless asked:** macOS sandbox, rewriting Kotlin sandbox, changing RPC protocol, per-plugin Cloudflare folders, lowering heap after DEX.

---

## 6. Project constraints (do not violate)

- Files ≤ 300 lines; functions ≤ 40 lines; ≤ 4 args; no unwrap/expect/panic/indexing_slicing (`RULES.md` / clippy workspace).
- Shared mutable maps → **race tests** in the same change (`AGENTS.md` Recipe 6).
- Never log full stream URLs / secrets (`core_model::redact_url`).
- Gates: `just gates` (or at least fmt, clippy workspace, `cargo test -p extension_runtime`, Linux CI compile).
- Fail closed for production sandbox setup failures — do not silently run unconstrained workers when `cloudstream.data.dir` is set.

---

## 7. How to verify on the Linux laptop

### Dev run
- JDK **21**, built sidecar (`scripts/setup_sidecar` / Gradle install dist), `libmpv`.
- Env: `CS_DATA_DIR`, `CS_SIDECAR_HOME`, `CS_SIDECAR_JAVA_HOME`, `CS_LIBMPV_PATH` as needed.
- Launch Tauri or `cargo run -p desktop_tauri` / AppImage as you normally would on Linux.

### Expected log / process signals
- Two installed plugins → **two** `java` processes (already true with `PluginWorkerPool`).
- New: log lines that the Linux sandbox applied (rlimit / landlock / bwrap).
- Load StreamPlay + another plugin; uninstall one → only that worker’s tree dies; the other keeps answering `search` / `main_page`.
- After Drop/shutdown, no leftover `java` for that worker.

### Negative checks
- With sandbox on, a worker should not be able to read e.g. `~/.ssh/id_rsa` (Landlock/bwrap proof test).
- Without `data.dir` (fake_sidecar tests), spawn stays unconstrained so unit tests keep working.

### Memory
- Large `.cs3` DEX load must not OOM at a too-low rlimit (validate against StreamPlay-sized plugins; Windows needed 1536 MB job).

---

## 8. Threat model reminder (for release notes / AGENTS)

Plugins are **untrusted community code** with intentional public HTTPS egress. Isolation aims to block:
- host filesystem escape,
- unconstrained RAM / runaway process trees,
- easy process spawn (bytecode + OS),

**not** to make arbitrary `.cs3` “perfectly safe.” Per-plugin JVMs already limit blast radius across plugins; OS jailing limits blast radius against the rest of the user account.

---

## 9. Done when

- [ ] Production Linux sidecars (with `cloudstream.data.dir`) get memory + kill-on-drop tree limits (fail closed).
- [ ] Production Linux sidecars get an FS allow-list (Landlock and/or bwrap), fail closed or explicitly documented.
- [ ] `fake_sidecar` / no-`data.dir` path unchanged for tests.
- [ ] Race/unit tests + Linux compile in CI green.
- [ ] `AGENTS.md` Linux sandbox bullet updated to match reality.
- [ ] Manual: two plugins, uninstall one, Cloudflare exchange still works, StreamPlay still loads.

---

## 10. Quick “start here” checklist for the Linux agent

1. Read `sidecar_process_start.rs` (`start_plain` vs Windows branch) and `sidecar_job_object.rs`.
2. Implement Phase A behind `#[cfg(target_os = "linux")]`; wire through `start_sidecar`.
3. Prove kill-on-drop and memory fail-closed with tests.
4. Implement Phase B Landlock allow-list; prove `~/.ssh` denial + plugin load success.
5. Update AGENTS.md; run gates; commit.

Do **not** port Win32 AppContainer APIs. Port the **behavior**.
