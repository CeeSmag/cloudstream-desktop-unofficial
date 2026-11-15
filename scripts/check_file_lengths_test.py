"""Tests of scripts/check_file_lengths.py (run: python -m unittest discover -s scripts -p "*_test.py")."""

import unittest

from check_file_lengths import find_problems, is_test_file, parse_baseline

LIMIT = 300


class IsTestFileTest(unittest.TestCase):
    def test_tests_folder_and_tests_suffix_are_test_files(self):
        self.assertTrue(is_test_file("crates/player_mpv/tests/t4_headless_playback.rs"))
        self.assertTrue(is_test_file("crates/desktop_tauri/src/update_verify_tests.rs"))
        self.assertTrue(is_test_file("crates/desktop_tauri/src/race_tests.rs"))

    def test_code_files_are_not_test_files(self):
        self.assertFalse(is_test_file("crates/desktop_tauri/src/update_install.rs"))
        self.assertFalse(is_test_file("crates/desktop_tauri/src/attestation.rs"))


class FindProblemsTest(unittest.TestCase):
    def test_files_within_the_limit_pass(self):
        self.assertEqual(find_problems({"a.rs": 300, "b.rs": 10}, {}, LIMIT), [])

    def test_file_over_the_limit_is_reported(self):
        problems = find_problems({"a.rs": 301}, {}, LIMIT)
        self.assertEqual(len(problems), 1)
        self.assertIn("a.rs", problems[0])
        self.assertIn("301", problems[0])

    def test_test_files_over_the_limit_are_not_reported(self):
        self.assertEqual(find_problems({"crates/x/tests/t4_big.rs": 900}, {}, LIMIT), [])

    def test_baseline_file_may_stay_at_its_recorded_length(self):
        self.assertEqual(find_problems({"a.rs": 340}, {"a.rs": 340}, LIMIT), [])
        self.assertEqual(find_problems({"a.rs": 320}, {"a.rs": 340}, LIMIT), [])

    def test_baseline_file_that_grows_is_reported(self):
        problems = find_problems({"a.rs": 341}, {"a.rs": 340}, LIMIT)
        self.assertEqual(len(problems), 1)
        self.assertIn("grew", problems[0])

    def test_baseline_entry_no_longer_needed_is_reported(self):
        problems = find_problems({"a.rs": 290}, {"a.rs": 340}, LIMIT)
        self.assertEqual(len(problems), 1)
        self.assertIn("remove", problems[0])

    def test_baseline_entry_for_a_deleted_file_is_reported(self):
        problems = find_problems({}, {"gone.rs": 340}, LIMIT)
        self.assertEqual(len(problems), 1)
        self.assertIn("gone.rs", problems[0])


class ParseBaselineTest(unittest.TestCase):
    def test_comments_and_blank_lines_are_skipped(self):
        text = "# why\n\n340 crates/a.rs\n  305   crates/b.rs  \n"
        self.assertEqual(parse_baseline(text), {"crates/a.rs": 340, "crates/b.rs": 305})

    def test_malformed_line_raises(self):
        with self.assertRaises(ValueError):
            parse_baseline("crates/a.rs 340\n")


if __name__ == "__main__":
    unittest.main()
