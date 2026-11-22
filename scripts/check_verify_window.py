"""Exercise the native verification helper with an isolated HTTP origin and desktop.

Run with python scripts/check_verify_window.py path/to/cloudstream-verify.
On Linux run under X11 with xdotool and a window manager.
"""

import argparse
import os
import subprocess
import tempfile
import time
from collections import defaultdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from queue import Empty, Queue
from threading import Event, Thread
from urllib.parse import urlsplit


HOST = "127.0.0.1"


class Page(BaseHTTPRequestHandler):
    def do_GET(self):
        address = urlsplit(self.path)
        if address.path.startswith("/ready/"):
            key = address.path.removeprefix("/ready/")
            self.html("yes" if self.server.ready[key].is_set() else "no")
        elif address.path.startswith("/challenge/"):
            self.challenge(address.path, address.query == "solved=1")
        elif address.path == "/attachment":
            self.html("fixture download", headers={"Content-Disposition": "attachment; filename=fixture-download.txt"})
        elif address.path == "/exposed":
            self.server.exposed.set()
            self.html("")
        else:
            self.html("""<title>Just a moment</title><body style="background:#fa00fb">
            <script>
            function report() {
                if (document.visibilityState === 'visible') fetch('/exposed');
            }
            report();
            document.addEventListener('visibilitychange', report);
            </script>""")

    def challenge(self, path, solved):
        mode = path.rsplit("/", 1)[-1]
        if not solved:
            self.html(f"""<title>Verification fixture</title><script>
            const poll = setInterval(async () => {{
                if (await (await fetch('/ready/{mode}')).text() === 'yes') {{
                    clearInterval(poll);
                    location.search = '?solved=1';
                }}
            }}, 40);
            </script>""", 403, {"cf-mitigated": "challenge"})
            return
        cookie = {"Set-Cookie": f"cf_clearance={mode}; Path={path}; HttpOnly"}
        if mode in ("same", "second", "delivery"):
            self.html("<title>Verified content</title>", headers=cookie)
        elif mode == "script":
            self.html(
                f"<script>location.replace('http://localhost:{self.server.server_port}/landing')</script>",
                headers=cookie,
            )
        else:
            location = f"http://localhost:{self.server.server_port}/landing"
            headers = {} if mode == "unproven" else cookie
            self.html("", 302, {**headers, "Location": location})

    def html(self, body, status=200, headers=None):
        self.send_response(status)
        self.send_header("Content-Type", "text/html")
        self.send_header("Cache-Control", "no-store")
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        try:
            self.wfile.write(body.encode())
        except (BrokenPipeError, ConnectionResetError):
            pass  # The fixture expects the browser to abort unapproved responses.

    def log_message(self, *args):
        pass


def visible_windows(pid):
    if os.name != "nt":
        result = subprocess.run(
            ["xdotool", "search", "--onlyvisible", "--all", "--pid", str(pid), "--name", "^Verify "],
            capture_output=True, text=True, check=False,
        )
        return {int(window) for window in result.stdout.split()}
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    windows = set()

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def collect(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, title, len(title))
            if title.value.startswith("Verify "):
                windows.add(hwnd)
        return True

    user32.EnumWindows(collect, 0)
    return windows


def close_window(handle):
    if os.name == "nt":
        import ctypes
        ctypes.windll.user32.PostMessageW(handle, 0x0010, 0, 0)  # WM_CLOSE
    else:
        subprocess.run(["xdotool", "windowactivate", "--sync", str(handle), "key", "alt+F4"], check=True, timeout=5)


class Helper:
    def __init__(self, proc, folder, server):
        self.proc = proc
        self.folder = folder
        self.server = server
        self.statuses = Queue()
        self.logs = []
        self.reader = Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        for line in self.proc.stdout:
            fields = line.rstrip().split("\t")
            if fields[0] in ("open", "passed", "failed"):
                self.statuses.put(fields)
            else:
                self.logs.append(line)

    def send(self, *fields):
        self.proc.stdin.write("\t".join(fields) + "\n")
        self.proc.stdin.flush()

    def expect(self, kind, request, timeout=10):
        try:
            status = self.statuses.get(timeout=timeout)
        except Empty as error:
            raise AssertionError(f"No {kind} status for {request}") from error
        assert status[:3] == [kind, HOST, request], status
        return status[3:]

    def start(self, request, path=None, mode="present"):
        path = path or f"/challenge/{request}"
        self.send(mode, request, HOST, f"http://{HOST}:{self.server.server_port}{path}", "GET", "0")
        self.expect("open", request, timeout=25)

    def wait_visible(self, count=1):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            windows = visible_windows(self.proc.pid)
            if len(windows) == count:
                return windows
            time.sleep(0.02)
        raise AssertionError(f"Expected {count} visible verification windows")

    def result(self, request):
        return (self.folder / f"result-{request}.txt").read_text()


