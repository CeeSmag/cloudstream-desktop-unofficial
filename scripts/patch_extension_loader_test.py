"""Tests of scripts/patch_extension_loader.py (run: python -m unittest discover -s scripts -p "*_test.py")."""

import pathlib
import tempfile
import unittest

from patch_extension_loader import LOADER, PATCHES, apply, patch

UPSTREAM = """
        if (ver == "2") reuse()
        if (pluginInstance is Plugin) {
            pluginInstance.resources = pluginContext.resources
            pluginInstance.load(pluginContext)
        }
"""


class ApplyTest(unittest.TestCase):
    def test_every_patch_applies_to_upstream_text(self):
        text, report = apply(UPSTREAM)
        for _what, old, new in PATCHES:
            self.assertIn(new, text)
            self.assertNotIn(old, text.replace(new, ""))
        self.assertEqual(len(report), len(PATCHES))
        self.assertTrue(all(line.startswith("patched:") for line in report))

    def test_applying_twice_changes_nothing(self):
        once, _ = apply(UPSTREAM)
        twice, report = apply(once)
        self.assertEqual(once, twice)
        self.assertTrue(all(line.startswith("already patched:") for line in report))

    def test_missing_anchor_stops_the_build(self):
        with self.assertRaises(SystemExit) as raised:
            apply(UPSTREAM.replace("pluginInstance.resources = pluginContext.resources", "x = 1"))
        self.assertIn("plugin assets", str(raised.exception))

    def test_an_anchor_found_twice_stops_the_build(self):
        with self.assertRaises(SystemExit) as raised:
            apply(UPSTREAM + 'if (ver == "2") again()\n')
        self.assertIn("cache version check", str(raised.exception))


class PatchFileTest(unittest.TestCase):
    def test_patches_the_loader_in_a_repo(self):
        with tempfile.TemporaryDirectory() as root:
            path = pathlib.Path(root) / LOADER
            path.parent.mkdir(parents=True)
            path.write_text(UPSTREAM, encoding="utf-8")
            patch(pathlib.Path(root))
            self.assertIn("PluginAssets.resourcesFor", path.read_text(encoding="utf-8"))

    def test_missing_loader_stops_the_build(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(SystemExit):
                patch(pathlib.Path(root))


if __name__ == "__main__":
    unittest.main()
