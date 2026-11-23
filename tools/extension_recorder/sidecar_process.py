"""One cs-sidecar JVM spoken to over JSON lines (moved from recorder.py; launch is now cross-platform)."""

import json
import os
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from queue import Empty, Queue

from sidecar_env import java_options, sidecar_launcher
from tool_log import log_excessive_error, log_trace


class SidecarProcess:
    """A sidecar with its own workspace, laid out as the app's data folder: `<workspace>/sidecar`
    is its data folder and `<workspace>/extensions` holds the plugins it may load (the sidecar's
    path jail refuses any other place). Put a plugin there with `stage` before `load_extension`."""

    def __init__(self, sidecar_home: str, java_home: str, workspace: Path, worker_id: int = 0):
        self.sidecar_home = sidecar_home
        self.java_home = java_home
        self.workspace = workspace
        self.data_dir = workspace / "sidecar"
        self.extensions_dir = workspace / "extensions"
        self.worker_id = worker_id
        self.proc: subprocess.Popen | None = None
        self.next_id = 1
        self.stderr_queue: Queue[str] = Queue()
        self.stderr_history: list[str] = []
        self.history_lock = threading.Lock()
        self.stderr_thread: threading.Thread | None = None
        self.start()

    def stage(self, cs3: Path) -> Path:
        """Copies a plugin into this sidecar's extensions folder; returns the path to load."""
        self.extensions_dir.mkdir(parents=True, exist_ok=True)
        staged = self.extensions_dir / cs3.name
        if not staged.exists() or staged.stat().st_size != cs3.stat().st_size:
            shutil.copyfile(cs3, staged)
        return staged.resolve()

    def start(self):
        launcher = sidecar_launcher(self.sidecar_home)
        env = os.environ.copy()
        env["JAVA_HOME"] = self.java_home
        self.data_dir.mkdir(parents=True, exist_ok=True)
        env["JAVA_OPTS"] = java_options(self.data_dir)
        log_trace(f"[Worker-{self.worker_id}] Spawning sidecar: {launcher}")
        self.proc = subprocess.Popen(
            [launcher],
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
