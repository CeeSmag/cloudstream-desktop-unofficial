"""Unit tests for scripts/quality_metrics.py."""

import unittest

import quality_metrics


class QualityMetricsTests(unittest.TestCase):
    def test_classify_test_file_tiers(self):
        self.assertEqual(
            quality_metrics.classify_test_file("crates/title_cache/src/title_cache_race_tests.rs", ""),
            "t2_race",
        )
        self.assertEqual(
            quality_metrics.classify_test_file("crates/core_model/tests/prop_url.rs", ""),
            "property",
        )
        self.assertEqual(
            quality_metrics.classify_test_file("crates/extension_runtime/tests/t3_parity.rs", ""),
            "t3_parity",
        )
        self.assertEqual(
            quality_metrics.classify_test_file("crates/player_mpv/tests/t4_headless.rs", ""),
            "t4_e2e",
        )
        self.assertEqual(
            quality_metrics.classify_test_file("crates/subtitle_store/tests/t2_http.rs", ""),
            "t2_integration",
        )
        self.assertEqual(
            quality_metrics.classify_test_file("crates/core_model/src/redact_url_tests.rs", ""),
            "t1_unit",
        )

    def test_render_markdown_includes_summary_sections(self):
        sample = {
            "crates": {
                "core_model": {
                    "src_files": 2,
                    "src_loc": 100,
                    "test_files": 1,
                    "test_loc": 50,
                    "tests": 5,
                    "max_src_lines": 60,
                }
            },
            "tiers": {
                "t1_unit": 5,
                "t2_integration": 0,
                "t2_race": 0,
                "t3_parity": 0,
                "t4_e2e": 0,
                "property": 0,
                "bdd_scenarios": 3,
            },
            "coverage": {"core_model": {"line_pct": 92.5}},
            "mutants": {"core_model": {"score_pct": 88.0}},
            "gates": {
                "architecture_ok": True,
                "architecture_problems": [],
                "unsafe_audit_ok": True,
                "unsafe_stats": {"unsafe_sites": 0, "missing_safety": 0},
                "unsafe_problems": [],
                "file_lengths_ok": True,
                "file_length_baseline_count": 0,
            },
        }
        md = quality_metrics.render_markdown(sample)
        self.assertIn("| `core_model` |", md)
        self.assertIn("92.5%", md)
        self.assertIn("88.0%", md)
        self.assertIn("BDD Scenarios", md)


if __name__ == "__main__":
    unittest.main()
