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


def proc(pid, ppid, name, pss_kib, cpu_ticks=0):
    return mm.ProcessSample(
        pid=pid, parent_pid=ppid, name=name, pss_kib=pss_kib, cpu_ticks=cpu_ticks
    )


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


class CpuTests(unittest.TestCase):
    def test_parse_cpu_ticks_adds_user_and_system_time_after_the_name(self):
        stat = "123 (my (odd) name) S 1 123 123 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 1 0 5 1000 100"
        self.assertEqual(mm.parse_cpu_ticks(stat), 300)

    def test_parse_cpu_ticks_of_garbage_is_zero(self):
        self.assertEqual(mm.parse_cpu_ticks("nonsense"), 0)

    def test_tree_cpu_keeps_the_time_of_a_process_that_has_exited(self):
        total = mm.CpuTotal()
        total.add([proc(1, 0, "a", 0, 100), proc(2, 1, "b", 0, 50)])
        total.add([proc(1, 0, "a", 0, 160)])  # process 2 is gone
        self.assertEqual(total.seconds(), (160 + 50) / mm.TICKS_PER_SECOND)

    def test_window_cpu_percent_is_cpu_seconds_over_wall_seconds_of_the_window(self):
        series = [
            mm.TimedSample(t=0.0, total_mib=1.0, cpu_seconds=0.0),
            mm.TimedSample(t=10.0, total_mib=1.0, cpu_seconds=1.0),
            mm.TimedSample(t=20.0, total_mib=1.0, cpu_seconds=4.0),
        ]
        self.assertAlmostEqual(mm.window_cpu_percent(series, 10.0), 30.0)
        self.assertAlmostEqual(mm.window_cpu_percent(series, 20.0), 20.0)

    def test_window_role_stats_separate_each_kind_of_process(self):
        series = [
            mm.TimedSample(0.0, 0, 0, {"app": (100.0, 0.0), "browser_helper": (900.0, 0.0)}),
            mm.TimedSample(10.0, 0, 0, {"app": (120.0, 2.0), "browser_helper": (1100.0, 8.0)}),
        ]
        stats = mm.window_role_stats(series, 10.0)
        self.assertAlmostEqual(stats["app"]["mib_avg"], 110.0)
        self.assertAlmostEqual(stats["app"]["cpu_percent"], 20.0)
        self.assertAlmostEqual(stats["browser_helper"]["cpu_percent"], 80.0)

    def test_window_role_stats_of_a_series_without_roles_is_empty(self):
        self.assertEqual(mm.window_role_stats([mm.TimedSample(0.0, 1.0, 0.0)], 10.0), {})

    def test_window_cpu_percent_of_a_short_series_is_zero(self):
        self.assertEqual(mm.window_cpu_percent([], 10.0), 0.0)


if __name__ == "__main__":
    unittest.main()
