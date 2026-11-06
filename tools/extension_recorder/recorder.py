#!/usr/bin/env python3
"""
CloudStream Extension Response Recorder (High-Performance Parallel Edition)

Crawls a root repository (e.g. MegaRepo), discovers all added repositories via cs-sidecar,
crawls all plugins across all discovered repositories in parallel, test-loads each plugin
across multiple concurrent cs-sidecar worker processes, and records all replies
(stdout RPC responses + stderr logs/stack traces) into the script's directory.

Excessive error logging is saved in:
  <script_dir>/logs/error-logs/
"""

import argparse
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import traceback
import urllib.parse
import urllib.request
import urllib.error
from datetime import datetime
from pathlib import Path
from queue import Queue, Empty

# Ensure immediate unbuffered terminal output on Windows
if hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(line_buffering=True)
    except Exception:
        pass
if hasattr(sys.stderr, "reconfigure"):
    try:
        sys.stderr.reconfigure(line_buffering=True)
    except Exception:
        pass

# ANSI color codes for rich terminal feedback
RESET = "\033[0m"
BOLD = "\033[1m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
RED = "\033[31m"
CYAN = "\033[36m"
MAGENTA = "\033[35m"
GRAY = "\033[90m"

# Default seed repository
DEFAULT_REPO_URL = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json"

SCRIPT_DIR = Path(__file__).resolve().parent
REPLIES_DIR = SCRIPT_DIR / "replies"
DETAILS_DIR = REPLIES_DIR / "details"
CACHE_DIR = SCRIPT_DIR / "cache"
LOGS_DIR = SCRIPT_DIR / "logs" / "error-logs"
PLUGIN_FAILURES_DIR = LOGS_DIR / "plugin_failures"

USER_AGENT = "CloudStream-Extension-Recorder/2.0"

# Session timestamp for log filenames
SESSION_ID = datetime.now().strftime("%Y%m%d_%H%M%S")
TRACE_LOG_FILE = LOGS_DIR / f"recorder_trace_{SESSION_ID}.log"
ERROR_LOG_FILE = LOGS_DIR / f"recorder_errors_{SESSION_ID}.log"
LATEST_ERROR_FILE = LOGS_DIR / "latest_errors.log"

print_lock = threading.Lock()
log_lock = threading.Lock()
results_lock = threading.RLock()


def init_logging_directories():
    LOGS_DIR.mkdir(parents=True, exist_ok=True)
    PLUGIN_FAILURES_DIR.mkdir(parents=True, exist_ok=True)
    REPLIES_DIR.mkdir(parents=True, exist_ok=True)
    DETAILS_DIR.mkdir(parents=True, exist_ok=True)
    CACHE_DIR.mkdir(parents=True, exist_ok=True)


def write_to_log_file(file_path: Path, text: str):
    with log_lock:
        try:
            with open(file_path, "a", encoding="utf-8") as f:
                f.write(text)
                if not text.endswith("\n"):
                    f.write("\n")
        except Exception:
            pass


def log_trace(msg: str):
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
    formatted = f"[{timestamp}] [TRACE] {msg}"
    write_to_log_file(TRACE_LOG_FILE, formatted)


def log_excessive_error(title: str, error: Exception | None = None, details: dict | None = None, stderr_dump: str = ""):
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
    divider = "=" * 80
    lines = [
        divider,
        f"[{timestamp}] ERROR: {title}",
        f"Thread: {threading.current_thread().name} (ID: {threading.get_ident()})",
    ]

    if details:
        lines.append("\n--- CONTEXT DETAILS ---")
        try:
            lines.append(json.dumps(details, indent=2, default=str))
        except Exception:
            for k, v in details.items():
                lines.append(f"  {k}: {v}")

    if error:
        lines.append("\n--- EXCEPTION INFO ---")
        lines.append(f"Exception Type : {error.__class__.__name__}")
        lines.append(f"Exception Msg  : {error}")
        lines.append("\n--- PYTHON TRACEBACK ---")
        lines.append(traceback.format_exc())

    if stderr_dump and stderr_dump.strip():
        lines.append("\n--- SIDECAR / JVM STDERR DUMP ---")
        lines.append(stderr_dump.strip())

    lines.append(divider)
    block = "\n".join(lines) + "\n"

    write_to_log_file(ERROR_LOG_FILE, block)
    write_to_log_file(LATEST_ERROR_FILE, block)
    write_to_log_file(TRACE_LOG_FILE, block)


