# CloudStream extension recorder

Finds what the desktop runtime lacks for the **video plugins** people use, before a viewer hits it.
Linux and Windows.

1. **Crawl** (`catalog.py`): downloads the seed repository's aggregator (MegaProvider), lets it announce
   its repositories in a sidecar, and reads every plugin list.
2. **Filter** (`content_filter.py`): keeps video plugins only, with the app's own rule
   (`crates/content_policy`, through its `plugin-content-policy` command; nothing is reimplemented
   here). Games, music, audio, audiobooks, podcasts and tools are left out and listed in
   `replies/excluded_plugins.json` with the reason.
3. **Scan** (`api_scan.py`, default) or **load** (`load_test.py`):
   - `--mode scan` downloads the video plugins and runs the static API gap scanner
     (`sidecar/api-scanner`): every reference to an Android or CloudStream class is checked against
     the runtime's classes, whether or not that code would run in a crawl. Report:
     `replies/api_gaps.md` (ranked: what to implement first) and `replies/api_gaps.json`.
   - `--mode load` loads each plugin in sidecar workers and records the replies (load-time errors only):
     `replies/summary.md`, `replies/results.json`, `replies/details/`.

## Requirements

- Python 3.10+, JDK 21, `cargo` (builds the content-policy command on first use).
- A built `cs-sidecar` (found in `CS_SIDECAR_HOME`, `target/bundle-cache/cs-sidecar-dist`, the sidecar
  checkout, or `C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar`).
- For scan mode, a built `api-scanner`: `--build-scanner` builds it on Linux
  (`scripts/prepare_sidecar_repo.sh`, then `:api-scanner:installDist`); on Windows run
  `scripts\setup_sidecar.ps1`, then `gradlew :api-scanner:installDist` in `C:\cs3d\repo`. Or point
  `--scanner-home` / `CS_API_SCANNER_HOME` at one.

## Usage

```sh
python3 recorder.py                         # scan every video plugin (needs a built api-scanner)
python3 recorder.py --build-scanner         # Linux: build the scanner first
python3 recorder.py --mode load --workers 8 # load-time audit in 8 sidecars
python3 recorder.py --max-plugins 20        # only the first 20 video plugins
```

Windows: `.\record_extensions.cmd [-Mode scan|load] [-Workers 8] [-MaxPlugins 20]`.

| Option | Default | |
|---|---|---|
| `--mode` | `scan` | `scan` (static report) or `load` (load every plugin) |
| `--repo-url` | MegaRepo `repo.json` | seed repository |
| `--workers` | `4` | sidecars (load) or scanner threads (scan) |
| `--max-plugins` | `0` (all) | cap after filtering |
| `--timeout` | `45` | load mode: seconds per plugin load |
| `--untrusted` | off | load mode: load untrusted (bytecode verifier on) |
| `--sidecar-home`, `--scanner-home`, `--jdk21` | auto | explicit locations |
| `--build-scanner` | off | Linux: build api-scanner first |

## Output

Everything below is ignored by git (`.gitignore`).

- `replies/discovered_repositories.json`, `replies/all_plugins_catalog.json`: what the crawl found
- `replies/excluded_plugins.json`: plugins left out, with the reason ("not a video plugin (it declares Others)")
- `replies/api_gaps.md` / `api_gaps.json` (scan): gaps by status (missing members, missing classes,
  neutralised UI calls, broken stubs) and plugins that would not load untrusted
- `replies/summary.md`, `results.json`, `details/` (load)
- `cache/`: downloaded plugins (`plugins/<name>_<version>.cs3`) and sidecar workspaces (each a data
  folder laid out like the app's: `sidecar/` and `extensions/`, the only place the sidecar loads from)
- `logs/error-logs/`: trace, errors, one dump per failing plugin

## Modules and tests

`recorder.py` (entry point), `catalog.py`, `content_filter.py`, `api_scan.py`, `load_test.py`,
`sidecar_process.py` (one sidecar over JSON lines), `sidecar_env.py` (JDK / sidecar / scanner
discovery), `http_fetch.py`, `tool_log.py`; `analyze_failures.py` groups a load run's errors.

Tests: `python3 -m unittest discover -s tools/extension_recorder -p "*_test.py"` (CI runs them; no
network; the contract test runs the real content-policy command when cargo is present).
