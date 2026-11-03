"""Records real replies of the JVM sidecar (CineStream extension) as test fixtures.

The replies are trimmed to a few items and every address in them is replaced by an
`example.test` address, so the fixtures keep the exact shape of the protocol (field names,
types, nulls, nesting) without real hosts or signed links.

Usage (needs the built sidecar, the CineStream extension and network access):
    set CS_SIDECAR_HOME=C:\\cs3d\\repo\\cs-sidecar\\build\\install\\cs-sidecar
    set CS_SIDECAR_JAVA_HOME=<JDK 21 home>
    set CS_TEST_CINESTREAM_CS3=<path to CineStream.cs3>
    python scripts/record_sidecar_replies.py
Writes crates/extension_runtime/tests/fixtures/sidecar_replies/*.json; review the diff before committing.
"""
import json
import os
import re
import subprocess
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates", "extension_runtime", "tests", "fixtures", "sidecar_replies")
EXTENSIONS = (".m3u8", ".mp4", ".mkv", ".srt", ".vtt", ".jpg", ".png", ".webp")
URL = re.compile(r'https?://[^\s"\\]+')
POLLS_KEPT = 5
SERIES_URL = '{"id":"tt18546730","type":"series"}'
counter = [0]


def replace_address(match):
    path = match.group(0).split("?")[0]
    extension = next((e for e in EXTENSIONS if path.lower().endswith(e)), "")
    counter[0] += 1
    return f"https://host{counter[0] % 7}.example.test/file{extension}"


def clean(value):
    if isinstance(value, str):
        return URL.sub(replace_address, value)
    if isinstance(value, list):
        return [clean(item) for item in value]
    if isinstance(value, dict):
        return {key: clean(item) for key, item in value.items()}
    return value


def save(name, value):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, name + ".json"), "w", encoding="utf-8", newline="\n") as handle:
        json.dump(clean(value), handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print("wrote", name)


def start_sidecar():
    env = dict(os.environ)
    env["JAVA_HOME"] = os.environ["CS_SIDECAR_JAVA_HOME"]
    launcher = os.path.join(os.environ["CS_SIDECAR_HOME"], "bin", "cs-sidecar.bat")
    return subprocess.Popen(
        [launcher], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        env=env, text=True, encoding="utf-8",
    )


class Sidecar:
    def __init__(self):
        self.process = start_sidecar()
        self.counter = 0

    def call(self, method, **params):
        self.counter += 1
        request = {"id": self.counter, "method": method, "params": params}
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise SystemExit("the sidecar exited")
            try:
                reply = json.loads(line)
            except ValueError:
                continue
            if reply.get("id") == self.counter:
                return reply.get("result", reply.get("error"))


def record():
    sidecar = Sidecar()
    providers = sidecar.call("load_extension", path=os.environ["CS_TEST_CINESTREAM_CS3"], trusted=False)
    provider, entry = providers[0]["name"], providers[0]["mainPage"][0]
    providers[0]["mainPage"] = providers[0]["mainPage"][:3]
    save("load_extension", providers[:1])

    search = sidecar.call("search", provider=provider, query="Spider-Man", page=1)
    search["items"] = search["items"][:3]
    save("search", search)

    home = sidecar.call("main_page", provider=provider, page=1, name=entry["name"],
                        data=entry["data"], horizontal=entry.get("horizontalImages", False))
    home["items"] = home["items"][:1]
    home["items"][0]["list"] = home["items"][0]["list"][:2]
    save("main_page", home)

    movie_url = search["items"][0]["url"]
    movie = sidecar.call("load", provider=provider, url=movie_url)
    movie["actors"] = (movie.get("actors") or [])[:2]
    save("load_movie", movie)

    series = sidecar.call("load", provider=provider, url=SERIES_URL)
    group_name = next(iter(series["episodes"]))
    episode_data = series["episodes"][group_name][0]["data"]
    series["episodes"] = {group_name: series["episodes"][group_name][:3]}
    series["actors"] = (series.get("actors") or [])[:2]
    series["latestEpisodes"] = {group_name: 3}
    save("load_series", series)

    search_id = sidecar.call("start_link_search", provider=provider, data=episode_data)
    cursor, polls, deadline = {"fromLink": 0, "fromSubtitle": 0}, [], time.time() + 8
    while time.time() < deadline and len(polls) < POLLS_KEPT:
        update = sidecar.call("poll_link_search", id=search_id, waitMs=800, **cursor)
        polls.append(update)
        cursor = {"fromLink": update["nextLink"], "fromSubtitle": update["nextSubtitle"]}
    save("poll_link_search", polls)
    sidecar.call("cancel_link_search", id=search_id)
    sidecar.process.stdin.close()
    sidecar.process.terminate()


if __name__ == "__main__":
    record()