def log_plugin_failure_dump(plugin_name: str, version: int, error_info: dict, stderr_logs: str, plugin_meta: dict):
    safe_name = sanitize_filename(f"{plugin_name}_v{version}")
    dump_file = PLUGIN_FAILURES_DIR / f"{safe_name}.log"
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]

    content = [
        "=" * 80,
        f"PLUGIN FAILURE AUDIT: {plugin_name} (v{version})",
        f"Timestamp: {timestamp}",
        "=" * 80,
        "\n--- PLUGIN METADATA ---",
        json.dumps(plugin_meta, indent=2, default=str),
        "\n--- REPORTED ERROR ---",
        json.dumps(error_info, indent=2, default=str),
        "\n--- COMPLETE JVM STDERR LOGS ---",
        stderr_logs.strip() if stderr_logs.strip() else "(No stderr logs produced)",
        "=" * 80,
    ]
    write_to_log_file(dump_file, "\n".join(content) + "\n")


def log_header(title: str):
    width = 78
    with print_lock:
        print(flush=True)
        print(f"{CYAN}{'=' * width}{RESET}", flush=True)
        print(f"{BOLD}{CYAN}{title.center(width)}{RESET}", flush=True)
        print(f"{CYAN}{'=' * width}{RESET}", flush=True)
    log_trace(f"HEADER: {title}")


def log_info(msg: str):
    with print_lock:
        print(f"{CYAN}[INFO]{RESET} {msg}", flush=True)
    log_trace(f"INFO: {msg}")


def log_step(step: str, title: str):
    with print_lock:
        print(f"\n{BOLD}{MAGENTA}[{step}]{RESET} {BOLD}{title}{RESET}", flush=True)
    log_trace(f"STEP {step}: {title}")


def log_success(msg: str):
    with print_lock:
        print(f"{GREEN}[SUCCESS]{RESET} {msg}", flush=True)
    log_trace(f"SUCCESS: {msg}")


def log_warn(msg: str):
    with print_lock:
        print(f"{YELLOW}[WARN]{RESET} {msg}", flush=True)
    log_trace(f"WARN: {msg}")


def log_error(msg: str):
    with print_lock:
        print(f"{RED}[ERROR]{RESET} {msg}", flush=True)
    log_trace(f"ERROR: {msg}")


def sanitize_filename(name: str) -> str:
    cleaned = re.sub(r'[^a-zA-Z0-9_\-\.]', '_', name)
    return cleaned[:100]


def clean_url(url: str) -> str:
    """Properly encodes spaces and special characters in URL path and query."""
    if not url:
        return ""
    try:
        parts = urllib.parse.urlsplit(url.strip())
        safe_path = urllib.parse.quote(parts.path, safe="/%~+=-_.")
        safe_query = urllib.parse.quote(parts.query, safe="&%=+-_.:")
        return urllib.parse.urlunsplit((parts.scheme, parts.netloc, safe_path, safe_query, parts.fragment))
    except Exception:
        return url


