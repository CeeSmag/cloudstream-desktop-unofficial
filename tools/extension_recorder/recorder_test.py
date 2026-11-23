"""Tests of the extension recorder's modules (run: python3 -m unittest discover -s tools/extension_recorder -p "*_test.py").

Nothing here touches the network or a real sidecar: fetchers, downloads and commands are fakes, and
output goes to temporary folders. The one exception, `RealPolicyContractTest`, runs the app's
`plugin-content-policy` command when cargo is available, to prove the tool and the app agree.
"""

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import api_scan
import catalog
import content_filter
import sidecar_env
from http_fetch import clean_url


def listing(name, repo="https://r.example/repo.json", types=("Movie",), version=1):
    return {"internalName": name, "name": name, "url": f"https://r.example/{name}.cs3", "version": version,
            "tvTypes": list(types) if types is not None else None, "_source_repo_url": repo}


class ContentFilterTest(unittest.TestCase):
    def fake_policy(self, video_names):
        """A stand-in for the policy command: video when the name is in `video_names`."""
        def run(command, input, **_):
            items = json.loads(input)
            answer = [{"repositoryUrl": i["repositoryUrl"], "internalName": i["internalName"],
                       "video": i["internalName"] in video_names, "reason": "fake"} for i in items]
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps(answer), stderr="")
        return run

    def test_the_policy_gets_what_it_reads_of_each_listing(self):
        sent = content_filter.policy_input([listing("Hub"), listing("Mega", types=None)])
        self.assertEqual(sent[0], {"repositoryUrl": "https://r.example/repo.json", "internalName": "Hub",
                                   "tvTypes": ["Movie"]})
        self.assertIsNone(sent[1]["tvTypes"], "null stays null; the policy reads it as none")

    def test_only_video_plugins_are_kept_and_the_rest_recorded_with_why(self):
        with tempfile.TemporaryDirectory() as out:
            kept = content_filter.video_plugins([listing("Hub"), listing("Doom", types=["Others"])],
                                                command=["policy"], run=self.fake_policy({"Hub"}),
                                                out_dir=Path(out))
            self.assertEqual([p["internalName"] for p in kept], ["Hub"])
            excluded = json.loads((Path(out) / "excluded_plugins.json").read_text())
            self.assertEqual(excluded, [{"internalName": "Doom", "repositoryUrl": "https://r.example/repo.json",
                                         "tvTypes": ["Others"], "reason": "fake"}])

    def test_a_failing_or_short_policy_answer_stops_the_run(self):
        def failing(command, input, **_):
            return subprocess.CompletedProcess(command, 2, stdout="", stderr="bad input")
        with self.assertRaisesRegex(RuntimeError, "bad input"):
            content_filter.verdicts([listing("A")], ["policy"], failing)

        def short(command, input, **_):
            return subprocess.CompletedProcess(command, 0, stdout="[]", stderr="")
        with self.assertRaisesRegex(RuntimeError, "0 verdicts for 1"):
            content_filter.verdicts([listing("A")], ["policy"], short)


@unittest.skipUnless(shutil.which("cargo"), "needs cargo to build the app's policy command")
class RealPolicyContractTest(unittest.TestCase):
    def test_the_tool_and_the_app_decide_alike(self):
        games = "https://raw.githubusercontent.com/errorcode26/Ayu-CloudStream-Games/builds/repo.json"
        mega = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json"
        listings = [listing("Hub", types=["Movies"]), listing("Doom", repo=games, types=["Others"]),
                    listing("MegaProvider", repo=mega, types=None)]
        answer = content_filter.verdicts(listings)
        self.assertEqual([v["video"] for v in answer], [True, False, True])
        self.assertIn("not a video plugin", answer[1]["reason"])
        self.assertTrue(answer[2]["reason"].startswith("reviewed:"))


class CatalogTest(unittest.TestCase):
    def fetcher(self, files):
        def fetch(url, **_):
            if url not in files:
                raise RuntimeError(f"404 {url}")
            return json.dumps(files[url]).encode()
        return fetch

    def test_every_repository_is_read_and_listings_are_tagged_and_deduplicated(self):
        files = {
            "https://a/repo.json": {"name": "A", "pluginLists": ["https://a/p1.json", "https://a/broken.json"]},
            "https://a/p1.json": [{"internalName": "Hub", "url": "https://x/Hub.cs3"}],
            "https://b/repo.json": {"name": "B", "pluginLists": ["https://b/p.json"]},
            "https://b/p.json": [{"internalName": "Hub", "url": "https://x/Hub.cs3"},
                                 {"internalName": "Anime", "url": "https://x/Anime.cs3"}],
        }
        repositories = {"https://a/repo.json": {"name": "A"}, "https://b/repo.json": {"name": "B"},
                        "https://gone/repo.json": {"name": "Gone"}}
        with tempfile.TemporaryDirectory() as out, mock.patch.object(catalog, "REPLIES_DIR", Path(out)):
            found = catalog.crawl(repositories, fetch=self.fetcher(files))
            self.assertTrue((Path(out) / "all_plugins_catalog.json").is_file())
        names = sorted(p["internalName"] for p in found)
        self.assertEqual(names, ["Anime", "Hub"], "the same download address counts once; a dead repo is skipped")
        anime = next(p for p in found if p["internalName"] == "Anime")
        self.assertEqual(anime["_source_repo_url"], "https://b/repo.json")
        self.assertEqual(anime["_source_repo_name"], "B")

    def test_the_aggregator_is_the_mega_plugin(self):
        files = {"https://seed/repo.json": {"pluginLists": ["https://seed/p.json"]},
                 "https://seed/p.json": [{"internalName": "Other", "url": "u1"},
                                         {"internalName": "MegaProvider", "url": "u2"}]}
        downloaded = []
        with tempfile.TemporaryDirectory() as cache, mock.patch.object(catalog, "CACHE_DIR", Path(cache)):
            path = catalog.download_aggregator("https://seed/repo.json", fetch=self.fetcher(files),
                                               download=lambda url, dest: downloaded.append((url, dest.name)))
        self.assertEqual(downloaded, [("u2", "MegaProvider.cs3")])
        self.assertEqual(path.name, "MegaProvider.cs3")


