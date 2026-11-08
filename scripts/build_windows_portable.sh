#!/usr/bin/env bash
# Builds the single-executable Windows portable package on Linux using cargo-xwin.
# Packages: desktop_tauri.exe, Windows libmpv-2.dll, Kotlin JVM sidecar, and Windows Adoptium JRE 21.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CACHE_DIR="$REPO_ROOT/target/bundle-cache"
STAGE_DIR="$REPO_ROOT/target/bundle-stage"
OUT_DIR="$REPO_ROOT/dist"
PAYLOAD_DIR="$STAGE_DIR/single-exe-x64/payload"
APP_DIR="$REPO_ROOT/crates/desktop_tauri"
FRONTEND_DIR="$APP_DIR/frontend"

VERSION=$(python3 -c "import json; print(json.load(open('$APP_DIR/tauri.conf.json'))['version'])")
APP_NAME="CloudStream"
TRIPLE="x86_64-pc-windows-msvc"

echo "=========================================================="
echo " Building $APP_NAME Single-Exe Portable v$VERSION (Windows x64)"
echo " Repository root: $REPO_ROOT"
echo " Output dir:      $OUT_DIR"
echo "=========================================================="

mkdir -p "$CACHE_DIR" "$STAGE_DIR" "$OUT_DIR"

# 1. Build Frontend
echo "[1/6] Building React / Vite frontend..."
(
    cd "$FRONTEND_DIR"
    npm ci --silent
    npm run build
)

# 2. Build desktop_tauri release binary for Windows MSVC
echo "[2/6] Compiling desktop_tauri.exe via cargo-xwin..."
(
    cd "$REPO_ROOT"
    TAURI_CONFIG='{"build":{"devUrl":null,"beforeBuildCommand":""}}' \
        cargo xwin build --release --target "$TRIPLE" -p desktop_tauri --features tauri/custom-protocol
)

# 3. Ensure Windows libmpv-2.dll is cached
echo "[3/6] Ensuring Windows x64 libmpv-2.dll is cached..."
MPV_DLL="$CACHE_DIR/libmpv-x86_64/libmpv-2.dll"
if [ ! -f "$MPV_DLL" ]; then
    python3 -c "
import urllib.request, json, os, subprocess, shutil
cache = '$CACHE_DIR'
print('Querying mpv-winbuild releases...')
req = urllib.request.Request('https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest', headers={'User-Agent': 'curl'})
with urllib.request.urlopen(req) as resp:
    data = json.load(resp)
asset = [a for a in data['assets'] if a['name'].startswith('mpv-dev-x86_64-') and a['name'].endswith('.7z')][0]
archive = os.path.join(cache, asset['name'])
if not os.path.exists(archive):
    print(f'Downloading {asset[\"browser_download_url\"]}...')
    urllib.request.urlretrieve(asset['browser_download_url'], archive)
unpacked = os.path.join(cache, 'mpv-unpacked')
os.makedirs(unpacked, exist_ok=True)
subprocess.run(['7z', 'x', '-y', f'-o{unpacked}', archive], check=True)
for root, dirs, files in os.walk(unpacked):
    if 'libmpv-2.dll' in files:
        os.makedirs(os.path.dirname('$MPV_DLL'), exist_ok=True)
        shutil.copy2(os.path.join(root, 'libmpv-2.dll'), '$MPV_DLL')
        break
shutil.rmtree(unpacked, ignore_errors=True)
print('libmpv-2.dll cached successfully.')
"
fi

# 3b. Vulkan loader. libmpv-2.dll imports vulkan-1.dll; PCs without a GPU driver have none, so
# libmpv fails to load there. Shipping the loader beside it fixes that (no GPU is needed to run it).
# Use VULKAN_LOADER_DLL=/path/to/vulkan-1.dll, or drop the file at $CACHE_DIR/vulkan-1.dll
# (take it from C:\Windows\System32 of a PC with the Vulkan SDK/runtime; Wine's copy will not work).
VULKAN_DLL="${VULKAN_LOADER_DLL:-$CACHE_DIR/vulkan-1.dll}"
if [ ! -f "$VULKAN_DLL" ]; then
    echo "ERROR: vulkan-1.dll not found at $VULKAN_DLL" >&2
    echo "Set VULKAN_LOADER_DLL or copy the real Windows vulkan-1.dll (x64) there." >&2
    exit 1
