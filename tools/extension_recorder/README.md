# CloudStream Extension Response Recorder (High-Performance Parallel Edition)

An automated parity audit tool for CloudStream extensions and repositories against `cs-sidecar`.

## Key Features & Speed Optimizations
1. **Multi-Worker Parallelism**: Runs **4 to 8 concurrent JVM `cs-sidecar` worker processes** simultaneously. Audits ~580+ plugins in ~1 to 2 minutes instead of 15+ minutes.
2. **Parallel Repository Indexing**: Crawls and parses all 34 repository manifests concurrently using a 16-thread pool.
3. **Trusted Mode by Default**: Bypasses the desktop unofficial client's overly-strict bytecode scanner (`PluginSecurityVerifier`) so community plugins using `HttpURLConnection`, `Executors`, and custom DNS work as intended on Android.
4. **URL Encoding Fix**: Automatically cleans and encodes spaces/symbols in URLs (e.g. `Aia2tv 2.cs3`).
5. **Incremental Progress Saving**: Writes results continuously so no data is lost if interrupted.
6. **Excessive Diagnostic Logging**: Captures full millisecond traces, Python call stacks, raw RPC payloads, and complete JVM standard error logs for all failures in `logs/error-logs/`.

---

## Usage

### Run Default Parallel Audit (4 Workers, Trusted)
From the directory:
```powershell
.\record_extensions.cmd
```

Or from anywhere with full paths:
```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "<repo>\tools\extension_recorder\record_extensions.ps1"
```

### High-Speed 8-Worker Scan
```powershell
.\record_extensions.cmd -Workers 8
```

### Quick Test Run (First 10 Plugins)
```powershell
.\record_extensions.cmd -MaxPlugins 10
```

### Parameters
- `-Workers <int>`: Number of concurrent sidecar worker processes (default: `4`).
- `-RepoUrl <string>`: Root repository URL (default: MegaRepo).
- `-MaxPlugins <int>`: Cap number of plugins to test (default: `0` = test all).
- `-TimeoutSec <int>`: Timeout per plugin load (default: `45` seconds).
- `-SkipDownload`: Reuses cached `.cs3` files without re-downloading.
- `-Untrusted`: Enables strict bytecode verification (blocks `HttpURLConnection` and threads).

---

## Output Structure

All outputs are saved in `tools/extension_recorder/`:

- `replies/discovered_repositories.json`: 34 repositories discovered from MegaRepo.
- `replies/all_plugins_catalog.json`: Full catalog of 580+ plugins.
- `replies/results.json`: Complete machine-readable audit results.
- `replies/summary.md`: Human-readable summary table and leaderboard of missing Android stubs.
- `replies/details/<plugin_name>_<version>.json`: Individual per-plugin JSON replies.
- `logs/error-logs/`:
  - `latest_errors.log`: Complete stack traces and diagnostics.
  - `plugin_failures/<plugin_name>_<version>.log`: Individual error logs per failing plugin.
  - `recorder_trace_<timestamp>.log`: Full microsecond communication trace.