class ApiScanTest(unittest.TestCase):
    def test_files_are_named_by_plugin_and_version_safely(self):
        self.assertEqual(api_scan.cache_name(listing("Hub", version=7)), "Hub_7.cs3")
        self.assertEqual(api_scan.cache_name({"internalName": "a/../b c", "version": 2}), "a_.._b_c_2.cs3")

    def test_downloads_are_kept_and_failures_skipped(self):
        def download(url, destination, timeout):
            if "Bad" in url:
                raise RuntimeError("404")
            destination.write_bytes(b"PK")
        with tempfile.TemporaryDirectory() as folder:
            files = api_scan.download_all([listing("Hub"), listing("Bad")], folder=Path(folder), download=download)
        self.assertEqual(list(files), ["Hub_1.cs3"])
        self.assertEqual(api_scan.identities(files),
                         {"Hub_1.cs3": {"internalName": "Hub", "repositoryUrl": "https://r.example/repo.json"}})

    def test_the_scanner_is_called_with_the_batch_and_identities(self):
        command = api_scan.scanner_command("/opt/scanner", Path("/out"), Path("/out/ids.json"), Path("/batch"), 6)
        self.assertEqual(command[1:], ["--out", "/out", "--workers", "6", "--identities", "/out/ids.json", "/batch"])
        self.assertTrue(command[0].endswith(("api-scanner", "api-scanner.bat")))


class SidecarEnvTest(unittest.TestCase):
    def test_a_jdk_is_21_only_when_java_says_so(self):
        with tempfile.TemporaryDirectory() as home:
            java = Path(home) / "bin" / ("java.exe" if sidecar_env.IS_WINDOWS else "java")
            java.parent.mkdir()
            java.write_text("")
            self.assertTrue(sidecar_env.is_jdk21(Path(home), run=lambda *a, **k: 'openjdk version "21.0.9"'))
            self.assertFalse(sidecar_env.is_jdk21(Path(home), run=lambda *a, **k: 'openjdk version "17.0.1"'))
            self.assertFalse(sidecar_env.is_jdk21(Path(home) / "nothing", run=lambda *a, **k: '"21.'))

    def test_an_explicit_jdk_wins_and_none_found_is_an_error(self):
        with mock.patch.dict("os.environ", {"CS_SIDECAR_JAVA_HOME": "", "JAVA_HOME": ""}), \
                mock.patch.object(sidecar_env, "jdk_candidates", return_value=[]), \
                mock.patch.object(sidecar_env, "is_jdk21", side_effect=lambda home, run: str(home) == "/jdk"):
            self.assertEqual(sidecar_env.find_jdk21("/jdk"), "/jdk")
            with self.assertRaises(RuntimeError):
                sidecar_env.find_jdk21("/other")

    def test_distributions_are_found_by_their_start_script(self):
        with tempfile.TemporaryDirectory() as home:
            launcher = Path(sidecar_env.sidecar_launcher(home))
            launcher.parent.mkdir()
            launcher.write_text("")
            with mock.patch.dict("os.environ", {"CS_SIDECAR_HOME": ""}):
                self.assertEqual(sidecar_env.find_sidecar_home(home), home)
            scanner = Path(sidecar_env.scanner_launcher(home))
            scanner.write_text("")
            with mock.patch.dict("os.environ", {"CS_API_SCANNER_HOME": home}):
                self.assertEqual(sidecar_env.find_api_scanner(), home)

    def test_java_options_name_the_data_folder_and_suit_the_os(self):
        options = sidecar_env.java_options(Path("/data/w1"))
        self.assertIn('-Dcloudstream.data.dir="', options)
        self.assertIn("-Dfile.encoding=UTF-8", options)
        self.assertEqual("Windows-ROOT" in options, sidecar_env.IS_WINDOWS)


class SidecarWorkspaceTest(unittest.TestCase):
    def test_plugins_are_staged_where_the_sidecar_jail_allows_them(self):
        import sidecar_process
        with tempfile.TemporaryDirectory() as root, mock.patch.object(sidecar_process.SidecarProcess, "start"):
            workspace = Path(root) / "w1"
            sidecar = sidecar_process.SidecarProcess("/home", "/jdk", workspace)
            self.assertEqual(sidecar.data_dir, workspace / "sidecar", "the data folder")
            plugin = Path(root) / "Hub_1.cs3"
            plugin.write_bytes(b"PK1")
            staged = sidecar.stage(plugin)
            self.assertEqual(staged, (workspace / "extensions" / "Hub_1.cs3").resolve(),
                             "<data>/extensions, next to <data>/sidecar, as in the app")
            self.assertEqual(staged.read_bytes(), b"PK1")
            plugin.write_bytes(b"PK22")
            self.assertEqual(sidecar.stage(plugin).read_bytes(), b"PK22", "a changed file is staged again")


class HttpFetchTest(unittest.TestCase):
    def test_addresses_are_encoded_once(self):
        self.assertEqual(clean_url("https://h/a b/c.cs3"), "https://h/a%20b/c.cs3")
        self.assertEqual(clean_url("https://h/a%20b/c.cs3"), "https://h/a%20b/c.cs3")
        self.assertEqual(clean_url(""), "")


if __name__ == "__main__":
    sys.exit(unittest.main())
