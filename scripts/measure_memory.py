#!/usr/bin/env python3
"""Measure the memory of CloudStream's whole process tree (docs/MEMORY.md).

The app is several processes: `cloudstream`, one `java` sidecar per resident plugin, the
Cloudflare helper (`cloudstream-verify`) and its WebKit / WebView2 children. Looking at one
process hides most of the cost, so this samples the tree under a root pid and reports:

* Linux: PSS (proportional set size, shared pages counted once) from /proc/<pid>/smaps_rollup.
* Windows: working set (PowerShell / CIM), which is an upper bound.

Usage:
    measure_memory.py --launch --label S1 --duration 120 -- ./cloudstream
    measure_memory.py --pid 12345 --label S3 --duration 600 --max-total-mib 700

Exit status is 1 when the peak exceeds --max-total-mib (used by the nightly job).
"""

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

KIB_PER_MIB = 1024.0
TICKS_PER_SECOND = 100.0  # Linux clock ticks (USER_HZ)
BROWSER_HELPER_MARKERS = ("cloudstream-verify", "webkit", "webview2")


@dataclass(frozen=True)
class ProcessSample:
    """One process at one instant."""

    pid: int
    parent_pid: int
    name: str
    pss_kib: int
    cpu_ticks: int = 0
    start_ticks: int = 0  # start time since boot, clock ticks


@dataclass(frozen=True)
class TimedSample:
    """The tree's memory and cumulative CPU time at `t` seconds."""

    t: float
    total_mib: float
    cpu_seconds: float
    # role -> (memory MiB, cumulative CPU seconds) of that kind of process
    roles: dict = field(default_factory=dict)
    # process name -> (memory MiB, process count) at this instant
    processes: dict = field(default_factory=dict)
    # every browser-helper process at this instant: pid, name, MiB, age in seconds
    helper_procs: list = field(default_factory=list)


class CpuTotal:
    """Cumulative CPU time of every process ever seen in the tree (one that exits keeps its time)."""

    def __init__(self):
        self._ticks = {}

    def add(self, tree):
        for sample in tree:
            self._ticks[sample.pid] = max(self._ticks.get(sample.pid, 0), sample.cpu_ticks)

    def seconds(self):
        return sum(self._ticks.values()) / TICKS_PER_SECOND


@dataclass(frozen=True)
class Summary:
    """Memory of a whole tree at one instant."""

    total_mib: float
    by_role_mib: dict = field(default_factory=dict)
    jvm_count: int = 0
    # process name -> (memory MiB, process count)
    by_name: dict = field(default_factory=dict)


def parse_pss_kib(smaps_rollup_text: str) -> int:
    """Reads the `Pss:` total from /proc/<pid>/smaps_rollup text (0 when absent)."""
    for line in smaps_rollup_text.splitlines():
        if line.startswith("Pss:"):
            parts = line.split()
            return int(parts[1]) if len(parts) >= 2 and parts[1].isdigit() else 0
    return 0


def parse_cpu_ticks(stat_text: str) -> int:
    """User + system CPU ticks from /proc/<pid>/stat (0 when unreadable). The name may contain spaces."""
    after_name = stat_text.rsplit(")", 1)[-1].split()
    try:
        return int(after_name[11]) + int(after_name[12])
    except (IndexError, ValueError):
        return 0


def parse_start_ticks(stat_text: str) -> int:
    """Start time (ticks since boot) from /proc/<pid>/stat (0 when unreadable)."""
    after_name = stat_text.rsplit(")", 1)[-1].split()
    try:
        return int(after_name[19])
    except (IndexError, ValueError):
        return 0


def helper_detail(tree: list, now_ticks: int) -> list:
    """One row per browser-helper process (the helper and its WebKit / WebView2 children)."""
    return [
        {
            "pid": sample.pid,
            "name": sample.name,
            "mib": round(sample.pss_kib / KIB_PER_MIB, 1),
            "age_s": round((now_ticks - sample.start_ticks) / TICKS_PER_SECOND, 1),
        }
        for sample in tree
        if role_of(sample.name) == "browser_helper"
    ]


def read_uptime_ticks() -> int:
    """Time since boot in clock ticks (0 when unreadable, e.g. on Windows)."""
    try:
        return int(float(Path("/proc/uptime").read_text().split()[0]) * TICKS_PER_SECOND)
    except (OSError, ValueError, IndexError):
        return 0


def window_cpu_percent(series: list, window_s: float) -> float:
    """Average CPU use (percent of one core) over the last `window_s` seconds of the series."""
    if len(series) < 2:
        return 0.0
    last = series[-1]
    first = next((s for s in series if last.t - s.t <= window_s), series[0])
    span = last.t - first.t
    return 100.0 * (last.cpu_seconds - first.cpu_seconds) / span if span > 0 else 0.0


