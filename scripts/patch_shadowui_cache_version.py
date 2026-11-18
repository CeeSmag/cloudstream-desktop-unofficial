#!/usr/bin/env python3
"""After the sidecar overlay is applied, sync ExtensionLoader cache accept with ShadowUiTransform.VERSION.

Upstream still hard-codes ver == "2"; our transformer writes "4". Without this patch, StreamPlay
keeps an unremapped FlixProxy jar and fails with RequiresPermissionException.
"""
from __future__ import annotations

import pathlib
import sys

OLD = 'ver == "2"'
NEW = "ver == ShadowUiTransform.VERSION"


def patch(repo: pathlib.Path) -> None:
    path = (
        repo
        / "plugin-runtime"
        / "src"
        / "main"
        / "kotlin"
        / "com"
        / "lagradost"
        / "runtime"
        / "loader"
        / "ExtensionLoader.kt"
    )
    if not path.is_file():
        raise SystemExit(f"missing {path}")
    text = path.read_text(encoding="utf-8")
    if NEW in text:
        print(f"already patched: {path}")
        return
    if OLD not in text:
        raise SystemExit(f"expected {OLD!r} in {path}")
    path.write_text(text.replace(OLD, NEW, 1), encoding="utf-8")
    print(f"patched ExtensionLoader cache version check -> {NEW}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} <sidecar-repo-root>")
    patch(pathlib.Path(sys.argv[1]))


if __name__ == "__main__":
    main()
