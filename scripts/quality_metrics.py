#!/usr/bin/env python3
"""Collect workspace quality metrics (LOC, file counts, test tiers, coverage, mutants, architecture, unsafe)."""

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

import check_architecture
import check_coverage
import check_file_lengths
import check_unsafe_audit

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "target" / "quality" / "metrics.json"

TEST_ATTR_RE = re.compile(r"^\s*#\[(?:tokio::)?test(?:\(.*\))?\]")
PROPTEST_MACRO_RE = re.compile(r"\bproptest!\s*\{")
SCENARIO_RE = re.compile(r"^\s*Scenario(?: Outline)?:", re.MULTILINE)


def count_lines(path: Path) -> int:
    with open(path, "rb") as fh:
        return fh.read().count(b"\n")


def count_tests_in_file(path: Path) -> int:
    text = path.read_text(encoding="utf-8", errors="ignore")
    return sum(1 for line in text.splitlines() if TEST_ATTR_RE.match(line))


def classify_test_file(rel: str, text: str) -> str:
    name = Path(rel).name
    if "race" in name:
        return "t2_race"
    if name.startswith("prop_") or PROPTEST_MACRO_RE.search(text):
        return "property"
    if name.startswith("t4_"):
        return "t4_e2e"
    if name.startswith("t3_"):
        return "t3_parity"
    if "/tests/" in f"/{rel}":
        return "t2_integration"
    return "t1_unit"


def load_mutation_summary(mutants_root: Path) -> dict[str, dict]:
    results: dict[str, dict] = {}
    if not mutants_root.exists():
        return results
    for crate_dir in sorted(mutants_root.iterdir()):
        if not crate_dir.is_dir():
            continue
        outcomes = crate_dir / "mutants.out" / "outcomes.json"
        if not outcomes.exists():
            outcomes = crate_dir / "outcomes.json"
        if not outcomes.exists():
            continue
        data = json.loads(outcomes.read_text(encoding="utf-8"))
        caught = missed = timeout = unviable = 0
        for item in data.get("outcomes", []):
            if item.get("scenario") == "Baseline":
                continue
            s = item.get("summary", "")
            if s == "CaughtMutant":
                caught += 1
            elif s == "MissedMutant":
                missed += 1
            elif s == "Timeout":
                timeout += 1
            elif s == "Unviable":
                unviable += 1
        tested = caught + missed + timeout
        score = round(100.0 * caught / tested, 1) if tested else 100.0
        results[crate_dir.name] = {
            "caught": caught,
            "missed": missed,
            "timeout": timeout,
            "unviable": unviable,
            "score_pct": score,
        }
    return results


def collect_metrics(repo_root: Path = REPO_ROOT) -> dict:
    with open(repo_root / "Cargo.toml", "rb") as fh:
        ws = tomllib.load(fh)

    crate_stats: dict[str, dict] = {}
    tier_counts = {
        "t1_unit": 0,
        "t2_integration": 0,
        "t2_race": 0,
        "t3_parity": 0,
        "t4_e2e": 0,
        "property": 0,
        "bdd_scenarios": 0,
    }

    for member in ws["workspace"]["members"]:
        if not member.startswith("crates/"):
            continue
        crate = member.split("/", 1)[1]
        cdir = repo_root / member
        src_files = 0
        src_loc = 0
        test_files = 0
        test_loc = 0
        tests_in_crate = 0
        max_src_lines = 0

        for rs in sorted(cdir.rglob("*.rs")):
            rel = rs.relative_to(repo_root).as_posix()
            lines = count_lines(rs)
            text = rs.read_text(encoding="utf-8", errors="ignore")
            n_tests = count_tests_in_file(rs)
            tests_in_crate += n_tests
            if check_file_lengths.is_test_file(rel):
                test_files += 1
                test_loc += lines
            else:
                src_files += 1
                src_loc += lines
                max_src_lines = max(max_src_lines, lines)
            if n_tests > 0:
                tier = classify_test_file(rel, text)
                tier_counts[tier] += n_tests

        crate_stats[crate] = {
            "src_files": src_files,
            "src_loc": src_loc,
            "test_files": test_files,
            "test_loc": test_loc,
            "tests": tests_in_crate,
            "max_src_lines": max_src_lines,
        }

    # Count Gherkin BDD scenarios if present.
    bdd_dir = repo_root / "tests" / "bdd" / "features"
    if bdd_dir.exists():
        for feature in sorted(bdd_dir.glob("*.feature")):
            text = feature.read_text(encoding="utf-8", errors="ignore")
            tier_counts["bdd_scenarios"] += len(SCENARIO_RE.findall(text))

    # Coverage if generated.
    cov_json = repo_root / "target" / "coverage" / "summary.json"
    cov_cfg_path = repo_root / "scripts" / "coverage_config.json"
    coverage_by_crate: dict[str, dict] = {}
    if cov_json.exists() and cov_cfg_path.exists():
        cfg = check_coverage.load_config(cov_cfg_path)
        cov_data = json.loads(cov_json.read_text(encoding="utf-8"))
        crates_cov, _ = check_coverage.aggregate_coverage(cov_data, cfg, repo_root=repo_root)
        for crate, s in crates_cov.items():
            coverage_by_crate[crate] = {
                "line_pct": round(100.0 * s["l_cov"] / s["l_tot"], 1) if s["l_tot"] else None,
                "branch_pct": round(100.0 * s["b_cov"] / s["b_tot"], 1) if s["b_tot"] else None,
                "func_pct": round(100.0 * s["f_cov"] / s["f_tot"], 1) if s["f_tot"] else None,
            }

    mutants = load_mutation_summary(repo_root / "target" / "mutants")
    arch_problems = check_architecture.check_graph(check_architecture.load_workspace_graph(repo_root))
    unsafe_problems, unsafe_stats = check_unsafe_audit.audit_workspace(repo_root)

    baseline_path = repo_root / "scripts" / "file_length_baseline.txt"
    baseline = check_file_lengths.parse_baseline(baseline_path.read_text(encoding="utf-8"))
    length_problems = check_file_lengths.find_problems(
        check_file_lengths.rust_file_lengths(), baseline, check_file_lengths.LIMIT
    )

    return {
        "crates": crate_stats,
        "tiers": tier_counts,
        "coverage": coverage_by_crate,
        "mutants": mutants,
        "gates": {
            "architecture_ok": len(arch_problems) == 0,
            "architecture_problems": arch_problems,
            "unsafe_audit_ok": len(unsafe_problems) == 0,
            "unsafe_stats": unsafe_stats,
            "unsafe_problems": unsafe_problems,
            "file_lengths_ok": len(length_problems) == 0,
            "file_length_baseline_count": len(baseline),
        },
    }


