#!/usr/bin/env python3
"""Fingerprint the repository's sidecar sources and verify a built Windows sidecar install.

Matches the freshness guarantee `scripts/build_release.sh` provides on Linux so
`scripts/build_release.ps1` cannot package an outdated sidecar (e.g., one lacking
`CloudflareExchangeFormat`, which would wrongly mark sites browser-only on refused
saved passes).

Usage:
    python scripts/sidecar_stamp.py --write <install_dir>
    python scripts/sidecar_stamp.py --verify <install_dir>
"""

import argparse
import hashlib
import sys
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
STAMP_FILE_NAME = ".sidecar-sources.stamp"
IGNORED_DIRS = frozenset({".gradle", "build", "__pycache__"})
INPUT_PATHS = (
    Path("sidecar"),
    Path("scripts/patch_extension_loader.py"),
    Path("scripts/setup_sidecar.ps1"),
)
REQUIRED_JAR = Path("lib/plugin-runtime.jar")
REQUIRED_CLASS = "com/lagradost/runtime/loader/stubs/CloudflareExchangeFormat.class"


def _iter_input_files(repo_root: Path) -> list[Path]:
    """Return every tracked sidecar input file in deterministic POSIX path order."""
    files: list[Path] = []
    for rel in INPUT_PATHS:
        target = repo_root / rel
        if target.is_file():
            files.append(target)
        elif target.is_dir():
            for candidate in target.rglob("*"):
                if not candidate.is_file():
                    continue
                parts = candidate.relative_to(repo_root).parts
                if any(part in IGNORED_DIRS for part in parts):
                    continue
                files.append(candidate)
    files.sort(key=lambda p: p.relative_to(repo_root).as_posix())
    return files


def compute_stamp(repo_root: Path = REPO_ROOT) -> str:
    """Compute a deterministic SHA-256 fingerprint of all sidecar build inputs."""
    digest = hashlib.sha256()
    for path in _iter_input_files(repo_root):
        rel_posix = path.relative_to(repo_root).as_posix().encode("utf-8")
        digest.update(rel_posix)
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def write_stamp(repo_root: Path, install_dir: Path) -> str:
    """Write the current source fingerprint into `install_dir/.sidecar-sources.stamp`."""
    stamp = compute_stamp(repo_root)
    stamp_path = install_dir / STAMP_FILE_NAME
    stamp_path.write_text(f"{stamp}\n", encoding="utf-8")
    return stamp


def _verify_jar_contract(install_dir: Path) -> list[str]:
    jar_path = install_dir / REQUIRED_JAR
    if not jar_path.is_file():
        return [f"missing {REQUIRED_JAR.as_posix()} under {install_dir}"]
    try:
        with zipfile.ZipFile(jar_path, "r") as archive:
            names = set(archive.namelist())
    except zipfile.BadZipFile:
        return [f"{jar_path} is not a valid zip/JAR archive"]
    if REQUIRED_CLASS not in names:
        return [
            f"{jar_path} does not contain {REQUIRED_CLASS}; "
            "sidecar predates saved Cloudflare passes"
        ]
    return []


def verify_install(repo_root: Path, install_dir: Path) -> list[str]:
    """Return a list of problems if `install_dir` is stale or missing required classes."""
    problems: list[str] = []
    expected = compute_stamp(repo_root)
    stamp_path = install_dir / STAMP_FILE_NAME
    if not stamp_path.is_file():
        problems.append(
            f"missing {STAMP_FILE_NAME} in {install_dir}; sidecar freshness cannot be confirmed"
        )
    else:
        recorded = stamp_path.read_text(encoding="utf-8").strip()
        if recorded != expected:
            problems.append(
                f"sidecar at {install_dir} is out of date with repository sources "
                f"(recorded {recorded[:12]}, expected {expected[:12]})"
            )
    problems.extend(_verify_jar_contract(install_dir))
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", type=Path, metavar="INSTALL_DIR")
    group.add_argument("--verify", type=Path, metavar="INSTALL_DIR")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    args = parser.parse_args(argv)

    if args.write is not None:
        stamp = write_stamp(args.repo_root, args.write)
        print(f"sidecar stamp written: {stamp[:12]} ({args.write / STAMP_FILE_NAME})")
        return 0

    problems = verify_install(args.repo_root, args.verify)
    if problems:
        for problem in problems:
            print(f"sidecar-stamp: {problem}", file=sys.stderr)
        print(
            "sidecar-stamp: rebuild the sidecar first with: powershell -File scripts\\setup_sidecar.ps1",
            file=sys.stderr,
        )
        return 1
    print(f"sidecar-stamp: OK ({args.verify})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
