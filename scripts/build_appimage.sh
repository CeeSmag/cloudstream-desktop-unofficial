#!/usr/bin/env bash
# Builds a self-contained CloudStream AppImage for Linux x86_64.
# Packages: Rust core (desktop_tauri), libmpv, Kotlin JVM sidecar, and Adoptium JRE 21.
# Leaves all repository source files completely untouched.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CACHE_DIR="$REPO_ROOT/target/bundle-cache"
STAGE_DIR="$REPO_ROOT/target/bundle-stage"
OUT_DIR="$REPO_ROOT/dist"
APPDIR="$STAGE_DIR/CloudStream.AppDir"
ICONS_DIR="$STAGE_DIR/icons"

APP_DIR="$REPO_ROOT/crates/desktop_tauri"
FRONTEND_DIR="$APP_DIR/frontend"

VERSION=$(python3 -c "import json; print(json.load(open('$APP_DIR/tauri.conf.json'))['version'])")
APP_NAME="CloudStream"

echo "=========================================================="
echo " Building $APP_NAME AppImage v$VERSION (x86_64)"
echo " Repository root: $REPO_ROOT"
echo " Output dir:      $OUT_DIR"
echo "=========================================================="

mkdir -p "$CACHE_DIR" "$STAGE_DIR" "$OUT_DIR"

# 1. Prepare RGBA Icons (Tauri proc macro requires true RGBA PNGs)
echo "[1/7] Preparing true RGBA icons in staging area..."
mkdir -p "$ICONS_DIR"
python3 -c "
from PIL import Image
src = '$APP_DIR/icons/icon.png'
im = Image.open(src).convert('RGBA')
im.resize((32, 32), Image.Resampling.LANCZOS).save('$ICONS_DIR/32x32.png')
im.resize((128, 128), Image.Resampling.LANCZOS).save('$ICONS_DIR/128x128.png')
im.resize((256, 256), Image.Resampling.LANCZOS).save('$ICONS_DIR/256x256.png')
im.resize((512, 512), Image.Resampling.LANCZOS).save('$ICONS_DIR/icon.png')
print('Staged RGBA icons successfully.')
"

# 2. Build Frontend
echo "[2/7] Building React / Vite frontend..."
(
    cd "$FRONTEND_DIR"
    npm ci --silent
    npm run build
)

# 3. Build Rust Native Executable
echo "[3/7] Compiling desktop_tauri release binary..."
TAURI_CONFIG_OVERRIDE=$(cat <<EOF
{
  "build": {
    "devUrl": null,
    "beforeBuildCommand": ""
  },
  "bundle": {
    "icon": [
      "$ICONS_DIR/32x32.png",
      "$ICONS_DIR/128x128.png",
      "$ICONS_DIR/icon.png"
    ]
  }
}
EOF
)

(
    cd "$REPO_ROOT"
    TAURI_CONFIG="$TAURI_CONFIG_OVERRIDE" cargo build --release -p desktop_tauri --features tauri/custom-protocol
)

# 4. Fetch or Prepare Adoptium Temurin 21 JRE for Linux x64
echo "[4/7] Ensuring Temurin 21 JRE for Linux x64 is cached..."
JRE_FOLDER="$CACHE_DIR/jre-21"
if [ ! -d "$JRE_FOLDER/bin" ]; then
    echo "Querying Adoptium API for latest Temurin 21 Hotspot JRE..."
    JRE_URL=$(curl -s "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&image_type=jre&os=linux" | \
        python3 -c "import sys, json; print(json.load(sys.stdin)[0]['binary']['package']['link'])")
    JRE_TAR="$CACHE_DIR/temurin21-jre.tar.gz"
    echo "Downloading Temurin JRE from $JRE_URL..."
    curl -L -f --progress-bar -o "$JRE_TAR.part" "$JRE_URL"
    mv "$JRE_TAR.part" "$JRE_TAR"
    
    mkdir -p "$CACHE_DIR/jre-unpacked"
    tar -xzf "$JRE_TAR" -C "$CACHE_DIR/jre-unpacked"
    INNER_JRE=$(find "$CACHE_DIR/jre-unpacked" -maxdepth 1 -mindepth 1 -type d | head -n 1)
    rm -rf "$JRE_FOLDER"
    mv "$INNER_JRE" "$JRE_FOLDER"
    rm -rf "$CACHE_DIR/jre-unpacked"