def find_jdk21(explicit_path: str = "") -> str:
    if explicit_path and os.path.exists(os.path.join(explicit_path, "bin", "java.exe")):
        return explicit_path

    env_jdk = os.environ.get("CS_SIDECAR_JAVA_HOME") or os.environ.get("JAVA_HOME")
    if env_jdk and os.path.exists(os.path.join(env_jdk, "bin", "java.exe")):
        try:
            out = subprocess.check_output([os.path.join(env_jdk, "bin", "java.exe"), "-version"], stderr=subprocess.STDOUT, text=True)
            if '"21.' in out:
                return env_jdk
        except Exception as e:
            log_excessive_error("Checking env JDK 21 version failed", e, {"env_jdk": env_jdk})

    candidates = [
        r"C:\Program Files\JetBrains\IntelliJ IDEA Community Edition 2025.2.4\jbr",
        r"C:\Program Files\JetBrains\IntelliJ IDEA Community Edition 2025.2.6.2\jbr",
        r"C:\Program Files\JetBrains\GoLand 2026.2.3\jbr",
        r"C:\Program Files\Java\jdk-21",
        r"C:\Program Files\Eclipse Adoptium\jdk-21",
    ]
    for cand in candidates:
        java_exe = os.path.join(cand, "bin", "java.exe")
        if os.path.exists(java_exe):
            try:
                out = subprocess.check_output([java_exe, "-version"], stderr=subprocess.STDOUT, text=True)
                if '"21.' in out:
                    return cand
            except Exception as e:
                log_excessive_error("Candidate java check failed", e, {"candidate": cand})

    for base in [r"C:\Program Files\JetBrains", r"C:\Program Files\Java", r"C:\Program Files\Eclipse Adoptium"]:
        if os.path.isdir(base):
            for root, dirs, files in os.walk(base):
                if "java.exe" in files:
                    java_exe = os.path.join(root, "java.exe")
                    try:
                        out = subprocess.check_output([java_exe, "-version"], stderr=subprocess.STDOUT, text=True)
                        if '"21.' in out:
                            return str(Path(root).parent)
                    except Exception:
                        pass
                if len(dirs) > 10:
                    dirs[:] = [d for d in dirs if 'jbr' in d.lower() or '21' in d]

    err = RuntimeError("JDK 21 not found. Please specify -Jdk21 or set CS_SIDECAR_JAVA_HOME.")
    log_excessive_error("JDK 21 discovery failed", err)
    raise err


def find_sidecar_home(explicit_path: str = "") -> str:
    if explicit_path and os.path.exists(os.path.join(explicit_path, "bin", "cs-sidecar.bat")):
        return explicit_path

    env_home = os.environ.get("CS_SIDECAR_HOME")
    if env_home and os.path.exists(os.path.join(env_home, "bin", "cs-sidecar.bat")):
        return env_home

    default_loc = r"C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar"
    if os.path.exists(os.path.join(default_loc, "bin", "cs-sidecar.bat")):
        return default_loc

    err = RuntimeError("Sidecar distribution not found. Please build it or specify -SidecarHome.")
    log_excessive_error("Sidecar discovery failed", err)
    raise err


def fetch_url(url: str, timeout: int = 10, retries: int = 1) -> bytes:
    encoded_url = clean_url(url)
    log_trace(f"HTTP GET -> {encoded_url} (timeout={timeout}s, retries={retries})")
    req = urllib.request.Request(encoded_url, headers={"User-Agent": USER_AGENT})
    last_err = None
    for attempt in range(retries + 1):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                data = resp.read()
                log_trace(f"HTTP {resp.status} <- {encoded_url} ({len(data)} bytes)")
                return data
        except Exception as e:
            last_err = e
            log_trace(f"HTTP attempt {attempt+1} failed for {encoded_url}: {e}")
            if attempt < retries:
                time.sleep(0.5)

    log_excessive_error(f"HTTP GET failed for {encoded_url}", last_err, {"url": url, "encoded": encoded_url, "timeout": timeout, "attempts": retries + 1})
    raise RuntimeError(f"Failed to fetch {encoded_url}: {last_err}")


def download_file(url: str, dest_path: Path, timeout: int = 30) -> Path:
    if dest_path.exists() and dest_path.stat().st_size > 0:
        log_trace(f"File already cached: {dest_path.name} ({dest_path.stat().st_size} bytes)")
        return dest_path

    dest_path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = dest_path.with_suffix(dest_path.suffix + f".part_{os.getpid()}_{threading.get_ident()}")
    try:
        content = fetch_url(url, timeout=timeout, retries=1)
        temp_path.write_bytes(content)
        temp_path.replace(dest_path)
        log_trace(f"Downloaded successfully: {dest_path.name} ({dest_path.stat().st_size} bytes)")
        return dest_path
    except Exception as e:
        if temp_path.exists():
            temp_path.unlink()
        log_excessive_error(f"Download failed for {url}", e, {"url": url, "destination": str(dest_path)})
        raise


