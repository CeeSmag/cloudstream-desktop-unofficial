"""Keeps only video plugins, using the app's own rule (`crates/content_policy`).

The rule is not reimplemented here: listings go to the `plugin-content-policy` command (JSON in,
one verdict per listing out), so the tool and the app always agree. Excluded plugins are written
with their reason to `replies/excluded_plugins.json`.
"""

import json
import subprocess
from pathlib import Path

from sidecar_env import REPO_ROOT
from tool_log import REPLIES_DIR, log_info

POLICY_COMMAND = ["cargo", "run", "--quiet", "-p", "content_policy", "--features", "cli",
                  "--bin", "plugin-content-policy"]


def policy_input(listings: list[dict]) -> list[dict]:
    """What the policy reads of each crawled listing."""
    return [{"repositoryUrl": item.get("_source_repo_url", ""),
             "internalName": item.get("internalName") or item.get("name") or "",
             "tvTypes": item.get("tvTypes")} for item in listings]


def verdicts(listings: list[dict], command: list[str] = POLICY_COMMAND, run=subprocess.run) -> list[dict]:
    """One verdict per listing, in order: `{"video": bool, "reason": str, ...}`."""
    completed = run(command, input=json.dumps(policy_input(listings)), capture_output=True, text=True,
                    cwd=REPO_ROOT, check=False)
    if completed.returncode != 0:
        raise RuntimeError(f"plugin-content-policy failed ({completed.returncode}): {completed.stderr.strip()}")
    answer = json.loads(completed.stdout)
    if len(answer) != len(listings):
        raise RuntimeError(f"plugin-content-policy answered {len(answer)} verdicts for {len(listings)} listings")
    return answer


def split(listings: list[dict], answer: list[dict]) -> tuple[list[dict], list[dict]]:
    """(video listings, excluded listings with `_excluded_reason`)."""
    kept, excluded = [], []
    for listing, verdict in zip(listings, answer):
        if verdict.get("video"):
            kept.append(listing)
        else:
            excluded.append({**listing, "_excluded_reason": verdict.get("reason", "")})
    return kept, excluded


def video_plugins(listings: list[dict], command: list[str] = POLICY_COMMAND, run=subprocess.run,
                  out_dir: Path = REPLIES_DIR) -> list[dict]:
    """The video listings; the rest are recorded in `excluded_plugins.json`."""
    kept, excluded = split(listings, verdicts(listings, command, run))
    out_dir.mkdir(parents=True, exist_ok=True)
    summary = [{"internalName": item.get("internalName"), "repositoryUrl": item.get("_source_repo_url"),
                "tvTypes": item.get("tvTypes"), "reason": item["_excluded_reason"]} for item in excluded]
    (out_dir / "excluded_plugins.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    log_info(f"Content policy: {len(kept)} video plugins kept, {len(excluded)} excluded "
             f"(see {out_dir.name}/excluded_plugins.json)")
    return kept
