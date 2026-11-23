"""Scan mode: download the video plugins and run the static API gap scanner over them.

The scanner (sidecar/api-scanner) reads each plugin's bytecode against the runtime's classes and
writes `replies/api_gaps.json` and `replies/api_gaps.md`: what to implement in the stubs, ranked by
how many plugins need it. Nothing of the plugins runs.
"""

import concurrent.futures
import json
import os
import subprocess
from pathlib import Path

from http_fetch import clean_url, download_file
from sidecar_env import scanner_launcher
from tool_log import CACHE_DIR, REPLIES_DIR, log_excessive_error, log_info, log_warn, sanitize_filename

PLUGINS_DIR = CACHE_DIR / "plugins"


def cache_name(listing: dict) -> str:
    """The file a listing is downloaded to: `<internalName>_<version>.cs3`, safe on every OS."""
    name = listing.get("internalName") or listing.get("name") or "plugin"
    return f"{sanitize_filename(name)}_{listing.get('version', 0)}.cs3"


def download_all(listings: list[dict], folder: Path = PLUGINS_DIR, download=download_file,
                 workers: int = 8) -> dict[str, dict]:
    """Downloads each listing (cached by name and version); returns file name -> listing for the
    ones that arrived. A failed download is logged and skipped."""

    def fetch(listing: dict):
        destination = folder / cache_name(listing)
        try:
            download(clean_url(listing.get("url", "")), destination, timeout=25)
            return destination.name, listing
        except Exception as error:
            log_excessive_error(f"Download failed: {listing.get('internalName')}", error, {"url": listing.get("url")})
            return None

    folder.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        arrived = [item for item in pool.map(fetch, listings) if item]
    missing = len(listings) - len(arrived)
    if missing:
        log_warn(f"{missing} plugins could not be downloaded (see the error log)")
    return dict(arrived)


def identities(files: dict[str, dict]) -> dict[str, dict]:
    """The scanner's `--identities` content: file name -> internal name and repository."""
    return {name: {"internalName": listing.get("internalName") or name,
                   "repositoryUrl": listing.get("_source_repo_url", "")} for name, listing in files.items()}


def scanner_command(scanner_home: str, out_dir: Path, ids_file: Path, folder: Path, workers: int) -> list[str]:
    return [scanner_launcher(scanner_home), "--out", str(out_dir), "--workers", str(workers),
            "--identities", str(ids_file), str(folder)]


def run_scan(listings: list[dict], scanner_home: str, jdk21: str, workers: int = 4,
             run=subprocess.run, download=download_file) -> Path:
    """Downloads `listings` and scans them; returns the Markdown report."""
    files = download_all(listings, download=download)
    if not files:
        raise RuntimeError("No plugin could be downloaded; nothing to scan.")
    # Scan exactly this run's files, not whatever older runs left in the cache.
    batch = CACHE_DIR / "scan-batch"
    if batch.exists():
        for old in batch.iterdir():
            old.unlink()
    batch.mkdir(parents=True, exist_ok=True)
    for name in files:
        (batch / name).write_bytes((PLUGINS_DIR / name).read_bytes())
    REPLIES_DIR.mkdir(parents=True, exist_ok=True)
    ids_file = REPLIES_DIR / "plugin_identities.json"
    ids_file.write_text(json.dumps(identities(files), indent=2), encoding="utf-8")
    log_info(f"Scanning {len(files)} plugins with the API scanner...")
    command = scanner_command(scanner_home, REPLIES_DIR, ids_file, batch, workers)
    run(command, env={**os.environ, "JAVA_HOME": jdk21}, check=True)
    return REPLIES_DIR / "api_gaps.md"
