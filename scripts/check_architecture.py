#!/usr/bin/env python3
"""Enforce RULES.md §2 one-way crate dependency architecture and cycle freedom."""

import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Architectural rules derived from RULES.md §2 & §3:
#   core_model -> (no internal deps)
#   traits (extension_api, player_api) -> only core_model
#   domain/impl leaves -> never depend on services/ui/shell
#   app_services / playback_session -> never depend on concrete runtime/player/proxy/window/ui/shell
#   ui_slint -> never depend on concrete runtime/player/proxy/storage/window/shell
#   app_shell -> wiring root
CORE_CRATES = {"core_model"}
TRAIT_CRATES = {"extension_api", "player_api"}
SERVICE_CRATES = {"app_services", "playback_session"}
UI_CRATES = {"ui_slint"}
SHELL_CRATES = {"app_shell"}
CONCRETE_IMPL_CRATES = {
    "extension_runtime",
    "player_mpv",
    "stream_proxy",
    "cloudflare_window",
}
LEAF_CRATES = {
    "extension_runtime",
    "extension_repo",
    "player_mpv",
    "storage",
    "subtitle_store",
    "skip_times",
    "stream_proxy",
    "title_cache",
    "poster_artwork",
    "memory_budget",
    "app_updater",
    "cloudflare_bridge",
    "cloudflare_window",
    "portable_launcher",
}


def load_workspace_graph(repo_root: Path = REPO_ROOT) -> dict[str, set[str]]:
    raw = subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version=1"],
        cwd=repo_root,
        text=True,
    )
    meta = json.loads(raw)
    ws_ids = set(meta["workspace_members"])
    pkgs = {p["id"]: p for p in meta["packages"] if p["id"] in ws_ids}
    ws_names = {p["name"] for p in pkgs.values()}

    graph: dict[str, set[str]] = {}
    for pkg in pkgs.values():
        manifest = Path(pkg["manifest_path"]).as_posix()
        # Non-product test harnesses (e.g. tests/bdd) are excluded from production layer checks.
        if "/crates/" not in manifest:
            continue
        deps = {
            d["name"]
            for d in pkg["dependencies"]
            if d["name"] in ws_names and d.get("kind") is None
        }
        graph[pkg["name"]] = deps
    return graph


def find_cycles(graph: dict[str, set[str]]) -> list[list[str]]:
    visited: dict[str, int] = defaultdict(int)  # 0=unvisited, 1=visiting, 2=done
    stack: list[str] = []
    cycles: list[list[str]] = []

    def dfs(node: str) -> None:
        visited[node] = 1
        stack.append(node)
        for nxt in sorted(graph.get(node, ())):
            if visited[nxt] == 1:
                idx = stack.index(nxt)
                cycles.append(stack[idx:] + [nxt])
            elif visited[nxt] == 0:
                dfs(nxt)
        stack.pop()
        visited[node] = 2

    for node in sorted(graph):
        if visited[node] == 0:
            dfs(node)
    return cycles


def check_graph(graph: dict[str, set[str]]) -> list[str]:
    problems: list[str] = []
    for cycle in find_cycles(graph):
        problems.append(f"dependency cycle: {' -> '.join(cycle)}")

    for crate, deps in sorted(graph.items()):
        if crate in CORE_CRATES and deps:
            problems.append(f"{crate}: must have zero internal dependencies, found {sorted(deps)}")
        elif crate in TRAIT_CRATES:
            bad = deps - CORE_CRATES
            if bad:
                problems.append(f"{crate}: trait crate may only depend on core_model, found {sorted(bad)}")
        elif crate in LEAF_CRATES:
            forbidden = SERVICE_CRATES | UI_CRATES | SHELL_CRATES
            bad = deps & forbidden
            if bad:
                problems.append(f"{crate}: leaf/impl crate must not depend on {sorted(bad)}")
        elif crate in SERVICE_CRATES:
            forbidden = CONCRETE_IMPL_CRATES | UI_CRATES | SHELL_CRATES
            bad = deps & forbidden
            if bad:
                problems.append(
                    f"{crate}: service crate must not depend on concrete impl/UI/shell {sorted(bad)}"
                )
        elif crate in UI_CRATES:
            forbidden = CONCRETE_IMPL_CRATES | {"storage"} | SHELL_CRATES
            bad = deps & forbidden
            if bad:
                problems.append(
                    f"{crate}: UI crate must not depend on concrete impl/storage/shell {sorted(bad)}"
                )
    return problems


def main() -> int:
    graph = load_workspace_graph()
    problems = check_graph(graph)
    if problems:
        for problem in problems:
            print(f"architecture: {problem}", file=sys.stderr)
        return 1
    print(f"architecture: {len(graph)} production crates obey one-way layering with 0 cycles.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