fi

# 4. Ensure Windows Adoptium Temurin 21 JRE is cached
echo "[4/6] Ensuring Temurin 21 JRE for Windows x64 is cached..."
JRE_DIR="$CACHE_DIR/jre-win-x64"
if [ ! -f "$JRE_DIR/bin/java.exe" ]; then
    python3 -c "
import urllib.request, json, os, subprocess, shutil
cache = '$CACHE_DIR'
api = 'https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&image_type=jre&os=windows'
req = urllib.request.Request(api, headers={'User-Agent': 'curl'})
with urllib.request.urlopen(req) as resp:
    data = json.load(resp)
link = data[0]['binary']['package']['link']
zip_path = os.path.join(cache, os.path.basename(link))
if not os.path.exists(zip_path):
    print(f'Downloading {link}...')
    urllib.request.urlretrieve(link, zip_path)
unpacked = os.path.join(cache, 'jre-win-unpacked')
os.makedirs(unpacked, exist_ok=True)
subprocess.run(['unzip', '-q', '-o', zip_path, '-d', unpacked], check=True)
inner = [os.path.join(unpacked, d) for d in os.listdir(unpacked) if os.path.isdir(os.path.join(unpacked, d))][0]
shutil.rmtree('$JRE_DIR', ignore_errors=True)
shutil.move(inner, '$JRE_DIR')
shutil.rmtree(unpacked, ignore_errors=True)
print('Windows JRE ready.')
"
fi

# 5. Ensure Kotlin JVM Sidecar is ready
echo "[5/6] Ensuring Sidecar distribution is ready..."
SIDECAR_DIST="$CACHE_DIR/cs-sidecar-dist"
if [ ! -f "$SIDECAR_DIST/bin/cs-sidecar.bat" ]; then
    SIDECAR_REPO="$CACHE_DIR/cs-sidecar-repo"
    if [ ! -d "$SIDECAR_REPO/.git" ]; then
        git clone --depth 1 https://github.com/recloudstream/cloudstream.git "$SIDECAR_REPO"
    fi
    chmod +x "$SIDECAR_REPO/gradlew"
    (
        cd "$SIDECAR_REPO"
        ./gradlew --no-daemon -Dorg.gradle.jvmargs="-Xmx4G -Dfile.encoding=UTF-8" :cs-sidecar:installDist
    )
    rm -rf "$SIDECAR_DIST"
    mkdir -p "$(dirname "$SIDECAR_DIST")"
    cp -rf "$SIDECAR_REPO/cs-sidecar/build/install/cs-sidecar" "$SIDECAR_DIST"
fi

# 6. Assemble Payload Archive and Compile Single-Exe Launcher
echo "[6/6] Assembling payload and compiling single-exe launcher..."
rm -rf "$PAYLOAD_DIR"
mkdir -p "$PAYLOAD_DIR"

cp "$REPO_ROOT/target/$TRIPLE/release/desktop_tauri.exe" "$PAYLOAD_DIR/desktop_tauri.exe"
cp "$MPV_DLL" "$PAYLOAD_DIR/libmpv-2.dll"
cp "$VULKAN_DLL" "$PAYLOAD_DIR/vulkan-1.dll"
cp -rf "$SIDECAR_DIST" "$PAYLOAD_DIR/sidecar"
cp -rf "$JRE_DIR" "$PAYLOAD_DIR/jre"

PAYLOAD_ARCHIVE="$STAGE_DIR/payload-x64.tar.gz"
rm -f "$PAYLOAD_ARCHIVE"
tar -czf "$PAYLOAD_ARCHIVE" -C "$PAYLOAD_DIR" .

(
    cd "$REPO_ROOT"
    CS_PAYLOAD_ARCHIVE="$PAYLOAD_ARCHIVE" cargo xwin build --release --target "$TRIPLE" -p portable_launcher
)

OUTPUT_EXE="$OUT_DIR/CloudStream_${VERSION}_x64_portable.exe"
cp -f "$REPO_ROOT/target/$TRIPLE/release/portable_launcher.exe" "$OUTPUT_EXE"

echo "=========================================================="
echo " Windows Single-Exe build complete!"
echo " Result: $OUTPUT_EXE"
ls -lh "$OUTPUT_EXE"
echo "=========================================================="