fi
echo "JRE 21 ready at $JRE_FOLDER."

# 5. Build or Ensure Kotlin Extension Sidecar is ready
echo "[5/7] Preparing Kotlin Extension Sidecar..."
SIDECAR_DIST="$CACHE_DIR/cs-sidecar-dist"
if [ ! -f "$SIDECAR_DIST/bin/cs-sidecar" ]; then
    SIDECAR_REPO="$CACHE_DIR/sidecar-repo"
    PINNED_COMMIT="2d5a86ce2e7466494a9084776dcddd3626f633dd"
    REPO_URL="https://github.com/errorcode26/CS3-desktop-client-unofficial.git"

    if [ ! -d "$SIDECAR_REPO/.git" ]; then
        echo "Cloning reference desktop client into $SIDECAR_REPO..."
        git clone --no-checkout "$REPO_URL" "$SIDECAR_REPO"
    fi
    (
        cd "$SIDECAR_REPO"
        git fetch --quiet origin "$PINNED_COMMIT"
        git checkout --quiet "$PINNED_COMMIT"
        git submodule update --init --depth 1
    )

    echo "Applying overlay patches..."
    cp -rf "$REPO_ROOT/sidecar/overlay/"* "$SIDECAR_REPO/"

    echo "Installing sidecar module..."
    rm -rf "$SIDECAR_REPO/cs-sidecar"
    cp -rf "$REPO_ROOT/sidecar/cs-sidecar" "$SIDECAR_REPO/cs-sidecar"

    if ! grep -q 'include(":cs-sidecar")' "$SIDECAR_REPO/settings.gradle.kts"; then
        echo -e '\ninclude(":cs-sidecar")' >> "$SIDECAR_REPO/settings.gradle.kts"
    fi

    echo "Compiling cs-sidecar with Gradle..."
    BUILD_JAVA_HOME="${JAVA_HOME:-}"
    if [ -z "$BUILD_JAVA_HOME" ] || [ ! -f "$BUILD_JAVA_HOME/bin/javac" ]; then
        if [ -f "/usr/lib/jvm/java-21-openjdk/bin/javac" ]; then
            BUILD_JAVA_HOME="/usr/lib/jvm/java-21-openjdk"
        elif [ -f "/usr/lib/jvm/default/bin/javac" ]; then
            BUILD_JAVA_HOME="/usr/lib/jvm/default"
        fi
    fi
    echo "Using JDK for Gradle compilation: $BUILD_JAVA_HOME"
    (
        cd "$SIDECAR_REPO"
        chmod +x ./gradlew
        JAVA_HOME="$BUILD_JAVA_HOME" ./gradlew --no-daemon -Dorg.gradle.jvmargs="-Xmx4G -Dfile.encoding=UTF-8" :cs-sidecar:installDist
    )

    rm -rf "$SIDECAR_DIST"
    mkdir -p "$(dirname "$SIDECAR_DIST")"
    cp -rf "$SIDECAR_REPO/cs-sidecar/build/install/cs-sidecar" "$SIDECAR_DIST"
    chmod +x "$SIDECAR_DIST/bin/cs-sidecar"
fi
echo "Sidecar distribution ready at $SIDECAR_DIST."

# 6. Assemble AppDir
echo "[6/7] Staging AppDir..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib"

# Copy binary
cp "$REPO_ROOT/target/release/desktop_tauri" "$APPDIR/usr/bin/desktop_tauri"
strip "$APPDIR/usr/bin/desktop_tauri" 2>/dev/null || true