def check_redirect(helper):
    helper.start("cross", mode="verify")
    assert not visible_windows(helper.proc.pid)
    helper.send("show", HOST, "cross")
    window, = helper.wait_visible()
    if os.name == "nt":
        import ctypes
        assert ctypes.windll.user32.GetForegroundWindow() != window, "Show activated without a grant"
    helper.server.ready["cross"].set()
    helper.expect("passed", "cross")
    helper.wait_visible(0)
    assert "cookie=cf_clearance=cross\n" in helper.result("cross")
    helper.send("show", HOST, "cross")
    assert not helper.server.exposed.is_set(), "The redirect destination was visible"


def check_other_pages(helper):
    for request, path in [("ordinary", "/landing"), ("attachment", "/attachment")]:
        helper.start(request, path)
        helper.expect("failed", request)
        assert not visible_windows(helper.proc.pid)
        assert "ok=false" in helper.result(request)
    assert not helper.server.exposed.is_set(), "An ordinary document was visible"
    assert not (helper.folder / "fixture-download.txt").exists()


def check_script_redirect(helper):
    helper.start("script")
    helper.wait_visible()
    helper.server.ready["script"].set()
    helper.expect("passed", "script")
    helper.wait_visible(0)
    assert "cookie=cf_clearance=script\n" in helper.result("script")
    assert not helper.server.exposed.is_set()


def check_terminal_order(helper):
    helper.start("first")
    window, = helper.wait_visible()
    helper.start("second")
    passed_window, = helper.wait_visible(2) - {window}
    helper.server.ready["second"].set()
    helper.expect("passed", "second")
    assert helper.wait_visible() == {window}
    assert "cookie=cf_clearance=second\n" in helper.result("second")
    helper.send("show", HOST, "second")
    if os.name == "nt":
        # A queued close for the kept browser context must not overwrite the passed result.
        close_window(passed_window)
    close_window(window)
    assert "viewer closed" in " ".join(helper.expect("failed", "first"))
    helper.send("show", HOST, "first")
    helper.wait_visible(0)
    assert "ok=false" in helper.result("first")


def check_failed_delivery(helper):
    helper.start("delivery")
    helper.wait_visible()
    (helper.folder / "result-delivery.tmp").mkdir()
    helper.server.ready["delivery"].set()
    assert "could not be delivered" in " ".join(helper.expect("failed", "delivery"))
    helper.wait_visible(0)
    assert not (helper.folder / "result-delivery.txt").exists()


def check_unproven_redirect(helper):
    helper.start("unproven")
    helper.wait_visible()
    helper.server.ready["unproven"].set()
    assert "could not confirm" in " ".join(helper.expect("failed", "unproven"))
    helper.wait_visible(0)
    assert "ok=false" in helper.result("unproven")
    assert not helper.server.exposed.is_set()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("helper", type=Path)
    parser.add_argument("--redirect-only", action="store_true")
    args = parser.parse_args()
    with ThreadingHTTPServer((HOST, 0), Page) as server:
        server.ready = defaultdict(Event)
        server.exposed = Event()
        Thread(target=server.serve_forever, daemon=True).start()
        try:
            run_helper(args, server)
        finally:
            server.shutdown()
    print("Native verification window checks passed")


def run_helper(args, server):
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temporary:
        folder = Path(temporary)
        flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
        with subprocess.Popen(
            [args.helper.resolve(), "--exchange", folder, "--profile", folder / "profile", "--debug"],
            cwd=folder, creationflags=flags, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, text=True, encoding="utf-8",
        ) as proc:
            helper = Helper(proc, folder, server)
            try:
                check_redirect(helper)
                if not args.redirect_only:
                    check_other_pages(helper)
                    check_script_redirect(helper)
                    check_terminal_order(helper)
                    check_unproven_redirect(helper)
                    check_failed_delivery(helper)
            except Exception:
                print("".join(helper.logs))
                raise
            finally:
                proc.stdin.close()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    raise


if __name__ == "__main__":
    main()
