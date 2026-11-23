"""HTTP downloads with retries and a cache-friendly atomic write (moved from recorder.py)."""

import os
import threading
import time
import urllib.parse
import urllib.request
from pathlib import Path

from tool_log import USER_AGENT, log_excessive_error, log_trace


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

