"""Fails when a Rust file outside the tests is longer than RULES.md section 4 allows (300 lines).

Clippy's too_many_lines counts lines per function only, so nothing else checks the file limit.

    python scripts/check_file_lengths.py

Files that were already too long when the check came in are listed in
scripts/file_length_baseline.txt ("<lines> <path>"). Such a file may shrink but not grow, and its line
must be removed once it is within the limit (RULES.md R1 work in docs/HARDENING_PLAN.md), so the list
only ever gets shorter. Test files (in a tests/ folder, or named *tests.rs) are not checked.
"""

import subprocess
import sys
from pathlib import Path

LIMIT = 300
REPO_ROOT = Path(__file__).resolve().parent.parent
BASELINE_FILE = REPO_ROOT / "scripts" / "file_length_baseline.txt"


def is_test_file(path):
    """True for integration tests (a tests/ folder) and unit-test files (*tests.rs)."""
    return "/tests/" in f"/{path}" or path.endswith("tests.rs")


def parse_baseline(text):
    """Reads "<lines> <path>" lines; '#' starts a comment. Raises ValueError on any other line."""
    baseline = {}
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        count, _, path = line.partition(" ")
        if not count.isdigit() or not path.strip():
            raise ValueError(f"baseline line is not '<lines> <path>': {line!r}")
        baseline[path.strip()] = int(count)
    return baseline


def find_problems(lengths, baseline, limit):
    """Every problem as one line of text: files over `limit`, baseline files that grew, and baseline
    entries that are no longer needed (the file is within the limit, or gone)."""
    problems = []
    for path, count in sorted(lengths.items()):
        if is_test_file(path) or count <= limit:
            continue
        allowed = baseline.get(path)
        if allowed is None:
            problems.append(f"{path}: {count} lines (limit {limit}); split it by responsibility")
        elif count > allowed:
            problems.append(f"{path}: grew to {count} lines (baseline {allowed}, limit {limit})")
    for path, allowed in sorted(baseline.items()):
        count = lengths.get(path)
        if count is None or count <= limit:
            shown = "is gone" if count is None else f"has {count} lines"
            problems.append(f"{path} {shown}: remove its line from {BASELINE_FILE.name}")
    return problems


def rust_file_lengths():
    """Line count of every tracked .rs file, by repository path."""
    listed = subprocess.run(
        ["git", "ls-files", "*.rs"], cwd=REPO_ROOT, capture_output=True, text=True, check=True
    )
    lengths = {}
    for path in listed.stdout.splitlines():
        with open(REPO_ROOT / path, "rb") as source:
            lengths[path] = source.read().count(b"\n")
    return lengths


def main():
    baseline = parse_baseline(BASELINE_FILE.read_text(encoding="utf-8"))
    problems = find_problems(rust_file_lengths(), baseline, LIMIT)
    for problem in problems:
        print(f"file length: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"file length: every Rust file outside the tests is within {LIMIT} lines "
          f"({len(baseline)} older file(s) listed in {BASELINE_FILE.name}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
