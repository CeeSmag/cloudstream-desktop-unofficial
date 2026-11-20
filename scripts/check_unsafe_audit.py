#!/usr/bin/env python3
"""Audit RULES.md §8 unsafe_code allowlist and `// SAFETY:` comments across the workspace."""

import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# RULES.md §8 allowlist: localized unsafe only in player_mpv (libmpv FFI),
# extension_runtime (OS sandbox spawn), and ui_slint's generated Slint bindings.
UNSAFE_ALLOWLIST_FILES: dict[str, set[str]] = {
    "player_mpv": {
        "crates/player_mpv/src/mpv_api.rs",
        "crates/player_mpv/src/mpv_api/properties.rs",
        "crates/player_mpv/src/mpv_log.rs",
        "crates/player_mpv/src/mpv_render_api.rs",
    },
    "extension_runtime": {
        "crates/extension_runtime/src/sidecar_app_container_attrs.rs",
        "crates/extension_runtime/src/sidecar_app_container_ffi.rs",
        "crates/extension_runtime/src/sidecar_app_container_launch.rs",
        "crates/extension_runtime/src/sidecar_app_container_profile.rs",
        "crates/extension_runtime/src/sidecar_job_object.rs",
        "crates/extension_runtime/src/sidecar_linux_landlock.rs",
        "crates/extension_runtime/src/sidecar_linux_limits.rs",
        "crates/extension_runtime/src/sidecar_spawn_thread.rs",
    },
    "ui_slint": {
        "crates/ui_slint/src/lib.rs",
    },
}

UNSAFE_ANY_RE = re.compile(r"\bunsafe\s*(\{|fn\b|impl\b|extern\b|trait\b)")
# RULES.md §8: each hand-written `unsafe` block (and `unsafe impl`) needs a `// SAFETY:` comment.
# Function-pointer type aliases (`type F = unsafe extern "C" fn(...)`) are signatures, not blocks.
UNSAFE_BLOCK_RE = re.compile(r"\bunsafe\s*(\{|impl\b)")
FORBID_UNSAFE_RE = re.compile(r"#!\[forbid\(unsafe_code\)\]")


def strip_test_module(lines: list[str]) -> list[tuple[int, str]]:
    """Return (1-based line_no, text) for non-test lines (stops at top-level #[cfg(test)] mod)."""
    out: list[tuple[int, str]] = []
    for idx, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped == "#[cfg(test)]":
            next_line = lines[idx].strip() if idx < len(lines) else ""
            if next_line.startswith("mod ") or next_line.startswith("#[path"):
                break
        out.append((idx, line))
    return out


def has_safety_comment(lines: list[str], line_idx_0: int) -> bool:
    """True if a `// SAFETY:` comment appears within the 6 lines preceding or on `line_idx_0`."""
    start = max(0, line_idx_0 - 6)
    window = "\n".join(lines[start : line_idx_0 + 1])
    return "SAFETY:" in window


def find_unsafe_occurrences(text: str, check_safety: bool) -> list[tuple[int, str]]:
    """Find non-comment unsafe constructs and optionally flag missing `// SAFETY:` comments."""
    lines = text.splitlines()
    code_lines = strip_test_module(lines)
    findings: list[tuple[int, str]] = []
    for line_no, raw in code_lines:
        stripped = raw.strip()
        if stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*"):
            continue
        # Remove line-end comments and string literals before matching.
        code_part = re.sub(r'"(?:\\.|[^"\\])*"', '""', raw)
        code_part = code_part.split("//", 1)[0]
        regex = UNSAFE_BLOCK_RE if check_safety else UNSAFE_ANY_RE
        if regex.search(code_part):
            if check_safety:
                if not has_safety_comment(lines, line_no - 1):
                    findings.append((line_no, "unsafe block/impl without nearby `// SAFETY:` comment"))
            else:
                findings.append((line_no, "disallowed unsafe outside RULES.md §8 allowlist"))
    return findings


def audit_workspace(repo_root: Path = REPO_ROOT) -> tuple[list[str], dict[str, int]]:
    with open(repo_root / "Cargo.toml", "rb") as fh:
        ws = tomllib.load(fh)

    problems: list[str] = []
    stats = {"crates_checked": 0, "allowlisted_files": 0, "unsafe_sites": 0, "missing_safety": 0}

    for member in ws["workspace"]["members"]:
        if not member.startswith("crates/"):
            continue
        crate = member.split("/", 1)[1]
        stats["crates_checked"] += 1
        crate_dir = repo_root / member
        allowed_files = UNSAFE_ALLOWLIST_FILES.get(crate, set())

        if crate not in UNSAFE_ALLOWLIST_FILES:
            roots = [p for p in (crate_dir / "src/lib.rs", crate_dir / "src/main.rs") if p.exists()]
            for root in roots:
                content = root.read_text(encoding="utf-8", errors="ignore")
                if not FORBID_UNSAFE_RE.search(content):
                    rel = root.relative_to(repo_root).as_posix()
                    problems.append(f"{rel}: missing #![forbid(unsafe_code)] (RULES.md §8)")

        for rs_file in sorted((crate_dir / "src").rglob("*.rs")):
            rel = rs_file.relative_to(repo_root).as_posix()
            if rel.endswith("_tests.rs"):
                continue
            text = rs_file.read_text(encoding="utf-8", errors="ignore")
            is_allowed = rel in allowed_files
            if is_allowed:
                stats["allowlisted_files"] += 1
                # Count all unsafe sites and check SAFETY comments (except Slint generated include).
                if crate == "ui_slint":
                    continue
                all_sites = find_unsafe_occurrences(text, check_safety=False)
                stats["unsafe_sites"] += len(all_sites)
                missing = find_unsafe_occurrences(text, check_safety=True)
                stats["missing_safety"] += len(missing)
                for line_no, reason in missing:
                    problems.append(f"{rel}:{line_no}: {reason}")
            else:
                for line_no, reason in find_unsafe_occurrences(text, check_safety=False):
                    problems.append(f"{rel}:{line_no}: {reason}")

    return problems, stats


def main() -> int:
    problems, stats = audit_workspace()
    if problems:
        for problem in problems:
            print(f"unsafe-audit: {problem}", file=sys.stderr)
        print(
            f"unsafe-audit: FAILED ({len(problems)} issue(s); "
            f"{stats['unsafe_sites']} allowlisted unsafe site(s), "
            f"{stats['missing_safety']} missing // SAFETY: comment(s)).",
            file=sys.stderr,
        )
        return 1
    print(
        f"unsafe-audit: OK ({stats['crates_checked']} crates checked; "
        f"{stats['unsafe_sites']} allowlisted unsafe site(s) in {stats['allowlisted_files']} files, "
        "all documented with // SAFETY:)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