class SidecarProcess:
    def __init__(self, sidecar_home: str, java_home: str, data_dir: Path, worker_id: int = 0):
        self.sidecar_home = sidecar_home
        self.java_home = java_home
        self.data_dir = data_dir
        self.worker_id = worker_id
        self.proc: subprocess.Popen | None = None
        self.next_id = 1
        self.stderr_queue: Queue[str] = Queue()
        self.stderr_history: list[str] = []
        self.history_lock = threading.Lock()
        self.stderr_thread: threading.Thread | None = None
        self.start()

    def start(self):
        bat = os.path.join(self.sidecar_home, "bin", "cs-sidecar.bat")
        env = os.environ.copy()
        env["JAVA_HOME"] = self.java_home
        self.data_dir.mkdir(parents=True, exist_ok=True)
        env["JAVA_OPTS"] = (
            f'-Dcloudstream.data.dir="{self.data_dir}" '
            "-Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8 -Dfile.encoding=UTF-8 "
            "-Djavax.net.ssl.trustStoreType=Windows-ROOT"
        )
        log_trace(f"[Worker-{self.worker_id}] Spawning sidecar: {bat}")
        self.proc = subprocess.Popen(
            [bat],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            env=env,
            bufsize=1,
        )
        log_trace(f"[Worker-{self.worker_id}] Sidecar started with PID {self.proc.pid}")

        def reader():
            while self.proc and self.proc.poll() is None:
                try:
                    line = self.proc.stderr.readline()
                    if line:
                        self.stderr_queue.put(line)
                        with self.history_lock:
                            self.stderr_history.append(line)
                            if len(self.stderr_history) > 1000:
                                self.stderr_history.pop(0)
                        log_trace(f"[JVM-{self.worker_id} STDERR] {line.strip()}")
                    else:
                        break
                except Exception:
                    break

        self.stderr_thread = threading.Thread(target=reader, name=f"StderrReader-{self.worker_id}", daemon=True)
        self.stderr_thread.start()

    def stop(self):
        if self.proc:
            pid = self.proc.pid
            log_trace(f"[Worker-{self.worker_id}] Stopping sidecar PID {pid}")
            try:
                if sys.platform == "win32":
                    subprocess.run(["taskkill", "/F", "/T", "/PID", str(pid)], capture_output=True, timeout=5)
                else:
                    self.proc.terminate()
                    self.proc.wait(timeout=3)
            except Exception:
                try:
                    self.proc.kill()
                except Exception:
                    pass
            self.proc = None

    def restart(self):
        log_trace(f"[Worker-{self.worker_id}] Restarting sidecar...")
        self.stop()
        self.start()

    def drain_stderr(self) -> str:
        lines = []
        while True:
            try:
                lines.append(self.stderr_queue.get_nowait())
            except Empty:
                break
        return "".join(lines)

    def get_recent_stderr_history(self, max_lines: int = 150) -> str:
        with self.history_lock:
            return "".join(self.stderr_history[-max_lines:])

    def call(self, method: str, params: dict | None = None, timeout: float = 30.0) -> tuple[dict, str]:
        if not self.proc or self.proc.poll() is not None:
            self.restart()

        req_id = self.next_id
        self.next_id += 1
        req = {"id": req_id, "method": method}
        if params is not None:
            req["params"] = params

        self.drain_stderr()

        try:
            line = json.dumps(req)
            log_trace(f"[Worker-{self.worker_id}] RPC SEND -> {line}")
            self.proc.stdin.write(line + "\n")
            self.proc.stdin.flush()
        except Exception as e:
            recent_err = self.get_recent_stderr_history()
            log_excessive_error(f"Worker {self.worker_id} write failed ({method})", e, {"request": req}, recent_err)
            self.restart()
            raise RuntimeError(f"Failed to write to sidecar stdin: {e}")

        result_queue: Queue[str] = Queue()

        def read_stdout():
            try:
                out_line = self.proc.stdout.readline()
                result_queue.put(out_line)
            except Exception as ex:
                result_queue.put(f"ERROR:{ex}")

        reader_t = threading.Thread(target=read_stdout, name=f"RpcReader-{self.worker_id}-{req_id}", daemon=True)
        reader_t.start()
        reader_t.join(timeout=timeout)

        if reader_t.is_alive():
            recent_err = self.get_recent_stderr_history()
            log_excessive_error(
                f"[Worker-{self.worker_id}] RPC TIMEOUT: {method} timed out after {timeout}s",
                None,
                {"request": req, "pid": self.proc.pid if self.proc else None},
                recent_err
            )
            self.restart()
            raise TimeoutError(f"Call {method} timed out after {timeout}s")

        try:
            raw_reply = result_queue.get_nowait()
        except Empty:
            recent_err = self.get_recent_stderr_history()
            log_excessive_error(f"[Worker-{self.worker_id}] Empty response for {method}", None, {"request": req}, recent_err)
            self.restart()
            raise RuntimeError(f"No response from sidecar for {method}")

        if raw_reply.startswith("ERROR:"):
            recent_err = self.get_recent_stderr_history()
            log_excessive_error(f"[Worker-{self.worker_id}] Stdout error: {raw_reply}", None, {"request": req}, recent_err)
            self.restart()
            raise RuntimeError(f"Error reading sidecar response: {raw_reply}")

        time.sleep(0.04)
        captured_stderr = self.drain_stderr()
        log_trace(f"[Worker-{self.worker_id}] RPC RECV <- {raw_reply.strip()}")

        try:
            data = json.loads(raw_reply)
            return data, captured_stderr
        except json.JSONDecodeError as ex:
            recent_err = self.get_recent_stderr_history()
            log_excessive_error(
                f"[Worker-{self.worker_id}] Unparseable JSON for {method}",
                ex,
                {"raw_reply": raw_reply, "request": req},
                captured_stderr or recent_err
            )
            raise RuntimeError(f"Unparseable JSON from sidecar: {raw_reply.strip()}")


