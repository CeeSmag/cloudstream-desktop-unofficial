"""Terminal output and log files of the recorder (moved from recorder.py, unchanged in behaviour)."""

import json
import re
import sys
import threading
import traceback
from datetime import datetime
from pathlib import Path

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

