# CloudStream Extension Response Recorder

Windows audit tool: crawls MegaRepo (or another seed `repo.json`) via `cs-sidecar`, discovers linked repositories, downloads `.cs3` plugins, and test-loads them across concurrent sidecar workers. Records RPC replies and JVM stderr for parity / stub gaps.

**Requires:** Python 3, JDK 21, and a built `cs-sidecar` install (`bin/cs-sidecar.bat`). Auto-detects `CS_SIDECAR_HOME` / `CS_SIDECAR_JAVA_HOME` (or `JAVA_HOME`), else `C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar`.

## Features

- **Parallel workers** — default 4 concurrent JVM sidecars (`-Workers`); up to whatever the host can run.
- **Parallel repo indexing** — up to 16 threads crawl discovered repository manifests.
- **Trusted mode by default** — skips strict bytecode verify so plugins using `HttpURLConnection` / threads load like Android verified installs; use `-Untrusted` for the verifier.
- **URL cleanup** — encodes spaces/symbols in plugin download URLs.
- **Incremental `results.json`** — rewritten as each plugin finishes.
- **Verbose failure dumps** — traces, Python stacks, RPC context, and JVM stderr under `logs/error-logs/`.

## Usage

From this directory:

```powershell
.\record_extensions.cmd
.\record_extensions.cmd -Workers 8
.\record_extensions.cmd -MaxPlugins 10
```

From anywhere:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "<repo>\tools\extension_recorder\record_extensions.ps1"
```

Or call Python directly:

```powershell
python recorder.py --repo-url <url> --workers 4 --timeout 45
```

### Parameters (`record_extensions.ps1` → `recorder.py`)

| PowerShell | Python | Default |
|---|---|---|
| `-RepoUrl` | `--repo-url` | MegaRepo `repo.json` |
| `-Workers` | `--workers` | `4` |
| `-MaxPlugins` | `--max-plugins` | `0` (all) |
| `-TimeoutSec` | `--timeout` | `45` (per plugin load) |
| `-SidecarHome` | `--sidecar-home` | auto-detect |
| `-Jdk21` | `--jdk21` | auto-detect |
| `-SkipDownload` | `--skip-download` | reuse `cache/*.cs3` |
| `-Untrusted` | `--untrusted` | trusted off (enable verifier) |

## Output (`tools/extension_recorder/`)

- `replies/discovered_repositories.json` — repos from the aggregator’s `take_added_repositories`
- `replies/all_plugins_catalog.json` — unique plugins across those repos
- `replies/results.json` — full audit rows (updated incrementally)
- `replies/summary.json` / `summary.md` — counts, error types, missing-class leaderboard
- `replies/details/<internalName>_<version>.json` — per-plugin reply
- `cache/` — downloaded `.cs3` files and per-worker sidecar data dirs
- `logs/error-logs/`
  - `latest_errors.log` — rolling error dump for this run
  - `recorder_trace_<timestamp>.log` — communication / step trace
  - `recorder_errors_<timestamp>.log` — same errors as `latest_errors`
  - `plugin_failures/<name>_v<version>.log` — one dump per failing plugin

## Helpers

- `analyze_failures.py` — from repo root, groups `replies/results.json` by error type (read-only).
- `verify_fixes.py` — ad-hoc Windows recheck of a hard-coded plugin list; edit paths before use.