def run_recorder(
    repo_url: str,
    sidecar_home: str,
    jdk21_home: str,
    max_plugins: int = 0,
    skip_download: bool = False,
    timeout_sec: int = 45,
    workers: int = 4,
    trusted: bool = True,
):
    start_time = datetime.now()
    init_logging_directories()

    log_header("CloudStream Extension Response Recorder (High-Performance)")
    log_info(f"Target Seed Repository : {repo_url}")
    log_info(f"Sidecar Distribution  : {sidecar_home}")
    log_info(f"JDK 21 Runtime         : {jdk21_home}")
    log_info(f"Parallel Workers       : {workers} processes")
    log_info(f"Trusted Mode           : {trusted} (Bypasses PluginSecurityVerifier)")
    log_info(f"Output Directory       : {REPLIES_DIR}")
    log_info(f"Error Logs Directory   : {LOGS_DIR}")
    log_info(f"Cache Directory        : {CACHE_DIR}")

    # Step 1: Ingest seed repository
    log_step("STEP 1/5", "Downloading Seed Repository & Aggregator Plugin")
    try:
        seed_bytes = fetch_url(repo_url)
        seed_manifest = json.loads(seed_bytes)
        log_info(f"Manifest Name: '{seed_manifest.get('name', 'Unknown')}'")
        plugin_lists = seed_manifest.get("pluginLists", [])
        if not plugin_lists:
            raise RuntimeError("No pluginLists found in seed repository manifest!")

        seed_plugins = []
        for plist_url in plugin_lists:
            log_info(f"Fetching plugin list: {plist_url}")
            plist_bytes = fetch_url(plist_url)
            seed_plugins.extend(json.loads(plist_bytes))

        if not seed_plugins:
            raise RuntimeError("No plugins found in seed repository!")

        mega_plugin = None
        for p in seed_plugins:
            if "mega" in p.get("internalName", "").lower() or "mega" in p.get("name", "").lower():
                mega_plugin = p
                break
        if not mega_plugin:
            mega_plugin = seed_plugins[0]

        log_info(f"Selected Aggregator Plugin: {mega_plugin.get('name')} ({mega_plugin.get('url')})")
        mega_cs3_dest = CACHE_DIR / f"{mega_plugin.get('internalName', 'Mega')}.cs3"
        download_file(mega_plugin.get("url"), mega_cs3_dest)
        log_success(f"Downloaded aggregator plugin ({mega_cs3_dest.stat().st_size} bytes)")

    except Exception as e:
        log_excessive_error("Step 1 failed: Ingesting seed repository", e, {"repo_url": repo_url})
        log_error(f"Failed to ingest seed repository: {e}")
        raise

    # Step 2: Start sidecar & discover repositories
    log_step("STEP 2/5", "Running Aggregator in Sidecar & Discovering Repositories")
    sidecar_data_dir = CACHE_DIR / "sidecar_seed"
    seed_sidecar = SidecarProcess(sidecar_home, jdk21_home, sidecar_data_dir, worker_id=0)
    discovered_repos: dict[str, dict] = {}
    try:
        ping_reply, _ = seed_sidecar.call("ping", timeout=10)
        log_info(f"Sidecar ping response: {ping_reply.get('result')}")
        seed_sidecar.call("set_debug_logging", {"enabled": True}, timeout=5)

        log_info(f"Loading aggregator plugin: {mega_cs3_dest.name}...")
        load_reply, stderr_log = seed_sidecar.call("load_extension", {"path": str(mega_cs3_dest.resolve()), "trusted": True}, timeout=60)
        log_info(f"Aggregator load reply: {load_reply}")

        log_info("Polling take_added_repositories for newly registered repositories...")
        quiet_polls = 0
        poll_count = 0
        while quiet_polls < 6 and poll_count < 30:
            time.sleep(2.0)
            poll_count += 1
            reply, _ = seed_sidecar.call("take_added_repositories", timeout=10)
            added = reply.get("result", [])
            new_this_round = 0
            for item in added:
                url = item.get("url")
                if url and url not in discovered_repos:
                    discovered_repos[url] = item
                    new_this_round += 1
                    with print_lock:
                        print(f"  {CYAN}-> Discovered Repository:{RESET} {BOLD}{item.get('name')}{RESET} ({url})")
            if new_this_round > 0:
                quiet_polls = 0
            else:
                quiet_polls += 1

        log_success(f"Discovery complete! Found {len(discovered_repos)} repositories.")
        repos_file = REPLIES_DIR / "discovered_repositories.json"
        repos_file.write_text(json.dumps(list(discovered_repos.values()), indent=2), encoding="utf-8")
        log_info(f"Saved discovered repositories to {repos_file.name}")

    except Exception as e:
        log_excessive_error("Step 2 failed: Repository discovery", e, {"mega_cs3": str(mega_cs3_dest)})
        log_error(f"Discovery failed: {e}")
        raise
    finally:
        seed_sidecar.stop()

    # Step 3: Crawl all discovered repositories in parallel
    log_step("STEP 3/5", f"Indexing Plugins from {len(discovered_repos)} Repositories (Parallel)")
    all_plugins: dict[str, dict] = {}
    catalog_lock = threading.Lock()

    def crawl_repo(repo_item):
        target_repo_url, repo_data = repo_item
        repo_name = repo_data.get("name") or target_repo_url
        found_plugins = []
        try:
            m_bytes = fetch_url(target_repo_url, timeout=8, retries=0)
            manifest = json.loads(m_bytes)
            plists = manifest.get("pluginLists", [])
            for pl_url in plists:
                try:
                    p_bytes = fetch_url(pl_url, timeout=8, retries=0)
                    plugins_in_list = json.loads(p_bytes)
                    for item in plugins_in_list:
                        item["_source_repo_name"] = repo_name
                        item["_source_repo_url"] = target_repo_url
                        found_plugins.append(item)
                except Exception:
                    pass
            with print_lock:
                print(f"  [Repo] {BOLD}{repo_name}{RESET}: {GREEN}{len(found_plugins)} plugins{RESET}")
            return found_plugins
        except Exception as ex:
            with print_lock:
                print(f"  [Repo] {BOLD}{repo_name}{RESET}: {RED}Failed ({ex.__class__.__name__}){RESET}")
            log_excessive_error(f"Failed to crawl repository {repo_name}", ex, {"repo_url": target_repo_url})
            return []

    with concurrent.futures.ThreadPoolExecutor(max_workers=min(16, len(discovered_repos) or 1)) as executor:
        futures = [executor.submit(crawl_repo, item) for item in discovered_repos.items()]
        for future in concurrent.futures.as_completed(futures):
            for item in future.result():
                cs3_url = item.get("url")
                int_name = item.get("internalName") or item.get("name")
                key = cs3_url or int_name
                if key and key not in all_plugins:
                    all_plugins[key] = item

    log_success(f"Total unique plugins discovered across all repositories: {len(all_plugins)}")
    catalog_file = REPLIES_DIR / "all_plugins_catalog.json"
    catalog_file.write_text(json.dumps(list(all_plugins.values()), indent=2), encoding="utf-8")
    log_info(f"Saved complete plugin catalog to {catalog_file.name}")

    plugins_to_test = list(all_plugins.values())
    if max_plugins > 0:
        plugins_to_test = plugins_to_test[:max_plugins]
        log_warn(f"Capping test run to first {max_plugins} plugins as requested.")

    # Step 4: Multi-Worker Testing
    log_step("STEP 4/5", f"Testing {len(plugins_to_test)} Plugins with {workers} Sidecar Workers")
    
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
                    {"path": str(cs3_dest.resolve()), "trusted": trusted},
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
    log_step("STEP 5/5", "Generating Final Reports & Analyzing Error Patterns")
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


