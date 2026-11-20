#!/usr/bin/env python3
"""Parse cargo-llvm-cov JSON output, print per-crate coverage, and enforce thresholds."""

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = REPO_ROOT / "scripts" / "coverage_config.json"
DEFAULT_JSON = REPO_ROOT / "target" / "coverage" / "summary.json"


def load_config(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def normalize_relpath(filename: str, repo_root: Path) -> str:
    try:
        rel = Path(filename).resolve().relative_to(repo_root.resolve())
        return rel.as_posix()
    except ValueError:
        return filename.replace("\\", "/")


def crate_of_relpath(relpath: str) -> str | None:
    parts = relpath.split("/")
    if len(parts) >= 3 and parts[0] == "crates":
        return parts[1]
    return None


def is_ignored(relpath: str, patterns: list[re.Pattern]) -> bool:
    return any(p.search(relpath) for p in patterns)


def aggregate_coverage(data: dict, config: dict, repo_root: Path = REPO_ROOT) -> tuple[dict, list[dict]]:
    """Aggregate line, branch, and function coverage per workspace crate."""
    patterns = [re.compile(r) for r in config.get("ignore_filename_regex", [])]
    crates: dict[str, dict[str, int]] = {}
    file_rows: list[dict] = []

    for entry in data.get("data", []):
        for file_obj in entry.get("files", []):
            rel = normalize_relpath(file_obj.get("filename", ""), repo_root)
            crate = crate_of_relpath(rel)
            if not crate or is_ignored(rel, patterns):
                continue
            summary = file_obj.get("summary", {})
            lines = summary.get("lines", {})
            branches = summary.get("branches", {})
            functions = summary.get("functions", {})

            l_cov = int(lines.get("covered", 0))
            l_tot = int(lines.get("count", 0))
            b_cov = int(branches.get("covered", 0))
            b_tot = int(branches.get("count", 0))
            f_cov = int(functions.get("covered", 0))
            f_tot = int(functions.get("count", 0))

            agg = crates.setdefault(
                crate,
                {"l_cov": 0, "l_tot": 0, "b_cov": 0, "b_tot": 0, "f_cov": 0, "f_tot": 0, "files": 0},
            )
            agg["l_cov"] += l_cov
            agg["l_tot"] += l_tot
            agg["b_cov"] += b_cov
            agg["b_tot"] += b_tot
            agg["f_cov"] += f_cov
            agg["f_tot"] += f_tot
            agg["files"] += 1

            pct = (100.0 * l_cov / l_tot) if l_tot else 100.0
            file_rows.append(
                {
                    "crate": crate,
                    "path": rel,
                    "line_pct": pct,
                    "covered": l_cov,
                    "total": l_tot,
                    "uncovered": l_tot - l_cov,
                }
            )

    return crates, file_rows


def pct_str(cov: int, tot: int) -> str:
    if tot == 0:
        return "   n/a"
    return f"{100.0 * cov / tot:5.1f}%"


def evaluate_thresholds(crates: dict, thresholds: dict[str, float]) -> list[str]:
    failures: list[str] = []
    for crate, minimum in sorted(thresholds.items()):
        stats = crates.get(crate)
        if not stats or stats["l_tot"] == 0:
            failures.append(f"{crate}: no measured lines (threshold {minimum:.1f}%)")
            continue
        actual = 100.0 * stats["l_cov"] / stats["l_tot"]
        if actual < minimum:
            failures.append(f"{crate}: line coverage {actual:.2f}% < threshold {minimum:.1f}%")
    return failures


def format_table(crates: dict, thresholds: dict[str, float]) -> str:
    header = (
        f"{'Crate':<20} {'Files':>5} {'Lines':>8} {'Branches':>9} "
        f"{'Funcs':>8} {'Min':>7} {'Status':<6}"
    )
    sep = "-" * len(header)
    rows = [header, sep]
    tot_l_cov = tot_l_tot = tot_b_cov = tot_b_tot = tot_f_cov = tot_f_tot = tot_files = 0

    for crate in sorted(crates):
        s = crates[crate]
        tot_l_cov += s["l_cov"]
        tot_l_tot += s["l_tot"]
        tot_b_cov += s["b_cov"]
        tot_b_tot += s["b_tot"]
        tot_f_cov += s["f_cov"]
        tot_f_tot += s["f_tot"]
        tot_files += s["files"]

        minimum = thresholds.get(crate)
        if minimum is None:
            min_col = "   -"
            status = "INFO"
        else:
            min_col = f"{minimum:5.1f}%"
            actual = (100.0 * s["l_cov"] / s["l_tot"]) if s["l_tot"] else 0.0
            status = "PASS" if actual >= minimum else "FAIL"

        rows.append(
            f"{crate:<20} {s['files']:>5} {pct_str(s['l_cov'], s['l_tot']):>8} "
            f"{pct_str(s['b_cov'], s['b_tot']):>9} {pct_str(s['f_cov'], s['f_tot']):>8} "
            f"{min_col:>7} {status:<6}"
        )

    rows.append(sep)
    rows.append(
        f"{'TOTAL':<20} {tot_files:>5} {pct_str(tot_l_cov, tot_l_tot):>8} "
        f"{pct_str(tot_b_cov, tot_b_tot):>9} {pct_str(tot_f_cov, tot_f_tot):>8}"
    )
    return "\n".join(rows)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", type=Path, default=DEFAULT_JSON)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--report-only", action="store_true")
    parser.add_argument("--show-gaps", type=int, default=0, help="Show N files with most uncovered lines")
    args = parser.parse_args()

    if not args.json.exists():
        print(f"ERROR: coverage JSON not found at {args.json}", file=sys.stderr)
        return 2

    config = load_config(args.config)
    data = json.loads(args.json.read_text(encoding="utf-8"))
    crates, file_rows = aggregate_coverage(data, config)
    thresholds = config.get("thresholds", {})

    print(format_table(crates, thresholds))

    if args.show_gaps > 0:
        print(f"\nTop {args.show_gaps} files by uncovered lines:")
        by_gap = sorted(file_rows, key=lambda r: (-r["uncovered"], r["line_pct"]))
        for row in by_gap[: args.show_gaps]:
            if row["uncovered"] == 0:
                break
            print(
                f"  {row['uncovered']:4d} uncovered ({row['line_pct']:5.1f}% of {row['total']:3d})  {row['path']}"
            )

    failures = evaluate_thresholds(crates, thresholds)
    if failures:
        print("\nThreshold failures:", file=sys.stderr)
        for msg in failures:
            print(f"  - {msg}", file=sys.stderr)
        return 0 if args.report_only else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