def render_markdown(metrics: dict) -> str:
    lines = [
        "# CloudStream RS Quality Metrics",
        "",
        "## Per-Crate Summary",
        "",
        "| Crate | Src Files | Src LOC | Max Lines | Tests | Line Cov | Mut Score |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    total_src_files = total_src_loc = total_tests = 0
    for crate, s in sorted(metrics["crates"].items()):
        total_src_files += s["src_files"]
        total_src_loc += s["src_loc"]
        total_tests += s["tests"]
        cov = metrics["coverage"].get(crate, {}).get("line_pct")
        cov_s = f"{cov:.1f}%" if cov is not None else "-"
        mut = metrics["mutants"].get(crate, {}).get("score_pct")
        mut_s = f"{mut:.1f}%" if mut is not None else "-"
        lines.append(
            f"| `{crate}` | {s['src_files']} | {s['src_loc']} | {s['max_src_lines']} "
            f"| {s['tests']} | {cov_s} | {mut_s} |"
        )
    lines.append(
        f"| **TOTAL** | **{total_src_files}** | **{total_src_loc}** | - | **{total_tests}** | - | - |"
    )

    t = metrics["tiers"]
    lines += [
        "",
        "## Test Counts by Tier",
        "",
        f"- **T1 Unit**: {t['t1_unit']}",
        f"- **T2 Integration**: {t['t2_integration']}",
        f"- **T2 Race**: {t['t2_race']}",
        f"- **T3 Parity**: {t['t3_parity']}",
        f"- **T4 End-to-End**: {t['t4_e2e']}",
        f"- **Property (`proptest`)**: {t['property']}",
        f"- **BDD Scenarios (`cucumber`)**: {t['bdd_scenarios']}",
        "",
        "## Static Gate Status",
        "",
        f"- **Architecture**: {'PASS' if metrics['gates']['architecture_ok'] else 'FAIL'}",
        f"- **Unsafe Allowlist Audit**: {'PASS' if metrics['gates']['unsafe_audit_ok'] else 'FAIL'} "
        f"({metrics['gates']['unsafe_stats']['unsafe_sites']} allowlisted unsafe sites, "
        f"{metrics['gates']['unsafe_stats']['missing_safety']} missing `// SAFETY:`)",
        f"- **File Lengths (<=300 lines)**: {'PASS' if metrics['gates']['file_lengths_ok'] else 'FAIL'} "
        f"({metrics['gates']['file_length_baseline_count']} baseline exemptions)",
    ]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json-out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--markdown-out", type=Path, default=None)
    args = parser.parse_args()

    metrics = collect_metrics()
    args.json_out.parent.mkdir(parents=True, exist_ok=True)
    args.json_out.write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")

    md = render_markdown(metrics)
    if args.markdown_out is not None:
        args.markdown_out.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_out.write_text(md, encoding="utf-8")

    print(md)
    return 0


if __name__ == "__main__":
    sys.exit(main())
