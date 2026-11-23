#!/usr/bin/env bash
# Builds the CloudStream Linux AppImage for the CPU of this PC:
#   CloudStream-<ver>-x86_64.AppImage   on an x86_64 PC
#   CloudStream-<ver>-aarch64.AppImage  on an ARM64 PC
#
# Usage: scripts/build_release.sh [--install-deps]
#   --install-deps  install the build tools first (Debian/Ubuntu, needs root or sudo)
# Results land in ./dist (or $CS_OUT_DIR).
#
# This fork ships the Slint desktop shell (`cloudstream` from app_shell + ui_slint). The Windows files are built by
# scripts/build_release.ps1. GitHub builds all of them (.github/workflows/release.yml), on Debian 12
# so the AppImage runs on most distros.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CACHE_DIR="$REPO_ROOT/target/bundle-cache"
STAGE_DIR="$REPO_ROOT/target/bundle-stage"
OUT_DIR="${CS_OUT_DIR:-$REPO_ROOT/dist}"
ICON_DIR="$REPO_ROOT/crates/app_shell/icons"

case "$(uname -m)" in
    x86_64)        IMAGE_ARCH=x86_64;  ADOPTIUM_ARCH=x64;     GNU_DIR=x86_64-linux-gnu ;;
    aarch64|arm64) IMAGE_ARCH=aarch64; ADOPTIUM_ARCH=aarch64; GNU_DIR=aarch64-linux-gnu ;;
    *) echo "ERROR: unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac

step() { echo; echo "==> $*"; }
die() { echo "ERROR: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is missing. Run: scripts/build_release.sh --install-deps"; }
version() {
    python3 -c "import re, pathlib; t=pathlib.Path('$REPO_ROOT/Cargo.toml').read_text(); m=re.search(r'(?m)^version\s*=\s*\"([^\"]+)\"', t); print(m.group(1) if m else '')"
}

# --- one-time tool installation ---------------------------------------------------------------------
install_deps() {
    command -v apt-get >/dev/null || die "--install-deps only knows apt (Debian/Ubuntu). Install the tools by hand."
    local SUDO=""; [ "$(id -u)" -eq 0 ] || SUDO="sudo"
    step "Installing system packages"
    $SUDO apt-get update
    # Slint: GTK3 + common X11/Wayland bits. WebKitGTK 4.1 for the Cloudflare verification helper.
    $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential pkg-config curl ca-certificates git rsync file xz-utils unzip python3 python3-pil \
        libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev \
        libssl-dev libmpv-dev libmpv2 libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
    if [ ! -x /usr/lib/jvm/temurin-21/bin/javac ]; then
        step "Installing JDK 21 (compiles the Kotlin sidecar)"
        local url
        url=$(curl -fsSL "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=$ADOPTIUM_ARCH&image_type=jdk&os=linux" \
            | python3 -c "import sys, json; print(json.load(sys.stdin)[0]['binary']['package']['link'])")
        $SUDO mkdir -p /usr/lib/jvm/temurin-21
        curl -fsSL "$url" | $SUDO tar -xz -C /usr/lib/jvm/temurin-21 --strip-components=1
    fi
    if ! command -v rustup >/dev/null && ! command -v cargo >/dev/null; then
        step "Installing Rust"
        curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
    fi
    echo; echo "Tools are installed. Run the script again without --install-deps."
}

for arg in "$@"; do
    case "$arg" in
        --install-deps) install_deps; exit 0 ;;
        -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown argument: $arg" ;;
    esac
done

# --- build ----------------------------------------------------------------------------------------
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
need cargo; need python3; need curl; need git
VERSION="$(version)"
[ -n "$VERSION" ] || die "Could not read workspace version from Cargo.toml."
mkdir -p "$CACHE_DIR" "$STAGE_DIR" "$OUT_DIR"
STARTED=$(date +%s)

# The Kotlin extension sidecar (a JVM program).
SIDECAR_DIST="$CACHE_DIR/cs-sidecar-dist"
# Records which sidecar sources the cached build was made from.
SIDECAR_STAMP="$CACHE_DIR/cs-sidecar-dist.stamp"

# A fingerprint of everything the sidecar build reads from this repository: our Gradle modules, the overlay
# and the scripts that prepare the pinned checkout (working tree, so uncommitted edits count; Gradle's own
# caches are left out).
sidecar_sources_stamp() {
    (
        cd "$REPO_ROOT"
        find sidecar scripts/prepare_sidecar_repo.sh scripts/patch_extension_loader.py -type f \
            -not -path '*/.gradle/*' -not -path '*/build/*' -print0 \
            | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
    )
}

