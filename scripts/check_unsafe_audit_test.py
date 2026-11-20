"""Unit tests for scripts/check_unsafe_audit.py."""

import unittest

import check_unsafe_audit


class CheckUnsafeAuditTests(unittest.TestCase):
    def test_flags_disallowed_unsafe_in_non_allowlisted_file(self):
        src = "fn foo() {\n    unsafe { bar(); }\n}\n"
        findings = check_unsafe_audit.find_unsafe_occurrences(src, check_safety=False)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0][0], 2)

    def test_ignores_unsafe_inside_comments_and_strings(self):
        src = 'fn foo() {\n    // unsafe { nope }\n    let s = "unsafe { nope }";\n}\n'
        self.assertEqual(check_unsafe_audit.find_unsafe_occurrences(src, check_safety=False), [])

    def test_requires_safety_comment_when_checking_allowlisted_file(self):
        missing = "fn foo() {\n    unsafe { bar(); }\n}\n"
        present = "fn foo() {\n    // SAFETY: pointer is valid.\n    unsafe { bar(); }\n}\n"
        self.assertEqual(len(check_unsafe_audit.find_unsafe_occurrences(missing, check_safety=True)), 1)
        self.assertEqual(check_unsafe_audit.find_unsafe_occurrences(present, check_safety=True), [])

    def test_stops_before_cfg_test_module(self):
        src = "fn safe() {}\n#[cfg(test)]\nmod tests {\n    fn t() { unsafe { x(); } }\n}\n"
        self.assertEqual(check_unsafe_audit.find_unsafe_occurrences(src, check_safety=False), [])


if __name__ == "__main__":
    unittest.main()