def window_role_stats(series: list, window_s: float) -> dict:
    """Per role over the last `window_s` seconds: average memory (MiB) and CPU (percent of a core)."""
    if len(series) < 2:
        return {}
    last = series[-1]
    inside = [s for s in series if last.t - s.t <= window_s] or series[-1:]
    first = inside[0]
    span = last.t - first.t
    stats = {}
    for role in last.roles:
        mibs = [s.roles[role][0] for s in inside if role in s.roles]
        cpu_now = last.roles[role][1]
        cpu_then = first.roles.get(role, (0.0, 0.0))[1]
        percent = 100.0 * (cpu_now - cpu_then) / span if span > 0 else 0.0
        stats[role] = {"mib_avg": sum(mibs) / len(mibs), "cpu_percent": percent}
    return stats


def window_process_stats(series: list, window_s: float) -> dict:
    """Per process name over the last `window_s` seconds: average memory (MiB) and highest count."""
    if not series:
        return {}
    last = series[-1]
    inside = [s for s in series if last.t - s.t <= window_s] or series[-1:]
    names = {name for sample in inside for name in sample.processes}
    stats = {}
    for name in sorted(names):
        points = [s.processes.get(name, (0.0, 0)) for s in inside]
        stats[name] = {
            "mib_avg": sum(mib for mib, _ in points) / len(points),
            "count_max": max(count for _, count in points),
        }
    return stats


def descendants_of(samples: list, root_pid: int) -> list:
    """The root process and everything below it (cycle-safe, unknown root gives [])."""
    by_pid = {sample.pid: sample for sample in samples}
    if root_pid not in by_pid:
        return []
    found = {root_pid}
    grew = True
    while grew:
        grew = False
        for sample in samples:
            if sample.parent_pid in found and sample.pid not in found:
                found.add(sample.pid)
                grew = True
    return [by_pid[pid] for pid in sorted(found)]


def role_of(process_name: str) -> str:
    """Groups a process by what it is for: app, jvm, browser_helper or other."""
    name = process_name.lower()
    if any(marker in name for marker in BROWSER_HELPER_MARKERS):
        return "browser_helper"
    if name.startswith(("java", "cloudstream-jvm")):
        return "jvm"
    if name.startswith("cloudstream"):
        return "app"
    return "other"


def summarize(tree: list) -> Summary:
    """Totals the tree in MiB, grouped by role, and counts the JVMs."""
    by_role: dict = {}
    by_name: dict = {}
    for sample in tree:
        role = role_of(sample.name)
        mib = sample.pss_kib / KIB_PER_MIB
        by_role[role] = by_role.get(role, 0.0) + mib
        seen_mib, seen_count = by_name.get(sample.name, (0.0, 0))
        by_name[sample.name] = (seen_mib + mib, seen_count + 1)
    jvms = sum(1 for sample in tree if role_of(sample.name) == "jvm")
    return Summary(
        total_mib=sum(by_role.values()), by_role_mib=by_role, jvm_count=jvms, by_name=by_name
    )


def peak_of(summaries: list) -> Summary:
    """The sample with the largest total, with the highest JVM count seen."""
    if not summaries:
        return Summary(total_mib=0.0)
    biggest = max(summaries, key=lambda summary: summary.total_mib)
    jvms = max(summary.jvm_count for summary in summaries)
    return Summary(biggest.total_mib, dict(biggest.by_role_mib), jvms, dict(biggest.by_name))


def check_budget(peak_total_mib: float, max_total_mib) -> list:
    """Failure messages when the peak is over budget (none when there is no budget)."""
    if max_total_mib is None or peak_total_mib <= max_total_mib:
        return []
    return [
        f"peak {peak_total_mib:.1f} MiB exceeds the budget of {max_total_mib:.1f} MiB"
    ]


def read_linux_snapshot() -> list:
    """Every readable process with its PSS from /proc."""
    samples = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        sample = _read_linux_process(entry)
        if sample is not None:
            samples.append(sample)
    return samples


def _read_linux_process(entry: Path):
    try:
        status = (entry / "status").read_text(errors="replace")
        fields = dict(
            line.split(":", 1) for line in status.splitlines() if ":" in line
        )
        pss = parse_pss_kib((entry / "smaps_rollup").read_text(errors="replace"))
        stat = (entry / "stat").read_text(errors="replace")
        ticks = parse_cpu_ticks(stat)
        return ProcessSample(
            pid=int(entry.name),
            parent_pid=int(fields.get("PPid", "0").strip() or 0),
            name=fields.get("Name", "").strip(),
            pss_kib=pss,
            cpu_ticks=ticks,
            start_ticks=parse_start_ticks(stat),
        )
    except (OSError, ValueError):
        return None  # the process ended or belongs to someone else


WINDOWS_QUERY = (
    "Get-CimInstance Win32_Process | "
    "Select-Object ProcessId,ParentProcessId,Name,WorkingSetSize | ConvertTo-Json"
)


