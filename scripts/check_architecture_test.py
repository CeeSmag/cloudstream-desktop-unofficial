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


def package(name, features=None, dependencies=None):
    return {"name": name, "features": features or {}, "dependencies": dependencies or []}


def dependency(name, uses_default_features=False, features=None, kind=None):
    return {
        "name": name,
        "uses_default_features": uses_default_features,
        "features": features or [],
        "kind": kind,
    }


class ForwardedDefaultsTests(unittest.TestCase):
    UI = package("ui_slint", {"default": ["wayland", "pointer-hiding"], "wayland": [], "pointer-hiding": []})

    def test_a_default_that_is_not_passed_on_is_reported(self):
        shell = package(
            "app_shell",
            {"default": ["wayland"], "wayland": ["ui_slint/wayland"]},
            [dependency("ui_slint")],
        )
        problems = check_architecture.check_forwarded_defaults([self.UI, shell])
        self.assertEqual(len(problems), 1)
        self.assertIn("pointer-hiding", problems[0])
        self.assertIn("app_shell", problems[0])

    def test_defaults_passed_on_through_own_features_or_the_dependency_pass(self):
        through_features = package(
            "app_shell",
            {
                "default": ["wayland", "pointer-hiding"],
                "wayland": ["ui_slint/wayland"],
                "pointer-hiding": ["ui_slint?/pointer-hiding"],
            },
            [dependency("ui_slint")],
        )
        on_the_dependency = package(
            "other", {}, [dependency("ui_slint", features=["wayland", "pointer-hiding"])]
        )
        self.assertEqual(
            check_architecture.check_forwarded_defaults([self.UI, through_features, on_the_dependency]),
            [],
        )

    def test_dependencies_with_defaults_dev_dependencies_and_outside_crates_are_not_checked(self):
        user = package(
            "app_shell",
            {},
            [
                dependency("ui_slint", uses_default_features=True),
                dependency("ui_slint", kind="dev"),
                dependency("slint"),
            ],
        )
        self.assertEqual(check_architecture.check_forwarded_defaults([self.UI, user]), [])


if __name__ == "__main__":
    unittest.main()
