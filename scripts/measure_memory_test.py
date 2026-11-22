"""Unit tests for scripts/measure_memory.py (pure parsing, tree and summary logic)."""

import unittest

import measure_memory as mm

SMAPS_ROLLUP = """\
55d0c0a00000-7ffd0b5ff000 ---p 00000000 00:00 0                          [rollup]
Rss:               20480 kB
Pss:               12288 kB
Pss_Anon:           8192 kB
Shared_Clean:       4096 kB
"""


def proc(pid, ppid, name, pss_kib):
    return mm.ProcessSample(pid=pid, parent_pid=ppid, name=name, pss_kib=pss_kib)


class ParsePssTests(unittest.TestCase):
    def test_parse_pss_reads_the_total_line_only(self):
        self.assertEqual(mm.parse_pss_kib(SMAPS_ROLLUP), 12288)

    def test_parse_pss_without_a_pss_line_is_zero(self):
        self.assertEqual(mm.parse_pss_kib("Rss: 10 kB\n"), 0)

    def test_parse_pss_ignores_pss_subtotals(self):
        self.assertEqual(mm.parse_pss_kib("Pss_Anon: 99 kB\n"), 0)


class ProcessTreeTests(unittest.TestCase):
    def test_descendants_include_root_and_grandchildren_only(self):
        samples = [
            proc(1, 0, "init", 1),
            proc(10, 1, "cloudstream", 100),
            proc(11, 10, "java", 200),
            proc(12, 11, "java", 50),
            proc(20, 1, "unrelated", 999),
        ]
        tree = mm.descendants_of(samples, 10)
        self.assertEqual(sorted(p.pid for p in tree), [10, 11, 12])

    def test_descendants_of_unknown_root_is_empty(self):
        self.assertEqual(mm.descendants_of([proc(1, 0, "a", 1)], 99), [])

    def test_a_parent_cycle_does_not_hang(self):
        samples = [proc(1, 2, "a", 1), proc(2, 1, "b", 1)]
        self.assertEqual(sorted(p.pid for p in mm.descendants_of(samples, 1)), [1, 2])


class RoleTests(unittest.TestCase):
    def test_roles_by_process_name(self):
        self.assertEqual(mm.role_of("cloudstream"), "app")
        self.assertEqual(mm.role_of("java"), "jvm")
        self.assertEqual(mm.role_of("java.exe"), "jvm")
        self.assertEqual(mm.role_of("cloudstream-verify"), "browser_helper")
        self.assertEqual(mm.role_of("WebKitWebProcess"), "browser_helper")
        self.assertEqual(mm.role_of("msedgewebview2.exe"), "browser_helper")
        self.assertEqual(mm.role_of("mpv"), "other")


class SummaryTests(unittest.TestCase):
    def test_summary_groups_pss_by_role_and_counts_jvms(self):
        tree = [
            proc(10, 1, "cloudstream", 100 * 1024),
            proc(11, 10, "java", 200 * 1024),
            proc(12, 10, "java", 150 * 1024),
            proc(13, 10, "cloudstream-verify", 50 * 1024),
        ]
        summary = mm.summarize(tree)
        self.assertEqual(summary.total_mib, 500.0)
        self.assertEqual(summary.by_role_mib["jvm"], 350.0)
        self.assertEqual(summary.jvm_count, 2)

    def test_peak_keeps_the_largest_total_and_jvm_count(self):
        small = mm.summarize([proc(1, 0, "cloudstream", 10 * 1024)])
        big = mm.summarize(
            [proc(1, 0, "cloudstream", 10 * 1024), proc(2, 1, "java", 90 * 1024)]
        )
        peak = mm.peak_of([small, big, small])
        self.assertEqual(peak.total_mib, 100.0)
        self.assertEqual(peak.jvm_count, 1)


class BudgetTests(unittest.TestCase):
    def test_within_budget_passes(self):
        self.assertEqual(mm.check_budget(449.9, 450.0), [])

    def test_over_budget_reports_a_failure_naming_both_numbers(self):
        failures = mm.check_budget(700.5, 450.0)
        self.assertEqual(len(failures), 1)
        self.assertIn("700.5", failures[0])
        self.assertIn("450.0", failures[0])

    def test_no_budget_never_fails(self):
        self.assertEqual(mm.check_budget(10_000.0, None), [])


if __name__ == "__main__":
    unittest.main()
