"""Load mode: load every plugin in sidecar workers and record what each reply says (moved from
recorder.py steps 4-5, unchanged; it now runs on the video plugins only).

This finds what breaks while loading. Problems in code paths that only run later (search, links)
need scan mode (api_scan.py), which reads the bytecode instead.
"""

import json
import re
import threading
from dataclasses import dataclass
from datetime import datetime
from queue import Empty, Queue

from http_fetch import clean_url, download_file
from sidecar_process import SidecarProcess
from tool_log import (BOLD, CACHE_DIR, CYAN, DETAILS_DIR, GRAY, GREEN, LOGS_DIR, RED, REPLIES_DIR, RESET, YELLOW,
                      log_excessive_error, log_header, log_info, log_plugin_failure_dump, log_step, log_success,
                      log_trace, log_warn, print_lock, sanitize_filename)


@dataclass
class LoadRun:
    """How to run the load test, and what the crawl found (for the summary)."""
    repo_url: str
    sidecar_home: str
    jdk21_home: str
    workers: int
    timeout_sec: int
    trusted: bool
    start_time: datetime
    repositories_found: int
    plugins_indexed: int


def run_load_test(plugins_to_test: list[dict], settings: LoadRun) -> None:
    repo_url, sidecar_home, jdk21_home = settings.repo_url, settings.sidecar_home, settings.jdk21_home
    workers, timeout_sec, trusted = settings.workers, settings.timeout_sec, settings.trusted
    start_time = settings.start_time
    discovered_repos = range(settings.repositories_found)  # only its length is reported
    all_plugins = range(settings.plugins_indexed)  # only its length is reported

    # Step 4: Multi-Worker Testing
    log_step("LOAD 1/2", f"Testing {len(plugins_to_test)} Plugins with {workers} Sidecar Workers")
    
    # Initialize workers
    worker_pool: list[SidecarProcess] = []
    log_info(f"Spawning {workers} sidecar worker processes...")
    for wid in range(1, workers + 1):
        wdir = CACHE_DIR / f"worker_{wid}"
        sp = SidecarProcess(sidecar_home, jdk21_home, wdir, worker_id=wid)
        sp.call("ping", timeout=10)
        sp.call("set_debug_logging", {"enabled": True}, timeout=5)
        worker_pool.append(sp)
    log_success(f"All {workers} sidecar workers are ready!")

    task_queue: Queue[tuple[int, dict]] = Queue()
    for idx, plugin in enumerate(plugins_to_test, 1):
        task_queue.put((idx, plugin))

    results: list[dict] = []
    results_lock = threading.RLock()
    progress_counter = 0

    def save_incremental_reports():
        with results_lock:
            current_results = list(results)
        results_file = REPLIES_DIR / "results.json"
        try:
            results_file.write_text(json.dumps(current_results, indent=2), encoding="utf-8")
        except Exception as e:
            log_trace(f"Error saving incremental results.json: {e}")

    def worker_loop(wid: int, worker: SidecarProcess):
        nonlocal progress_counter
        while True:
            try:
                idx, plugin = task_queue.get_nowait()
            except Empty:
                break

            name = plugin.get("name") or plugin.get("internalName") or "Unknown"
            int_name = plugin.get("internalName") or name
            version = plugin.get("version", 1)
            raw_url = plugin.get("url")
            cs3_url = clean_url(raw_url)
            repo_name = plugin.get("_source_repo_name", "Unknown Repo")

            record = {
                "plugin_name": name,
                "internal_name": int_name,
                "version": version,
                "repository_name": repo_name,
                "repository_url": plugin.get("_source_repo_url"),
                "cs3_url": cs3_url,
                "tested_at": datetime.now().isoformat(),
                "status": "unknown",
                "providers_count": 0,
                "providers": [],
                "error": None,
                "stderr_logs": "",
            }

            if not cs3_url:
                record["status"] = "error"
                record["error"] = {"type": "InvalidPluginMetadata", "message": "Missing CS3 URL"}
                with print_lock:
                    print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version})... {RED}[FAILED: No CS3 URL]{RESET}", flush=True)
                should_save = False
                with results_lock:
                    results.append(record)
                    progress_counter += 1
                    should_save = (progress_counter % 10 == 0)
                if should_save:
                    save_incremental_reports()
                task_queue.task_done()
                continue

            # Download .cs3
            safe_fname = f"{sanitize_filename(int_name)}_{version}.cs3"
            cs3_dest = CACHE_DIR / safe_fname
            try:
                if not cs3_dest.exists():
                    download_file(cs3_url, cs3_dest, timeout=25)
            except Exception as dl_err:
                record["status"] = "error"
                record["error"] = {"type": "DownloadError", "message": str(dl_err)}
                with print_lock:
                    print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version})... {RED}[DOWNLOAD FAILED: {dl_err.__class__.__name__}]{RESET}", flush=True)
                log_excessive_error(f"Plugin {name} download failed", dl_err, {"plugin": plugin, "cs3_dest": str(cs3_dest)})
                log_plugin_failure_dump(name, version, record["error"], str(dl_err), plugin)
                should_save = False
                with results_lock:
                    results.append(record)
                    progress_counter += 1
                    should_save = (progress_counter % 10 == 0)
                if should_save:
                    save_incremental_reports()
                task_queue.task_done()
                continue

            # Call load_extension on worker sidecar
            try:
                reply, stderr_log = worker.call(
                    "load_extension",
                    {"path": str(worker.stage(cs3_dest)), "trusted": trusted},
                    timeout=timeout_sec,
                )
                record["stderr_logs"] = stderr_log

                if "error" in reply:
                    record["status"] = "error"
                    record["error"] = reply["error"]
                    err_type = reply["error"].get("type", "Error")
                    err_msg = reply["error"].get("message", "")
                    short_msg = f"{err_type}: {err_msg[:60]}" if err_msg else err_type
                    with print_lock:
                        print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version}) [{GRAY}{repo_name}{RESET}]... {RED}[FAILED: {short_msg}]{RESET}", flush=True)
                    log_excessive_error(f"Plugin Load Error: {name} (v{version})", None, {"plugin": plugin, "reply": reply}, stderr_log)
                    log_plugin_failure_dump(name, version, reply["error"], stderr_log, plugin)
                else:
                    providers = reply.get("result", [])
                    record["status"] = "success"
                    record["providers_count"] = len(providers)
                    record["providers"] = providers
                    with print_lock:
                        print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version}) [{GRAY}{repo_name}{RESET}]... {GREEN}[SUCCESS: {len(providers)} provider(s)]{RESET}", flush=True)

            except TimeoutError as t_err:
                record["status"] = "timeout"
                record["error"] = {"type": "TimeoutError", "message": str(t_err)}
                recent_stderr = worker.get_recent_stderr_history()
                with print_lock:
                    print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version})... {YELLOW}[TIMEOUT ({timeout_sec}s)]{RESET}", flush=True)
                log_excessive_error(f"Plugin Load Timeout: {name} (v{version})", t_err, {"plugin": plugin}, recent_stderr)
                log_plugin_failure_dump(name, version, record["error"], recent_stderr, plugin)

            except Exception as c_err:
                record["status"] = "crashed"
                record["error"] = {"type": "CrashOrTransportError", "message": str(c_err)}
                recent_stderr = worker.get_recent_stderr_history()
                with print_lock:
                    print(f"  [{idx:3d}/{len(plugins_to_test):3d}] [W-{wid}] {BOLD}{name}{RESET} (v{version})... {RED}[CRASHED/TRANSPORT ERROR]{RESET}", flush=True)
                log_excessive_error(f"Plugin Crash/Transport: {name} (v{version})", c_err, {"plugin": plugin}, recent_stderr)
                log_plugin_failure_dump(name, version, record["error"], recent_stderr, plugin)

            detail_file = DETAILS_DIR / f"{sanitize_filename(int_name)}_{version}.json"
            detail_file.write_text(json.dumps(record, indent=2), encoding="utf-8")

            should_save = False
            with results_lock:
                results.append(record)
                progress_counter += 1
                should_save = (progress_counter % 10 == 0)

            if should_save:
                save_incremental_reports()

            task_queue.task_done()

    # Launch worker threads
    worker_threads = []
    stop_heartbeat = threading.Event()

    def heartbeat_worker():
        while not stop_heartbeat.wait(10.0):
            with results_lock:
                current_done = len(results)
            pct = round((current_done / max(len(plugins_to_test), 1)) * 100, 1)
            log_info(f"Progress Heartbeat: {current_done}/{len(plugins_to_test)} plugins completed ({pct}%)...")

    hb_thread = threading.Thread(target=heartbeat_worker, name="Heartbeat", daemon=True)
    hb_thread.start()

    for wid, worker in enumerate(worker_pool, 1):
        t = threading.Thread(target=worker_loop, args=(wid, worker), name=f"WorkerThread-{wid}", daemon=True)
        t.start()
        worker_threads.append(t)

    try:
        for t in worker_threads:
            t.join()
    except KeyboardInterrupt:
        log_warn("\nAudit interrupted by user! Saving current results...")
    finally:
        stop_heartbeat.set()
        log_info("Shutting down worker processes...")
        for sp in worker_pool:
            sp.stop()

    # Step 5: Save aggregate reports
    log_step("LOAD 2/2", "Generating Final Reports & Analyzing Error Patterns")
    save_incremental_reports()
    log_info(f"Saved raw test results to results.json")

    passed_count = sum(1 for r in results if r["status"] == "success")
    failed_count = sum(1 for r in results if r["status"] == "error")
    timeout_count = sum(1 for r in results if r["status"] == "timeout")
    crashed_count = sum(1 for r in results if r["status"] == "crashed")

    error_patterns: dict[str, int] = {}
    missing_classes: dict[str, int] = {}
    for r in results:
        err = r.get("error")
        if err:
            etype = err.get("type", "Unknown")
            emsg = str(err.get("message", ""))
            error_patterns[etype] = error_patterns.get(etype, 0) + 1

            if "NoClassDefFoundError" in etype or "ClassNotFoundException" in etype or "NoClassDefFoundError" in r.get("stderr_logs", ""):
                m = re.search(r'(?:NoClassDefFoundError|ClassNotFoundException):\s*([a-zA-Z0-9_/\.$]+)', emsg + " " + r.get("stderr_logs", ""))
                if m:
                    cls_name = m.group(1).replace('/', '.')
                    missing_classes[cls_name] = missing_classes.get(cls_name, 0) + 1

    summary_stats = {
        "generated_at": datetime.now().isoformat(),
        "elapsed_seconds": int((datetime.now() - start_time).total_seconds()),
        "total_repositories_discovered": len(discovered_repos),
        "total_plugins_indexed": len(all_plugins),
        "total_plugins_tested": len(results),
        "passed": passed_count,
        "failed": failed_count,
        "timed_out": timeout_count,
        "crashed": crashed_count,
        "pass_rate_percent": round((passed_count / max(len(results), 1)) * 100, 1),
        "error_types": error_patterns,
        "top_missing_classes": dict(sorted(missing_classes.items(), key=lambda x: x[1], reverse=True)[:30]),
        "error_logs_directory": str(LOGS_DIR.resolve()),
    }

    summary_json_file = REPLIES_DIR / "summary.json"
    summary_json_file.write_text(json.dumps(summary_stats, indent=2), encoding="utf-8")

    md_lines = [
        "# CloudStream Extension Audit Report",
        f"\n**Audit Date**: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}  ",
        f"**Seed Repository**: `{repo_url}`  ",
        f"**Sidecar**: `{sidecar_home}`  ",
        f"**Workers**: `{workers}`  ",
        f"**Trusted Mode**: `{trusted}`  ",
        f"**Error Logs Directory**: `{LOGS_DIR.resolve()}`  \n",
        "## Overall Summary",
        "",
        "| Metric | Value |",
        "|---|---|",
        f"| Discovered Repositories | **{len(discovered_repos)}** |",
        f"| Total Plugins Discovered | **{len(all_plugins)}** |",
        f"| Total Plugins Tested | **{len(results)}** |",
        f"| Successfully Loaded | **{passed_count}** ({summary_stats['pass_rate_percent']}%) |",
        f"| Failed to Load | **{failed_count}** |",
        f"| Timed Out | **{timeout_count}** |",
        f"| Crashes/Transport Errors | **{crashed_count}** |",
        "",
        "## Top Error Categories",
        "",
        "| Error Type | Occurrences |",
        "|---|---|",
    ]
    for et, cnt in sorted(error_patterns.items(), key=lambda x: x[1], reverse=True):
        md_lines.append(f"| `{et}` | {cnt} |")

    if missing_classes:
        md_lines.extend([
            "",
            "## Top Missing Android Stubs / Classes",
            "These classes need to be added or implemented in `android-stubs` or `cs-sidecar`:",
            "",
            "| Class Name | Failing Extensions |",
            "|---|---|",
        ])
        for cls, count in sorted(missing_classes.items(), key=lambda x: x[1], reverse=True)[:35]:
            md_lines.append(f"| `{cls}` | {count} |")

    md_lines.extend([
        "",
        "## Tested Extensions Detail",
        "",
        "| Plugin | Repository | Status | Providers | Error |",
        "|---|---|---|---|---|",
    ])
    for r in results:
        status_badge = "✅ PASS" if r["status"] == "success" else f"❌ {r['status'].upper()}"
        err_msg = r["error"].get("type", "") if r.get("error") else "-"
        md_lines.append(f"| **{r['plugin_name']}** (v{r['version']}) | {r['repository_name']} | {status_badge} | {r['providers_count']} | `{err_msg}` |")

    summary_md_file = REPLIES_DIR / "summary.md"
    summary_md_file.write_text("\n".join(md_lines), encoding="utf-8")
    log_info(f"Saved Markdown report to {summary_md_file.name}")

    log_header("AUDIT EXECUTION COMPLETE")
    print(f"  {BOLD}Total Tested{RESET}   : {len(results)}")
    print(f"  {GREEN}{BOLD}Passed{RESET}         : {passed_count} ({summary_stats['pass_rate_percent']}%)")
    print(f"  {RED}{BOLD}Failed{RESET}         : {failed_count}")
    print(f"  {YELLOW}{BOLD}Timeouts{RESET}       : {timeout_count}")
    print(f"  {RED}{BOLD}Crashes{RESET}        : {crashed_count}")
    if missing_classes:
        print(f"\n  {BOLD}Top Missing Android Classes to implement:{RESET}")
        for cls, cnt in list(sorted(missing_classes.items(), key=lambda x: x[1], reverse=True))[:5]:
            print(f"    - {YELLOW}{cls}{RESET} ({cnt} plugins)")

    print(f"\n  {BOLD}Results & Reports saved to:{RESET}")
    print(f"  -> {CYAN}{REPLIES_DIR.resolve()}{RESET}")
    print(f"\n  {BOLD}Detailed Error Logs saved to:{RESET}")
    print(f"  -> {RED}{LOGS_DIR.resolve()}{RESET}")
    print()