def main():
    parser = argparse.ArgumentParser(description="CloudStream Extension Response Recorder (High-Performance)")
    parser.add_argument("--repo-url", default=DEFAULT_REPO_URL, help="Root repository URL")
    parser.add_argument("--sidecar-home", default="", help="Path to cs-sidecar install home")
    parser.add_argument("--jdk21", default="", help="Path to JDK 21 home")
    parser.add_argument("--max-plugins", type=int, default=0, help="Limit number of plugins to test (0 = all)")
    parser.add_argument("--skip-download", action="store_true", help="Skip downloading existing cached cs3 files")
    parser.add_argument("--timeout", type=int, default=45, help="Per-plugin load timeout in seconds")
    parser.add_argument("--workers", type=int, default=4, help="Number of concurrent sidecar worker processes")
    parser.add_argument("--untrusted", action="store_true", help="Run with untrusted mode (enables bytecode security verification)")

    args = parser.parse_args()

    try:
        jdk21_path = find_jdk21(args.jdk21)
        sidecar_path = find_sidecar_home(args.sidecar_home)

        run_recorder(
            repo_url=args.repo_url,
            sidecar_home=sidecar_path,
            jdk21_home=jdk21_path,
            max_plugins=args.max_plugins,
            skip_download=args.skip_download,
            timeout_sec=args.timeout,
            workers=max(1, args.workers),
            trusted=not args.untrusted,
        )
    except Exception as fatal_e:
        init_logging_directories()
        log_excessive_error("FATAL SCRIPT ERROR", fatal_e, {"args": vars(args)})
        log_error(f"Fatal error occurred: {fatal_e}")
        print(f"\n{RED}Full error log saved to: {LOGS_DIR / 'latest_errors.log'}{RESET}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
