"""Tests for scripts/sidecar_stamp.py (Windows sidecar freshness & JAR contract guard)."""

import tempfile
import unittest
import zipfile
from pathlib import Path

import sidecar_stamp


def _make_repo(root: Path) -> None:
    """Create a minimal repository tree containing the tracked sidecar inputs."""
    (root / "sidecar" / "overlay").mkdir(parents=True)
    (root / "sidecar" / "overlay" / "Solver.kt").write_text("object Solver\n", encoding="utf-8")
    (root / "scripts").mkdir(parents=True)
    (root / "scripts" / "patch_extension_loader.py").write_text("pass\n", encoding="utf-8")
    (root / "scripts" / "setup_sidecar.ps1").write_text("# setup\n", encoding="utf-8")


def _make_install(install_dir: Path, classes: list[str]) -> None:
    """Create a minimal sidecar install directory with lib/plugin-runtime.jar."""
    lib_dir = install_dir / "lib"
    lib_dir.mkdir(parents=True, exist_ok=True)
    (install_dir / "bin").mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(lib_dir / "plugin-runtime.jar", "w") as jar:
        for entry in classes:
            jar.writestr(entry, b"bytecode")


class SidecarStampTests(unittest.TestCase):
    def test_compute_stamp_is_deterministic_and_ignores_build_caches(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _make_repo(repo)
            first = sidecar_stamp.compute_stamp(repo)
            self.assertEqual(len(first), 64)
            self.assertEqual(first, sidecar_stamp.compute_stamp(repo))

            # Build caches and Gradle state must not invalidate the stamp.
            (repo / "sidecar" / "cs-sidecar" / "build").mkdir(parents=True)
            (repo / "sidecar" / "cs-sidecar" / "build" / "out.jar").write_bytes(b"abc")
            (repo / "sidecar" / ".gradle").mkdir(parents=True)
            (repo / "sidecar" / ".gradle" / "cache.bin").write_bytes(b"xyz")
            self.assertEqual(first, sidecar_stamp.compute_stamp(repo))

    def test_compute_stamp_changes_when_sidecar_source_or_script_changes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _make_repo(repo)
            before = sidecar_stamp.compute_stamp(repo)

            (repo / "sidecar" / "overlay" / "Solver.kt").write_text(
                "object Solver { val v = 2 }\n", encoding="utf-8"
            )
            after_kotlin = sidecar_stamp.compute_stamp(repo)
            self.assertNotEqual(before, after_kotlin)

            (repo / "scripts" / "setup_sidecar.ps1").write_text("# v2\n", encoding="utf-8")
            after_script = sidecar_stamp.compute_stamp(repo)
            self.assertNotEqual(after_kotlin, after_script)

    def test_verify_install_rejects_missing_stamp(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, install = root / "repo", root / "install"
            _make_repo(repo)
            _make_install(install, [sidecar_stamp.REQUIRED_CLASS])

            problems = sidecar_stamp.verify_install(repo, install)
            self.assertTrue(any("stamp" in p.lower() for p in problems), problems)

    def test_verify_install_rejects_stale_stamp(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, install = root / "repo", root / "install"
            _make_repo(repo)
            _make_install(install, [sidecar_stamp.REQUIRED_CLASS])
            sidecar_stamp.write_stamp(repo, install)

            (repo / "sidecar" / "overlay" / "Solver.kt").write_text("changed\n", encoding="utf-8")
            problems = sidecar_stamp.verify_install(repo, install)
            self.assertTrue(any("out of date" in p.lower() for p in problems), problems)

    def test_verify_install_rejects_missing_exchange_format_class(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, install = root / "repo", root / "install"
            _make_repo(repo)
            _make_install(install, ["com/lagradost/runtime/Other.class"])
            sidecar_stamp.write_stamp(repo, install)

            problems = sidecar_stamp.verify_install(repo, install)
            self.assertTrue(
                any("CloudflareExchangeFormat" in p for p in problems), problems
            )

    def test_verify_install_rejects_corrupt_or_missing_jar(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, install = root / "repo", root / "install"
            _make_repo(repo)
            install.mkdir(parents=True)
            sidecar_stamp.write_stamp(repo, install)

            missing_problems = sidecar_stamp.verify_install(repo, install)
            self.assertTrue(any("plugin-runtime.jar" in p for p in missing_problems))

            (install / "lib").mkdir()
            (install / "lib" / "plugin-runtime.jar").write_bytes(b"not a zip")
            corrupt_problems = sidecar_stamp.verify_install(repo, install)
            self.assertTrue(any("not a valid zip" in p.lower() for p in corrupt_problems))

    def test_write_then_verify_succeeds_for_fresh_install(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, install = root / "repo", root / "install"
            _make_repo(repo)
            _make_install(install, [sidecar_stamp.REQUIRED_CLASS])
            written = sidecar_stamp.write_stamp(repo, install)

            self.assertEqual(written, sidecar_stamp.compute_stamp(repo))
            self.assertEqual(sidecar_stamp.verify_install(repo, install), [])


if __name__ == "__main__":
    unittest.main()
