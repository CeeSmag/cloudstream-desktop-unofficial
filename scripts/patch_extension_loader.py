#!/usr/bin/env python3
"""After the sidecar overlay is applied, patch the pinned ExtensionLoader.kt where the overlay cannot.

ExtensionLoader.kt stays the reference file (it is large and changes upstream); these are the only
lines we change, each checked: a patch applies to exactly one place, is already applied, or stops
the build.

1. Cache version: upstream accepts a cached plugin jar when ``ver == "2"``; our transformer writes
   ``ShadowUiTransform.VERSION``. Without this, StreamPlay keeps an unremapped FlixProxy jar and
   fails with RequiresPermissionException, and new sandbox rewrites never reach cached jars.
2. Plugin assets: upstream gives every plugin the context's resources, which have no assets.
   CloudStream gives a ``requiresResources`` plugin Resources over its own archive; without that,
   plugins such as Eclipsia fail on ``Resources.getAssets()`` when they read their bundled files.
"""
from __future__ import annotations

import pathlib
import sys

LOADER = pathlib.Path("plugin-runtime/src/main/kotlin/com/lagradost/runtime/loader/ExtensionLoader.kt")

# (what it is, upstream text, our text)
PATCHES = [
    ("cache version check", 'ver == "2"', "ver == ShadowUiTransform.VERSION"),
    (
        "plugin assets",
        "pluginInstance.resources = pluginContext.resources",
        "pluginInstance.resources = com.lagradost.runtime.assets.PluginAssets.resourcesFor(pluginInstance, pluginContext)",
    ),
]


def apply(text: str) -> tuple[str, list[str]]:
    """Returns the patched text and one line per patch; raises SystemExit when one cannot apply."""
    report = []
    for what, old, new in PATCHES:
        if new in text:
            report.append(f"already patched: {what}")
            continue
        count = text.count(old)
        if count != 1:
            raise SystemExit(f"{what}: expected {old!r} exactly once in ExtensionLoader.kt, found {count}")
        text = text.replace(old, new, 1)
        report.append(f"patched: {what}")
    return text, report


def patch(repo: pathlib.Path) -> None:
    path = repo / LOADER
    if not path.is_file():
        raise SystemExit(f"missing {path}")
    text, report = apply(path.read_text(encoding="utf-8"))
    path.write_text(text, encoding="utf-8")
    for line in report:
        print(f"{line} ({path})")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} <sidecar-repo-root>")
    patch(pathlib.Path(sys.argv[1]))


if __name__ == "__main__":
    main()
