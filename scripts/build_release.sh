#!/usr/bin/env bash
# Builds the CloudStream Linux AppImage for the CPU of this PC:
#   CloudStream-<ver>-x86_64.AppImage   on an x86_64 PC
#   CloudStream-<ver>-aarch64.AppImage  on an ARM64 PC
#
# Usage: scripts/build_release.sh [--install-deps]
#   --install-deps  install the build tools first (Debian/Ubuntu, needs root or sudo)
# Results land in ./dist (or $CS_OUT_DIR).
#
# The Windows files are built by scripts/build_release.ps1. GitHub builds all of them
# (.github/workflows/release.yml), on Debian 12 so the AppImage runs on most distros.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CACHE_DIR="$REPO_ROOT/target/bundle-cache"
STAGE_DIR="$REPO_ROOT/target/bundle-stage"
OUT_DIR="${CS_OUT_DIR:-$REPO_ROOT/dist}"
APP_DIR="$REPO_ROOT/crates/desktop_tauri"
FRONTEND_DIR="$APP_DIR/frontend"

case "$(uname -m)" in
    x86_64)        IMAGE_ARCH=x86_64;  ADOPTIUM_ARCH=x64;     GNU_DIR=x86_64-linux-gnu ;;
    aarch64|arm64) IMAGE_ARCH=aarch64; ADOPTIUM_ARCH=aarch64; GNU_DIR=aarch64-linux-gnu ;;
    *) echo "ERROR: unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac

step() { echo; echo "==> $*"; }
die() { echo "ERROR: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is missing. Run: scripts/build_release.sh --install-deps"; }
version() { python3 -c "import json; print(json.load(open('$APP_DIR/tauri.conf.json'))['version'])"; }

# --- one-time tool installation ---------------------------------------------------------------------
install_deps() {
    command -v apt-get >/dev/null || die "--install-deps only knows apt (Debian/Ubuntu). Install the tools by hand."
    local SUDO=""; [ "$(id -u)" -eq 0 ] || SUDO="sudo"
    step "Installing system packages"
    $SUDO apt-get update
    $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential pkg-config curl ca-certificates git rsync file xz-utils unzip python3 python3-pil \
        libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev \
        libmpv-dev libmpv2
    if ! command -v node >/dev/null || [ "$(node -p 'process.versions.node.split(".")[0]')" -lt 20 ]; then
        step "Installing Node 20"
        curl -fsSL https://deb.nodesource.com/setup_20.x | $SUDO bash -
        $SUDO apt-get install -y --no-install-recommends nodejs
    fi
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
        -h|--help) sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown argument: $arg" ;;
    esac
done

# --- build ----------------------------------------------------------------------------------------
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
need cargo; need python3; need node; need npm; need curl; need git
VERSION="$(version)"
mkdir -p "$CACHE_DIR" "$STAGE_DIR" "$OUT_DIR"
STARTED=$(date +%s)

# The Kotlin extension sidecar (a JVM program).
SIDECAR_DIST="$CACHE_DIR/cs-sidecar-dist"
ensure_sidecar() {
    [ -f "$SIDECAR_DIST/bin/cs-sidecar" ] && return
    step "Building the extension sidecar (first time only)"
    local repo="$CACHE_DIR/sidecar-repo"
    local commit="2d5a86ce2e7466494a9084776dcddd3626f633dd"
    local url="https://github.com/errorcode26/CS3-desktop-client-unofficial.git"
    if [ ! -d "$repo/.git" ]; then git clone --no-checkout "$url" "$repo"; fi
    (
        cd "$repo"
        git fetch --quiet origin "$commit"
        git checkout --quiet "$commit"
        git submodule update --init --depth 1
    )
    cp -rf "$REPO_ROOT/sidecar/overlay/"* "$repo/"
    rm -rf "$repo/cs-sidecar"
    cp -rf "$REPO_ROOT/sidecar/cs-sidecar" "$repo/cs-sidecar"
    grep -q 'include(":cs-sidecar")' "$repo/settings.gradle.kts" || printf '\ninclude(":cs-sidecar")\n' >> "$repo/settings.gradle.kts"
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

step "Building the React / Vite frontend"
(cd "$FRONTEND_DIR" && npm ci --silent && npm run build)
ensure_sidecar

step "Compiling desktop_tauri"
APPDIR="$STAGE_DIR/CloudStream-$IMAGE_ARCH.AppDir"
ICONS="$STAGE_DIR/icons"
mkdir -p "$ICONS"
python3 - "$APP_DIR/icons/icon.png" "$ICONS" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGBA')
for size, name in ((32, '32x32'), (128, '128x128'), (256, '256x256'), (512, 'icon')):
    im.resize((size, size), Image.Resampling.LANCZOS).save(f'{sys.argv[2]}/{name}.png')
PY
CFG=$(printf '{"build":{"devUrl":null,"beforeBuildCommand":""},"bundle":{"icon":["%s/32x32.png","%s/128x128.png","%s/icon.png"]}}' "$ICONS" "$ICONS" "$ICONS")
(cd "$REPO_ROOT" && TAURI_CONFIG="$CFG" cargo build --release -p desktop_tauri --features tauri/custom-protocol)

step "Staging the AppDir"
rm -rf "$APPDIR"; mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib"
cp "$REPO_ROOT/target/release/desktop_tauri" "$APPDIR/usr/bin/desktop_tauri"
strip "$APPDIR/usr/bin/desktop_tauri" 2>/dev/null || true
cp -rf "$SIDECAR_DIST" "$APPDIR/usr/bin/sidecar"
chmod +x "$APPDIR/usr/bin/sidecar/bin/cs-sidecar"
cp -rf "$(get_jre)" "$APPDIR/usr/jre"
ln -s ../jre "$APPDIR/usr/bin/jre"
MPV_DIR=""
for d in "/usr/lib/$GNU_DIR" /usr/lib /usr/lib64; do [ -f "$d/libmpv.so.2" ] && MPV_DIR="$d" && break; done
[ -n "$MPV_DIR" ] || die "libmpv.so.2 not found (install libmpv2)."
cp -a "$MPV_DIR"/libmpv.so* "$APPDIR/usr/lib/"
ln -s ../lib/libmpv.so.2 "$APPDIR/usr/bin/libmpv.so.2"

cp "$ICONS/256x256.png" "$APPDIR/CloudStream.png"
cp "$ICONS/256x256.png" "$APPDIR/.DirIcon"
mkdir -p "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/metainfo"
cp "$ICONS/256x256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/CloudStream.png"
cp "$ICONS/256x256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.cloudstream.desktop.png"
cat > "$APPDIR/CloudStream.desktop" <<EOF
[Desktop Entry]
Name=CloudStream
Comment=CloudStream desktop streaming application
Exec=desktop_tauri
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
if [ -f "$APPDIR/usr/lib/libmpv.so.2" ]; then
    export CS_LIBMPV_PATH="$APPDIR/usr/lib/libmpv.so.2"
fi
exec "$APPDIR/usr/bin/desktop_tauri" "$@"
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
