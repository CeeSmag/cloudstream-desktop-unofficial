"""Unit tests for scripts/check_coverage.py."""

import unittest
from pathlib import Path

import check_coverage


class CheckCoverageTests(unittest.TestCase):
    def sample_data(self):
        return {
            "data": [
                {
                    "files": [
                        {
                            "filename": "/repo/crates/core_model/src/stable_key.rs",
                            "summary": {
                                "lines": {"covered": 9, "count": 10},
                                "branches": {"covered": 2, "count": 4},
                                "functions": {"covered": 1, "count": 1},
                            },
                        },
                        {
                            "filename": "/repo/crates/core_model/src/redact_url_tests.rs",
                            "summary": {
                                "lines": {"covered": 1, "count": 100},
                                "branches": {"covered": 0, "count": 0},
                                "functions": {"covered": 1, "count": 10},
                            },
                        },
                        {
                            "filename": "/repo/crates/player_mpv/src/mpv_api.rs",
                            "summary": {
                                "lines": {"covered": 0, "count": 50},
                                "branches": {"covered": 0, "count": 0},
                                "functions": {"covered": 0, "count": 5},
                            },
                        },
                    ]
                }
            ]
        }

    def test_ignores_tests_and_ffi_exclusions(self):
        cfg = {
            "ignore_filename_regex": [
                r"crates/player_mpv/src/mpv_api\.rs$",
                r"_tests\.rs$",
            ],
            "thresholds": {"core_model": 80.0},
        }
        crates, rows = check_coverage.aggregate_coverage(
            self.sample_data(), cfg, repo_root=Path("/repo")
        )
        self.assertEqual(list(crates.keys()), ["core_model"])
        self.assertEqual(crates["core_model"]["l_cov"], 9)
        self.assertEqual(crates["core_model"]["l_tot"], 10)
        self.assertEqual(len(rows), 1)

    def test_evaluate_thresholds_detects_breach(self):
        crates = {"core_model": {"l_cov": 70, "l_tot": 100}}
        self.assertEqual(check_coverage.evaluate_thresholds(crates, {"core_model": 70.0}), [])
        fails = check_coverage.evaluate_thresholds(crates, {"core_model": 75.0})
        self.assertEqual(len(fails), 1)
        self.assertIn("core_model", fails[0])


if __name__ == "__main__":
    unittest.main()