def read_windows_snapshot() -> list:
    """Every process with its working set (an upper bound of PSS)."""
    output = subprocess.run(
        ["powershell.exe", "-NoLogo", "-Command", WINDOWS_QUERY],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    rows = json.loads(output or "[]")
    rows = rows if isinstance(rows, list) else [rows]
    return [
        ProcessSample(
            pid=int(row["ProcessId"]),
            parent_pid=int(row["ParentProcessId"]),
            name=str(row["Name"]),
            pss_kib=int(row["WorkingSetSize"] or 0) // 1024,
        )
        for row in rows
    ]


def read_snapshot() -> list:
    """Platform dispatch for one snapshot of all processes."""
    return read_windows_snapshot() if os.name == "nt" else read_linux_snapshot()


def sample_for(root_pid: int, duration_s: float, interval_s: float):
    """Summaries of the tree under `root_pid`, one per interval, and the timed series."""
    summaries, series, cpu = [], [], CpuTotal()
    role_cpu = {}
    start = time.monotonic()
    deadline = start + duration_s
    while True:
        tree = descendants_of(read_snapshot(), root_pid)
        if not tree:
            break  # the app exited
        summary = summarize(tree)
        cpu.add(tree)
        for role in summary.by_role_mib:
            role_cpu.setdefault(role, CpuTotal()).add([s for s in tree if role_of(s.name) == role])
        summaries.append(summary)
        now = time.monotonic()
        roles = {
            role: (mib, role_cpu[role].seconds()) for role, mib in summary.by_role_mib.items()
        }
        series.append(
            TimedSample(
                now - start,
                summary.total_mib,
                cpu.seconds(),
                roles,
                summary.by_name,
                helper_detail(tree, read_uptime_ticks()),
            )
        )
        if now >= deadline:
            break
        time.sleep(interval_s)
    return summaries, series


def series_row(sample: TimedSample) -> dict:
    """One point of the time series: total, CPU, and each role's and process's memory."""
    row = {
        "t": round(sample.t, 1),
        "total_mib": round(sample.total_mib, 1),
        "cpu_s": round(sample.cpu_seconds, 2),
        "roles_mib": {role: round(mib, 1) for role, (mib, _) in sample.roles.items()},
        "processes": {
            name: [round(mib, 1), count] for name, (mib, count) in sample.processes.items()
        },
    }
    if sample.helper_procs:
        row["helper_procs"] = sample.helper_procs
    return row


def render_report(label: str, summaries: list, series=None, window_s: float = 30.0) -> dict:
    """The report written to stdout / --json-out."""
    series = series or []
    peak = peak_of(summaries)
    final = summaries[-1] if summaries else Summary(total_mib=0.0)
    return {
        "label": label,
        "samples": len(summaries),
        "peak_total_mib": round(peak.total_mib, 1),
        "peak_by_role_mib": {k: round(v, 1) for k, v in peak.by_role_mib.items()},
        "peak_jvm_count": peak.jvm_count,
        "peak_by_process": {
            name: {"mib": round(mib, 1), "count": count}
            for name, (mib, count) in sorted(peak.by_name.items())
        },
        "final_total_mib": round(final.total_mib, 1),
        "metric": "working_set" if os.name == "nt" else "pss",
        "window_s": window_s,
        "window_cpu_percent": round(window_cpu_percent(series, window_s), 1),
        "window_roles": {
            role: {key: round(value, 1) for key, value in stat.items()}
            for role, stat in window_role_stats(series, window_s).items()
        },
        "window_processes": {
            name: {key: round(value, 1) for key, value in stat.items()}
            for name, stat in window_process_stats(series, window_s).items()
        },
        "series": [series_row(s) for s in series],
    }


def parse_args(argv: list):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--pid", type=int, help="root process to follow")
    parser.add_argument("--launch", action="store_true", help="start the command after --")
    parser.add_argument("--label", default="run", help="scenario name, e.g. S1")
    parser.add_argument("--duration", type=float, default=60.0, help="seconds")
    parser.add_argument("--interval", type=float, default=2.0, help="seconds")
    parser.add_argument("--max-total-mib", type=float, default=None)
    parser.add_argument("--json-out", type=Path, default=None)
    parser.add_argument("--window", type=float, default=30.0, help="seconds for the CPU average")
    parser.add_argument("command", nargs="*", help="command to launch (after --)")
    return parser.parse_args(argv)


def main(argv: list) -> int:
    args = parse_args(argv)
    child = None
    if args.launch:
        if not args.command:
            print("--launch needs a command after --", file=sys.stderr)
            return 2
        child = subprocess.Popen(args.command)
        args.pid = child.pid
    if args.pid is None:
        print("give --pid or --launch -- <command>", file=sys.stderr)
        return 2
    try:
        summaries, series = sample_for(args.pid, args.duration, args.interval)
    finally:
        if child is not None:
            child.terminate()
    report = render_report(args.label, summaries, series, args.window)
    print(json.dumps(report, indent=2))
    if args.json_out:
        args.json_out.write_text(json.dumps(report, indent=2) + "\n")
    failures = check_budget(report["peak_total_mib"], args.max_total_mib)
    for failure in failures:
        print(f"FAIL {args.label}: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