# Copy sidecar and JRE
cp -rf "$SIDECAR_DIST" "$APPDIR/usr/bin/sidecar"
chmod +x "$APPDIR/usr/bin/sidecar/bin/cs-sidecar"

cp -rf "$JRE_FOLDER" "$APPDIR/usr/jre"
ln -s ../jre "$APPDIR/usr/bin/jre"

# Copy libmpv shared library
if [ -f "/usr/lib/libmpv.so.2" ]; then
    cp -a /usr/lib/libmpv.so* "$APPDIR/usr/lib/" 2>/dev/null || cp "/usr/lib/libmpv.so.2" "$APPDIR/usr/lib/"
elif [ -f "/usr/lib64/libmpv.so.2" ]; then
    cp -a /usr/lib64/libmpv.so* "$APPDIR/usr/lib/" 2>/dev/null || cp "/usr/lib64/libmpv.so.2" "$APPDIR/usr/lib/"
fi
if [ -f "$APPDIR/usr/lib/libmpv.so.2" ]; then
    ln -s ../lib/libmpv.so.2 "$APPDIR/usr/bin/libmpv.so.2"
fi

# Copy Desktop Entry and Icons
cp "$ICONS_DIR/256x256.png" "$APPDIR/CloudStream.png"
cp "$ICONS_DIR/256x256.png" "$APPDIR/.DirIcon"

cat <<'EOF' > "$APPDIR/CloudStream.desktop"
[Desktop Entry]
Name=CloudStream
Comment=CloudStream desktop streaming application
Exec=desktop_tauri
Icon=CloudStream
Terminal=false
Type=Application
Categories=AudioVideo;Video;Player;
StartupWMClass=CloudStream
EOF

# Create AppRun script
cat <<'EOF' > "$APPDIR/AppRun"
#!/usr/bin/env bash
set -e

# Resolve AppDir root
APPDIR="${APPDIR:-$(dirname "$(readlink -f "$0")")}"

# libmpv requires LC_NUMERIC=C to initialize floating point timers/codecs properly
export LC_NUMERIC=C

export PATH="$APPDIR/usr/bin:$APPDIR/usr/jre/bin:$PATH"
export LD_LIBRARY_PATH="$APPDIR/usr/lib:${LD_LIBRARY_PATH:-}"

# Point CloudStream to its bundled components
export CS_SIDECAR_HOME="$APPDIR/usr/bin/sidecar"
export CS_SIDECAR_JAVA_HOME="$APPDIR/usr/jre"

if [ -f "$APPDIR/usr/lib/libmpv.so.2" ]; then
    export CS_LIBMPV_PATH="$APPDIR/usr/lib/libmpv.so.2"
fi

exec "$APPDIR/usr/bin/desktop_tauri" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# 7. Package AppImage with appimagetool
echo "[7/7] Packaging AppImage..."
APPIMAGETOOL="$CACHE_DIR/appimagetool"
if [ ! -f "$APPIMAGETOOL" ]; then
    echo "Downloading appimagetool..."
    curl -L -f --progress-bar -o "$APPIMAGETOOL.part" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    mv "$APPIMAGETOOL.part" "$APPIMAGETOOL"
    chmod +x "$APPIMAGETOOL"
fi

OUTPUT_APPIMAGE="$OUT_DIR/CloudStream-${VERSION}-x86_64.AppImage"
rm -f "$OUTPUT_APPIMAGE"

echo "Running appimagetool..."
export ARCH=x86_64
if ! "$APPIMAGETOOL" "$APPDIR" "$OUTPUT_APPIMAGE"; then
    echo "Standard execution failed (possibly FUSE restriction); trying with --appimage-extract-and-run..."
    "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$OUTPUT_APPIMAGE"
fi

echo "=========================================================="
echo " AppImage build complete!"
echo " Result: $OUTPUT_APPIMAGE"
ls -lh "$OUTPUT_APPIMAGE"
echo "=========================================================="