ensure_sidecar() {
    local stamp
    stamp="$(sidecar_sources_stamp)"
    if [ -f "$SIDECAR_DIST/bin/cs-sidecar" ] && [ "$(cat "$SIDECAR_STAMP" 2>/dev/null)" = "$stamp" ]; then
        return
    fi
    step "Building the extension sidecar (its sources changed since the cached build)"
    local repo="$CACHE_DIR/sidecar-repo"
    "$REPO_ROOT/scripts/prepare_sidecar_repo.sh" "$repo"
    local jdk="${JAVA_HOME:-}"
    if [ -z "$jdk" ] || [ ! -x "$jdk/bin/javac" ]; then
        for d in /usr/lib/jvm/temurin-21 /usr/lib/jvm/java-21-openjdk-amd64 /usr/lib/jvm/java-21-openjdk-arm64 /usr/lib/jvm/java-21-openjdk; do
            [ -x "$d/bin/javac" ] && jdk="$d" && break
        done
    fi
    [ -x "${jdk:-/nonexistent}/bin/javac" ] || die "JDK 21 not found (set JAVA_HOME)."
    (
        cd "$repo"
        chmod +x ./gradlew
        JAVA_HOME="$jdk" ./gradlew --no-daemon -Dorg.gradle.jvmargs="-Xmx4G -Dfile.encoding=UTF-8" :cs-sidecar:installDist
    )
    rm -rf "$SIDECAR_DIST"
    cp -rf "$repo/cs-sidecar/build/install/cs-sidecar" "$SIDECAR_DIST"
    chmod +x "$SIDECAR_DIST/bin/cs-sidecar"
    printf '%s\n' "$stamp" > "$SIDECAR_STAMP"
}

# Temurin 21 JRE that ships inside the AppImage. Prints the folder.
get_jre() {
    local folder="$CACHE_DIR/jre-linux-$ADOPTIUM_ARCH"
    if [ ! -f "$folder/bin/java" ]; then
        step "Java runtime (Temurin 21, $ADOPTIUM_ARCH)" >&2
        local url file unpacked="$CACHE_DIR/jre-unpacked"
        url=$(curl -fsSL "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=$ADOPTIUM_ARCH&image_type=jre&os=linux" \
            | python3 -c "import sys, json; print(json.load(sys.stdin)[0]['binary']['package']['link'])")
        file="$CACHE_DIR/$(basename "$url")"
        [ -f "$file" ] || { curl -fL --progress-bar -o "$file.part" "$url" >&2; mv "$file.part" "$file"; }
        rm -rf "$unpacked"; mkdir -p "$unpacked"
        tar -xzf "$file" -C "$unpacked"
        rm -rf "$folder"
        mv "$(find "$unpacked" -mindepth 1 -maxdepth 1 -type d | head -n 1)" "$folder"
        rm -rf "$unpacked"
    fi
    echo "$folder"
}

ensure_sidecar

step "Compiling app_shell (cloudstream) and the Cloudflare helper (cloudstream-verify)"
(cd "$REPO_ROOT" && cargo build --release -p app_shell -p cloudflare_window)

APPDIR="$STAGE_DIR/CloudStream-$IMAGE_ARCH.AppDir"
ICONS="$STAGE_DIR/icons"
mkdir -p "$ICONS"
[ -f "$ICON_DIR/icon.png" ] || die "Missing $ICON_DIR/icon.png"
python3 - "$ICON_DIR/icon.png" "$ICONS" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGBA')
for size, name in ((32, '32x32'), (128, '128x128'), (256, '256x256'), (512, 'icon')):
    im.resize((size, size), Image.Resampling.LANCZOS).save(f'{sys.argv[2]}/{name}.png')
PY

step "Staging the AppDir"
rm -rf "$APPDIR"; mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib"
cp "$REPO_ROOT/target/release/cloudstream" "$APPDIR/usr/bin/cloudstream"
strip "$APPDIR/usr/bin/cloudstream" 2>/dev/null || true
# The Cloudflare verification window. It uses the system's WebKitGTK 4.1 (not bundled: WebKit's
# helper processes have fixed install paths); without it, such sites show "could not be verified".
cp "$REPO_ROOT/target/release/cloudstream-verify" "$APPDIR/usr/bin/cloudstream-verify"
strip "$APPDIR/usr/bin/cloudstream-verify" 2>/dev/null || true
cp -rf "$SIDECAR_DIST" "$APPDIR/usr/bin/sidecar"
chmod +x "$APPDIR/usr/bin/sidecar/bin/cs-sidecar"
cp -rf "$(get_jre)" "$APPDIR/usr/jre"
ln -s ../jre "$APPDIR/usr/bin/jre"
# A system monitor names a process after the file it was started as, and a symlink keeps its own name:
# start the plugin JVM through "cloudstream-jvm" so every CloudStream process starts with "cloudstream"
# (the WebKit processes of the verification window keep WebKit's fixed names, under cloudstream-verify).
ln -s java "$APPDIR/usr/jre/bin/cloudstream-jvm"
# The start script must keep the literal text $JAVA_HOME (it expands when the sidecar starts), hence \$.
sed -i "s|\\\$JAVA_HOME/bin/java|\$JAVA_HOME/bin/cloudstream-jvm|" "$APPDIR/usr/bin/sidecar/bin/cs-sidecar"
grep -q 'bin/cloudstream-jvm' "$APPDIR/usr/bin/sidecar/bin/cs-sidecar" || die "the sidecar start script does not name the JVM as expected."
MPV_DIR=""
for d in "/usr/lib/$GNU_DIR" /usr/lib /usr/lib64; do [ -f "$d/libmpv.so.2" ] && MPV_DIR="$d" && break; done
[ -n "$MPV_DIR" ] || die "libmpv.so.2 not found (install libmpv2)."
# The bundled libmpv gets its own folder, NOT on LD_LIBRARY_PATH: it needs the ffmpeg of the system it
# was built on (Debian 12 on GitHub), which other distros may not have. The app tries it first through
# CS_LIBMPV_PATH and, when it cannot load, falls back to the libmpv of the running system, which only
# resolves to the system's when this copy is out of the loader's search path.
mkdir -p "$APPDIR/usr/lib/mpv"
cp -a "$MPV_DIR"/libmpv.so* "$APPDIR/usr/lib/mpv/"

