"""Where the JDK 21, the sidecar and the API scanner are, on Linux and on Windows.

Search order, first hit wins: the explicit path, the environment (CS_SIDECAR_JAVA_HOME / JAVA_HOME,
CS_SIDECAR_HOME, CS_API_SCANNER_HOME), then the places this repository's scripts build them:
`target/bundle-cache/...` (scripts/build_release.sh, scripts/prepare_sidecar_repo.sh) and
`C:\\cs3d\\repo\\...` (scripts/setup_sidecar.ps1).
"""

import os
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
BUNDLE_CACHE = REPO_ROOT / "target" / "bundle-cache"
SIDECAR_REPO = BUNDLE_CACHE / "sidecar-repo"
WINDOWS_REPO = Path(r"C:\cs3d\repo")
IS_WINDOWS = sys.platform == "win32"


def _java(home: Path) -> Path:
    return home / "bin" / ("java.exe" if IS_WINDOWS else "java")


def is_jdk21(home: Path, run=subprocess.check_output) -> bool:
    """True when `home` holds a Java 21 (`java -version` says `"21.`)."""
    java = _java(home)
    if not java.is_file():
        return False
    try:
        out = run([str(java), "-version"], stderr=subprocess.STDOUT, text=True)
    except (OSError, subprocess.SubprocessError):
        return False
    return '"21.' in out


def jdk_candidates() -> list[Path]:
    """Usual install folders of a JDK 21 on this OS."""
    if IS_WINDOWS:
        roots = [Path(r"C:\Program Files\Java"), Path(r"C:\Program Files\Eclipse Adoptium"),
                 Path(r"C:\Program Files\Microsoft")]
    else:
        roots = [Path("/usr/lib/jvm"), BUNDLE_CACHE]
    found = []
    for root in roots:
        if root.is_dir():
            found += sorted(p for p in root.iterdir() if p.is_dir() and "21" in p.name)
    return found


def find_jdk21(explicit: str = "", run=subprocess.check_output) -> str:
    """The JDK 21 home to run the sidecar and the scanner with."""
    env = [os.environ.get("CS_SIDECAR_JAVA_HOME", ""), os.environ.get("JAVA_HOME", "")]
    for candidate in [explicit, *env, *map(str, jdk_candidates())]:
        if candidate and is_jdk21(Path(candidate), run):
            return candidate
    raise RuntimeError("JDK 21 not found. Pass --jdk21 <home> or set CS_SIDECAR_JAVA_HOME.")


def sidecar_launcher(home: str) -> str:
    """The start script inside a cs-sidecar distribution."""
    return str(Path(home) / "bin" / ("cs-sidecar.bat" if IS_WINDOWS else "cs-sidecar"))


def find_sidecar_home(explicit: str = "") -> str:
    """A built cs-sidecar distribution (`bin/cs-sidecar[.bat]`)."""
    candidates = [
        explicit,
        os.environ.get("CS_SIDECAR_HOME", ""),
        str(BUNDLE_CACHE / "cs-sidecar-dist"),
        str(SIDECAR_REPO / "cs-sidecar" / "build" / "install" / "cs-sidecar"),
        str(WINDOWS_REPO / "cs-sidecar" / "build" / "install" / "cs-sidecar"),
    ]
    for home in candidates:
        if home and Path(sidecar_launcher(home)).is_file():
            return home
    raise RuntimeError("cs-sidecar not found. Build it (scripts/build_release.sh or scripts/setup_sidecar.ps1) "
                       "or pass --sidecar-home.")


def scanner_launcher(home: str) -> str:
    """The start script inside an api-scanner distribution."""
    return str(Path(home) / "bin" / ("api-scanner.bat" if IS_WINDOWS else "api-scanner"))


def find_api_scanner(explicit: str = "") -> str:
    """A built api-scanner distribution. Build it with `--build-scanner` or by hand:
    scripts/prepare_sidecar_repo.sh target/bundle-cache/sidecar-repo, then
    `./gradlew :api-scanner:installDist` inside it."""
    candidates = [
        explicit,
        os.environ.get("CS_API_SCANNER_HOME", ""),
        str(SIDECAR_REPO / "api-scanner" / "build" / "install" / "api-scanner"),
        str(WINDOWS_REPO / "api-scanner" / "build" / "install" / "api-scanner"),
    ]
    for home in candidates:
        if home and Path(scanner_launcher(home)).is_file():
            return home
    raise RuntimeError("api-scanner not found. Run with --build-scanner or pass --scanner-home.")


def build_api_scanner(jdk21: str, run=subprocess.run) -> str:
    """Prepares the sidecar checkout and builds the scanner there (Linux; Windows: setup_sidecar.ps1)."""
    if IS_WINDOWS:
        raise RuntimeError("On Windows run scripts\\setup_sidecar.ps1, then "
                           "gradlew :api-scanner:installDist in C:\\cs3d\\repo.")
    run([str(REPO_ROOT / "scripts" / "prepare_sidecar_repo.sh"), str(SIDECAR_REPO)], check=True)
    env = {**os.environ, "JAVA_HOME": jdk21}
    run(["./gradlew", "--no-daemon", "-q", ":api-scanner:installDist"], cwd=SIDECAR_REPO, env=env, check=True)
    return find_api_scanner()


def java_options(data_dir: Path) -> str:
    """JAVA_OPTS for a sidecar: its data folder, UTF-8 I/O, and the Windows trust store on Windows."""
    options = (f'-Dcloudstream.data.dir="{data_dir}" '
               "-Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8 -Dfile.encoding=UTF-8")
    if IS_WINDOWS:
        options += " -Djavax.net.ssl.trustStoreType=Windows-ROOT"
    else:
        options += " -Djava.awt.headless=true"
    return options
