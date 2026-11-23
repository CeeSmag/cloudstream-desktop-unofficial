#!/usr/bin/env python3
"""
CloudStream extension recorder: find what the desktop runtime lacks for the video plugins people use.

1. Crawl: download the seed repository's aggregator (MegaProvider), let it announce its repositories
   in a sidecar, read every plugin list (catalog.py).
2. Filter: keep video plugins only, with the app's own rule (content_filter.py ->
   crates/content_policy); excluded ones land in replies/excluded_plugins.json with their reason.
3. Either
   - scan (default): download them and run the static API gap scanner (api_scan.py ->
     sidecar/api-scanner) -> replies/api_gaps.md / api_gaps.json, ranked by how many plugins need
     each missing member or class; or
   - load: load each one in sidecar workers and record the replies (load_test.py), the older
     load-time audit.

Linux and Windows. See README.md.
"""

import argparse
import sys
from datetime import datetime

import catalog
from api_scan import run_scan
from content_filter import POLICY_COMMAND, video_plugins
from load_test import LoadRun, run_load_test
from sidecar_env import build_api_scanner, find_api_scanner, find_jdk21, find_sidecar_home
from tool_log import (LOGS_DIR, RED, REPLIES_DIR, RESET, init_logging_directories, log_error, log_excessive_error,
                      log_header, log_info, log_step, log_success, log_warn)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.strip().splitlines()[0])
    parser.add_argument("--mode", choices=["scan", "load"], default="scan",
                        help="scan: static API gap report (default); load: load every plugin in sidecars")
    parser.add_argument("--repo-url", default=catalog.DEFAULT_REPO_URL, help="seed repository (repo.json)")
    parser.add_argument("--sidecar-home", default="", help="cs-sidecar distribution (needed to discover repositories)")
    parser.add_argument("--scanner-home", default="", help="api-scanner distribution (scan mode)")
    parser.add_argument("--build-scanner", action="store_true", help="build api-scanner first (Linux)")
    parser.add_argument("--jdk21", default="", help="JDK 21 home")
    parser.add_argument("--max-plugins", type=int, default=0, help="only the first N video plugins (0 = all)")
    parser.add_argument("--timeout", type=int, default=45, help="load mode: per-plugin load timeout, seconds")
    parser.add_argument("--workers", type=int, default=4, help="parallel sidecars (load) or scanner threads (scan)")
    parser.add_argument("--untrusted", action="store_true", help="load mode: load untrusted (bytecode verifier on)")
    return parser.parse_args(argv)


def collect_video_plugins(args: argparse.Namespace, sidecar_home: str, jdk21: str) -> tuple[list[dict], int, int]:
    """(video listings to work on, repositories found, listings indexed)."""
    log_step("STEP 1/3", "Downloading the seed repository's aggregator and discovering repositories")
    aggregator = catalog.download_aggregator(args.repo_url)
    repositories = catalog.discover_repositories(aggregator, sidecar_home, jdk21)
    log_step("STEP 2/3", f"Reading the plugin lists of {len(repositories)} repositories")
    listings = catalog.crawl(repositories)
    plugins = video_plugins(listings, POLICY_COMMAND)
    if args.max_plugins > 0:
        plugins = plugins[: args.max_plugins]
        log_warn(f"Capped to the first {args.max_plugins} video plugins.")
    return plugins, len(repositories), len(listings)


def run(args: argparse.Namespace) -> None:
    started = datetime.now()
    init_logging_directories()
    log_header(f"CloudStream extension recorder ({args.mode} mode)")
    jdk21 = find_jdk21(args.jdk21)
    sidecar_home = find_sidecar_home(args.sidecar_home)
    scanner_home = ""
    if args.mode == "scan":
        scanner_home = build_api_scanner(jdk21) if args.build_scanner else find_api_scanner(args.scanner_home)
    log_info(f"JDK 21: {jdk21} | sidecar: {sidecar_home}" + (f" | scanner: {scanner_home}" if scanner_home else ""))
    plugins, repositories, indexed = collect_video_plugins(args, sidecar_home, jdk21)
    if args.mode == "scan":
        log_step("STEP 3/3", f"Scanning {len(plugins)} video plugins")
        report = run_scan(plugins, scanner_home, jdk21, workers=max(1, args.workers))
        log_success(f"API gap report: {report}")
        return
    log_step("STEP 3/3", f"Loading {len(plugins)} video plugins")
    run_load_test(plugins, LoadRun(
        repo_url=args.repo_url, sidecar_home=sidecar_home, jdk21_home=jdk21, workers=max(1, args.workers),
        timeout_sec=args.timeout, trusted=not args.untrusted, start_time=started,
        repositories_found=repositories, plugins_indexed=indexed,
    ))
    log_success(f"Load report: {REPLIES_DIR / 'summary.md'}")


def main() -> None:
    args = parse_args()
    try:
        run(args)
    except Exception as fatal:
        init_logging_directories()
        log_excessive_error("FATAL SCRIPT ERROR", fatal, {"args": vars(args)})
        log_error(f"Fatal error occurred: {fatal}")
        print(f"\n{RED}Full error log saved to: {LOGS_DIR / 'latest_errors.log'}{RESET}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