cp "$ICONS/256x256.png" "$APPDIR/CloudStream.png"
cp "$ICONS/256x256.png" "$APPDIR/.DirIcon"
mkdir -p "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/metainfo"
cp "$ICONS/256x256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/CloudStream.png"
cp "$ICONS/256x256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.cloudstream.desktop.png"
cat > "$APPDIR/CloudStream.desktop" <<EOF
[Desktop Entry]
Name=CloudStream
Comment=CloudStream desktop streaming application
Exec=cloudstream
Icon=CloudStream
Terminal=false
Type=Application
Categories=AudioVideo;Video;Player;
StartupWMClass=CloudStream
X-AppImage-Version=${VERSION}
X-AppImage-Name=CloudStream
EOF
cp "$APPDIR/CloudStream.desktop" "$APPDIR/com.cloudstream.desktop.desktop"
cp "$APPDIR/CloudStream.desktop" "$APPDIR/usr/share/applications/CloudStream.desktop"
cp "$APPDIR/CloudStream.desktop" "$APPDIR/usr/share/applications/com.cloudstream.desktop.desktop"
cat > "$APPDIR/usr/share/metainfo/com.cloudstream.desktop.metainfo.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>com.cloudstream.desktop</id>
  <name>CloudStream</name>
  <summary>CloudStream desktop streaming application</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-only</project_license>
  <url type="homepage">https://github.com/CeeSmag/cloudstream-desktop-unofficial</url>
  <description>
    <p>CloudStream is a fast and responsive desktop streaming application with universal extension support.</p>
  </description>
  <developer id="com.cloudstream">
    <name>CloudStream Team</name>
  </developer>
  <launchable type="desktop-id">com.cloudstream.desktop.desktop</launchable>
  <provides>
    <id>CloudStream.desktop</id>
  </provides>
  <content_rating type="oars-1.1" />
  <releases>
    <release version="${VERSION}" date="$(date +%Y-%m-%d)"/>
  </releases>
</component>
EOF
cp "$APPDIR/usr/share/metainfo/com.cloudstream.desktop.metainfo.xml" "$APPDIR/usr/share/metainfo/com.cloudstream.desktop.appdata.xml"
cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
set -e
APPDIR="${APPDIR:-$(dirname "$(readlink -f "$0")")}"
# libmpv requires LC_NUMERIC=C to initialize floating point timers/codecs properly
export LC_NUMERIC=C
export PATH="$APPDIR/usr/bin:$APPDIR/usr/jre/bin:$PATH"
export LD_LIBRARY_PATH="$APPDIR/usr/lib:${LD_LIBRARY_PATH:-}"
export CS_SIDECAR_HOME="$APPDIR/usr/bin/sidecar"
export CS_SIDECAR_JAVA_HOME="$APPDIR/usr/jre"
# The bundled libmpv goes first; the app falls back to the system's when this file is absent.
if [ -f "$APPDIR/usr/lib/mpv/libmpv.so.2" ]; then
    export CS_LIBMPV_PATH="$APPDIR/usr/lib/mpv/libmpv.so.2"
fi
exec "$APPDIR/usr/bin/cloudstream" "$@"
EOF
chmod +x "$APPDIR/AppRun"
chmod -R a+rX "$APPDIR"

step "Packing the AppImage"
TOOL="$CACHE_DIR/appimagetool-$(uname -m)"
if [ ! -f "$TOOL" ]; then
    curl -fL --progress-bar -o "$TOOL.part" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$(uname -m).AppImage"
    mv "$TOOL.part" "$TOOL"; chmod +x "$TOOL"
fi
OUT="$OUT_DIR/CloudStream-${VERSION}-${IMAGE_ARCH}.AppImage"
rm -f "$OUT"
# --appimage-extract-and-run: containers and CI runners have no FUSE.
ARCH="$IMAGE_ARCH" VERSION="$VERSION" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"

step "Done in $(( $(date +%s) - STARTED )) s"
ls -lh "$OUT"
echo "Unsigned: the in-app updater refuses it. To sign: scripts/sign_release.sh <folder> (docs/RELEASING.md)."
