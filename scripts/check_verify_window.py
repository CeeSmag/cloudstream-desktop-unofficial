"""Check the real Windows verification window against a loopback page.

Run with python scripts/check_verify_window.py path/to/cloudstream-verify.exe.
"""
import argparse
import ctypes
from ctypes import wintypes
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from queue import Queue
import subprocess
import tempfile
from threading import Thread
import time


class Page(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(b"<title>Local verification fixture</title>")

    def log_message(self, *args):
        pass


def visible_windows(pid):
    user32 = ctypes.windll.user32
    windows = {}

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def collect(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, title, len(title))
            if title.value.startswith("Verify "):
                windows[title.value.split(" ")[1]] = hwnd
        return True

    user32.EnumWindows(collect, 0)
    return windows


def read_statuses(stream, statuses):
    for line in stream:
        fields = line.rstrip().split("\t")
        if fields[0] in ("open", "passed", "failed"):
            statuses.put(fields)


def check_windows(proc, folder, port):
    statuses = Queue()
    Thread(target=read_statuses, args=(proc.stdout, statuses), daemon=True).start()
    host = "127.0.0.1"
    def send(*fields):
        proc.stdin.write("\t".join(fields) + "\n")
        proc.stdin.flush()

    def start(command, request_id, site):
        send(command, request_id, site, f"http://{site}:{port}/check")
        assert statuses.get(timeout=20) == ["open", site, request_id]

    def wait_visible(site):
        until = time.monotonic() + 5
        while time.monotonic() < until:
            if window := visible_windows(proc.pid).get(site):
                return window
            time.sleep(0.02)
        raise AssertionError(f"No visible verification window for {site}")

    start("verify", "one", host)
    start("verify", "two", host)
    start("verify", "other", "localhost")
    until = time.monotonic() + 5  # The previous release raised a window after three seconds.
    while time.monotonic() < until:
        assert not visible_windows(proc.pid), "Background verification raised a window"
        time.sleep(0.02)
    send("show", host, "one")
    window = wait_visible(host)
    assert ctypes.windll.user32.GetForegroundWindow() != window, "Show without a foreground grant activated the helper"
    assert set(visible_windows(proc.pid)) == {host}
    ctypes.windll.user32.PostMessageW(window, 0x0010, 0, 0)  # WM_CLOSE
    assert statuses.get(timeout=5)[:2] == ["failed", host]
    for request in ("one", "two"):
        assert "ok=false" in (folder / f"result-{request}.txt").read_text()
    assert not (folder / "result-other.txt").exists()
    send("show", host, "two")  # A click delivered after closing cannot reopen the check.
    send("show", "localhost", "other")
    wait_visible("localhost")
    assert set(visible_windows(proc.pid)) == {"localhost"}
    start("verify", "replacement", host)
    send("show", host, "one")
    start("verify", "barrier", "localhost")
    assert set(visible_windows(proc.pid)) == {"localhost"}
    start("present", "retry", host)
    wait_visible(host)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("helper", type=Path)
    args = parser.parse_args()
    with ThreadingHTTPServer(("127.0.0.1", 0), Page) as server:
        Thread(target=server.serve_forever, daemon=True).start()
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as tmp:
            folder = Path(tmp)
            with subprocess.Popen(
                [args.helper.resolve(), "--exchange", tmp, "--profile", folder / "profile"],
                creationflags=subprocess.CREATE_NO_WINDOW, stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, text=True, encoding="utf-8",
            ) as proc:
                try:
                    check_windows(proc, folder, server.server_port)
                finally:
                    proc.stdin.close()
                    try:
                        proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        raise
        server.shutdown()
    print("Windows verification presentation checks passed")


if __name__ == "__main__":
    main()
