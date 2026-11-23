"""From a seed repository to every plugin listing: download the aggregator (MegaProvider), let it
announce its repositories in a sidecar, then read each repository's plugin lists.

Moved from recorder.py (steps 1-3); the logic is unchanged, split into functions that can be tested
with a fake fetcher.
"""

import concurrent.futures
import json
import time
from pathlib import Path

from http_fetch import download_file, fetch_url
from sidecar_process import SidecarProcess
from tool_log import (BOLD, CACHE_DIR, CYAN, GREEN, RED, REPLIES_DIR, RESET, log_excessive_error, log_info,
                      log_success, print_lock)

DEFAULT_REPO_URL = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json"


def plugin_lists(manifest_url: str, fetch=fetch_url, **fetch_options) -> list[dict]:
    """Every listing of every plugin list a repository manifest names."""
    manifest = json.loads(fetch(manifest_url, **fetch_options))
    listings = []
    for list_url in manifest.get("pluginLists", []):
        try:
            listings.extend(json.loads(fetch(list_url, **fetch_options)))
        except Exception as error:  # one broken list never hides the others
            log_excessive_error(f"Unreadable plugin list {list_url}", error, {"repo": manifest_url})
    return listings


def download_aggregator(repo_url: str, fetch=fetch_url, download=download_file) -> Path:
    """The seed repository's aggregator plugin (the one named like "mega", else the first), downloaded."""
    listings = plugin_lists(repo_url, fetch)
    if not listings:
        raise RuntimeError("No plugins found in seed repository!")
    mega = next((p for p in listings
                 if "mega" in (p.get("internalName", "") + p.get("name", "")).lower()), listings[0])
    log_info(f"Selected Aggregator Plugin: {mega.get('name')} ({mega.get('url')})")
    destination = CACHE_DIR / f"{mega.get('internalName', 'Mega')}.cs3"
    download(mega.get("url"), destination)
    return destination


def discover_repositories(aggregator: Path, sidecar_home: str, jdk21: str) -> dict[str, dict]:
    """Loads the aggregator in a sidecar and collects the repositories it adds until it goes quiet."""
    sidecar = SidecarProcess(sidecar_home, jdk21, CACHE_DIR / "sidecar_seed", worker_id=0)
    found: dict[str, dict] = {}
    try:
        sidecar.call("ping", timeout=10)
        loaded, _ = sidecar.call("load_extension", {"path": str(sidecar.stage(aggregator)), "trusted": True},
                                      timeout=60)
        if "error" in loaded:
            # Without the aggregator there is nothing to discover: stop instead of reporting "0 found".
            raise RuntimeError(f"the sidecar refused the aggregator {aggregator.name}: {loaded['error']}")
        quiet, polls = 0, 0
        while quiet < 6 and polls < 30:
            time.sleep(2.0)
            polls += 1
            reply, _ = sidecar.call("take_added_repositories", timeout=10)
            new = [item for item in reply.get("result", []) if item.get("url") and item["url"] not in found]
            for item in new:
                found[item["url"]] = item
                with print_lock:
                    print(f"  {CYAN}-> Discovered Repository:{RESET} {BOLD}{item.get('name')}{RESET} ({item['url']})")
            quiet = 0 if new else quiet + 1
    finally:
        sidecar.stop()
    log_success(f"Discovery complete! Found {len(found)} repositories.")
    (REPLIES_DIR / "discovered_repositories.json").write_text(json.dumps(list(found.values()), indent=2),
                                                             encoding="utf-8")
    return found


def crawl(repositories: dict[str, dict], fetch=fetch_url) -> list[dict]:
    """Every listing of every repository, once per download address, each tagged with its repository
    (`_source_repo_url`, `_source_repo_name`)."""

    def listings_of(item) -> list[dict]:
        url, data = item
        name = data.get("name") or url
        try:
            found = plugin_lists(url, fetch, timeout=8, retries=0)
        except Exception as error:
            with print_lock:
                print(f"  [Repo] {BOLD}{name}{RESET}: {RED}Failed ({error.__class__.__name__}){RESET}")
            log_excessive_error(f"Failed to crawl repository {name}", error, {"repo_url": url})
            return []
        with print_lock:
            print(f"  [Repo] {BOLD}{name}{RESET}: {GREEN}{len(found)} plugins{RESET}")
        return [{**listing, "_source_repo_name": name, "_source_repo_url": url}
                for listing in found if isinstance(listing, dict)]

    unique: dict[str, dict] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=min(16, len(repositories) or 1)) as pool:
        for listings in pool.map(listings_of, repositories.items()):
            for listing in listings:
                key = listing.get("url") or listing.get("internalName") or listing.get("name")
                if key and key not in unique:
                    unique[key] = listing
    log_success(f"Total unique plugins discovered across all repositories: {len(unique)}")
    (REPLIES_DIR / "all_plugins_catalog.json").write_text(json.dumps(list(unique.values()), indent=2),
                                                          encoding="utf-8")
    return list(unique.values())
