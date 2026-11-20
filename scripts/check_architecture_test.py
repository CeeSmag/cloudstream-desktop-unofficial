"""Unit tests for scripts/check_architecture.py."""

import unittest

import check_architecture


class CheckArchitectureTests(unittest.TestCase):
    def test_valid_one_way_graph_passes(self):
        graph = {
            "core_model": set(),
            "extension_api": {"core_model"},
            "player_api": set(),
            "player_mpv": {"core_model", "player_api"},
            "storage": {"core_model"},
            "app_services": {"core_model", "extension_api", "player_api", "storage"},
            "ui_slint": {"core_model", "app_services", "player_api"},
            "app_shell": {"ui_slint", "app_services", "player_mpv", "storage"},
        }
        self.assertEqual(check_architecture.check_graph(graph), [])

    def test_detects_ui_depending_on_player_mpv_or_storage(self):
        graph = {
            "core_model": set(),
            "player_mpv": {"core_model"},
            "storage": {"core_model"},
            "ui_slint": {"player_mpv", "storage"},
        }
        problems = check_architecture.check_graph(graph)
        self.assertEqual(len(problems), 1)
        self.assertIn("ui_slint", problems[0])
        self.assertIn("player_mpv", problems[0])
        self.assertIn("storage", problems[0])

    def test_detects_service_depending_on_extension_runtime(self):
        graph = {
            "core_model": set(),
            "extension_runtime": {"core_model"},
            "app_services": {"extension_runtime"},
        }
        problems = check_architecture.check_graph(graph)
        self.assertEqual(len(problems), 1)
        self.assertIn("app_services", problems[0])

    def test_detects_cycle(self):
        graph = {
            "core_model": set(),
            "storage": {"extension_repo"},
            "extension_repo": {"storage"},
        }
        problems = check_architecture.check_graph(graph)
        self.assertTrue(any("cycle" in p for p in problems))


if __name__ == "__main__":
    unittest.main()
