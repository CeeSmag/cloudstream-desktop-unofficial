# Builds the Windows (x64) release files:
#
#   CloudStream_<ver>_x64-setup.exe       installer
#   CloudStream_<ver>_x64_portable.exe    single-file portable
#
#   .\scripts\build_release.ps1                    # both
#   .\scripts\build_release.ps1 -Kind portable     # only one: installer or portable
#
# The Linux AppImage is built by scripts/build_release.sh. GitHub builds all of them
# (.github/workflows/release.yml), or run each script on its own OS.
#
# Needs: the Rust MSVC toolchain, Node 20+, and the extension sidecar built once with
# scripts\setup_sidecar.ps1. Every package carries what the app needs to run on a clean PC:
# libmpv, vulkan-1.dll, the sidecar and a Java runtime (downloaded once into target\bundle-cache).
# Results land in .\dist.
param(
    [ValidateSet('all', 'installer', 'portable')][string]$Kind = 'all',
    [string]$SidecarHome = 'C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar',
    # Where to take libmpv-2.dll from; when absent it is downloaded (mpv-winbuild, latest).
    [string]$LibMpv = 'C:\cs3d\mpv\libmpv-2.dll',
    # vulkan-1.dll (the Vulkan loader) that libmpv imports; PCs without a GPU driver lack it and libmpv
    # then fails to load. Default: scripts\vendor\vulkan\x64\vulkan-1.dll (kept in the repo).
    [string]$VulkanDll = '',
    [string]$OutDir = ''
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$repoRoot = Split-Path -Parent $PSScriptRoot
$appDir = Join-Path $repoRoot 'crates\desktop_tauri'
$frontendDir = Join-Path $appDir 'frontend'
$cache = Join-Path $repoRoot 'target\bundle-cache'
$stageRoot = Join-Path $repoRoot 'target\bundle-stage'
if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'dist' }
$version = (Get-Content (Join-Path $appDir 'tauri.conf.json') -Raw | ConvertFrom-Json).version
$triple = 'x86_64-pc-windows-msvc'

$started = Get-Date
$logDir = Join-Path $repoRoot 'target\bundle-logs'

function Get-Elapsed([datetime]$since) { '{0:mm\:ss}' -f ((Get-Date) - $since) }

function Write-Step([string]$text) {
    Write-Host "`n[$(Get-Elapsed $started)] ==> $text" -ForegroundColor Cyan
}

function Format-Argument([string]$argument) {
    if ($argument -eq '' -or $argument -match '[\s"]') { return '"' + $argument.Replace('"', '\"') + '"' }
    return $argument
}

function Get-LastLine([string[]]$files) {
    foreach ($file in ($files | Sort-Object { (Get-Item $_ -ErrorAction SilentlyContinue).LastWriteTime } -Descending)) {
        $line = Get-Content $file -Tail 3 -ErrorAction SilentlyContinue | Where-Object { $_.Trim() } | Select-Object -Last 1
        if ($line) { return ($line -replace '\x1b\[[0-9;]*m', '').Trim() }
    }
    return ''
}

# Runs a program, shows that it is alive every few seconds (time spent and the last thing it
# printed) and stops with its output if it fails. The full output is kept in target\bundle-logs.
function Invoke-Native([string]$what, [string]$program, [string[]]$arguments) {
    New-Item -ItemType Directory -Force $logDir | Out-Null
    $slug = ($what -replace '[^A-Za-z0-9]+', '-').Trim('-').ToLower()
    $out = Join-Path $logDir "$slug.out.log"
    $err = Join-Path $logDir "$slug.err.log"
    $began = Get-Date
    Write-Host "    $what ..."
    $process = Start-Process -FilePath $program -ArgumentList (($arguments | ForEach-Object { Format-Argument $_ }) -join ' ') `
        -NoNewWindow -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
    # Reading the handle keeps the exit code available in Windows PowerShell 5.1.
    $null = $process.Handle
    $frames = '|', '/', '-', [string][char]92
    $tick = 0
    $last = ''
    $width = 100
    try { $width = [Math]::Max(40, $Host.UI.RawUI.WindowSize.Width - 1) } catch { $width = 100 }
    while (-not $process.WaitForExit(200)) {
        $tick++
        if ($tick % 5 -eq 1) { $last = Get-LastLine @($out, $err) }
        $line = '    {0} {1}  {2}  {3}' -f $frames[$tick % 4], (Get-Elapsed $began), $what, $last
        if ($line.Length -gt $width) { $line = $line.Substring(0, $width - 3) + '...' }
        Write-Host ("`r" + $line.PadRight($width)) -NoNewline -ForegroundColor DarkGray
    }
    Write-Host ("`r" + (' ' * $width) + "`r") -NoNewline
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) {
        Write-Host "`n--- last lines of the output ($out / $err) ---" -ForegroundColor Yellow
        Get-Content $out, $err -Tail 25 -ErrorAction SilentlyContinue | Write-Host
        throw "$what failed (exit code $($process.ExitCode)). Full output: $logDir"
    }
    Write-Host ("    done: {0} ({1})" -f $what, (Get-Elapsed $began)) -ForegroundColor Green
}

function Get-Download([string]$url, [string]$file) {
    if (Test-Path $file) { return }
    Write-Host "    downloading $url"
    New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
    # curl prints its progress meter on stderr; that must not count as an error.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { curl.exe -L -f --progress-bar -o "$file.part" $url 2>&1 | Out-Host } finally { $ErrorActionPreference = $previous }
    if ($LASTEXITCODE -ne 0) { throw "Downloading $url failed (curl exit code $LASTEXITCODE)." }
    Move-Item "$file.part" $file
}

function Get-Jre {
    $folder = Join-Path $cache 'jre-x64'
    if (Test-Path (Join-Path $folder 'bin\java.exe')) { return $folder }
    Write-Step 'Java runtime (Temurin 21)'
    $api = 'https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&image_type=jre&os=windows'
    $release = (Invoke-RestMethod -Uri $api -UseBasicParsing) | Select-Object -First 1
    $zip = Join-Path $cache (Split-Path $release.binary.package.link -Leaf)
    Get-Download $release.binary.package.link $zip
    $unpacked = Join-Path $cache 'jre-x64-unpacked'
    if (Test-Path $unpacked) { Remove-Item $unpacked -Recurse -Force }
    Expand-Archive $zip -DestinationPath $unpacked
    $inner = Get-ChildItem $unpacked -Directory | Select-Object -First 1
    if (Test-Path $folder) { Remove-Item $folder -Recurse -Force }
    Move-Item $inner.FullName $folder
    Remove-Item $unpacked -Recurse -Force
    return $folder
}

function Get-LibMpv {
    if ($LibMpv -and (Test-Path $LibMpv)) { return $LibMpv }
    $file = Join-Path $cache 'libmpv-x86_64\libmpv-2.dll'
    if (Test-Path $file) { return $file }
    Write-Step 'libmpv (mpv-winbuild)'
    $latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest' -UseBasicParsing
    $asset = $latest.assets | Where-Object { $_.name -match '^mpv-dev-x86_64-\d' } | Select-Object -First 1
    if (-not $asset) { throw 'No x86_64 libmpv package in the latest mpv-winbuild release.' }
    $archive = Join-Path $cache $asset.name
    Get-Download $asset.browser_download_url $archive
    $unpacked = Join-Path $cache 'libmpv-x86_64-unpacked'
    New-Item -ItemType Directory -Force $unpacked | Out-Null
    Invoke-Native 'Unpacking libmpv' 'tar' @('-xf', $archive, '-C', $unpacked)
    $dll = Get-ChildItem $unpacked -Recurse -Filter 'libmpv-2.dll' | Select-Object -First 1
    if (-not $dll) { throw "libmpv-2.dll is not in $($asset.name)." }
    New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
    Copy-Item $dll.FullName $file
    Remove-Item $unpacked -Recurse -Force
    return $file
}

# Gradle's start script puts the full path of each of the 77 jars on one line, and cmd.exe stops at
# 8191 characters ("The input line is too long") once the install folder has a longer name. The
# shipped copy lists the jars relative to the sidecar folder instead and starts in that folder.
function Repair-SidecarScript([string]$script) {
    $lines = Get-Content $script
    $fixed = foreach ($line in $lines) {
        if ($line -like 'set CLASSPATH=*') { $line.Replace('%APP_HOME%\lib\', 'lib\') }
        elseif ($line -like '"%JAVA_EXE%"*') { 'pushd "%APP_HOME%"'; $line }
        else { $line }
    }
    if (-not ($fixed -contains 'pushd "%APP_HOME%"') -or -not ($fixed | Where-Object { $_ -like 'set CLASSPATH=lib\*' })) {
        throw "The sidecar start script has a layout this build does not know: $script"
    }
    Set-Content $script $fixed -Encoding ASCII
}

# The Vulkan loader libmpv-2.dll imports; shipped beside it so a PC without a GPU driver can load libmpv.
# The build stops when it is missing or is built for another CPU type.
function Get-VulkanLoader {
    $vendored = Join-Path $PSScriptRoot 'vendor\vulkan\x64\vulkan-1.dll'
    $found = @($VulkanDll, $vendored) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $found) { throw "vulkan-1.dll not found. Expected $vendored (see scripts\vendor\vulkan\README.md) or pass -VulkanDll." }
    $bytes = [IO.File]::ReadAllBytes($found)
    $machine = '{0:X4}' -f [BitConverter]::ToUInt16($bytes, [BitConverter]::ToInt32($bytes, 0x3C) + 4)
    if ($machine -ne '8664') { throw "$found is built for CPU type $machine, but the x64 build needs 8664." }
    return $found
}

function Assert-InPayload([string]$archive, [string]$name) {
    $listing = & tar -tzf $archive
    if (-not ($listing | Where-Object { $_ -match "(^|/)$([regex]::Escape($name))$" })) {
        throw "$name is missing from the portable payload ($archive); the build is not safe to ship."
    }
}

# Lays out what ships beside the program: libmpv-2.dll, vulkan-1.dll, sidecar\, jre\.
function New-Stage {
    Write-Step 'Collecting libmpv, vulkan-1.dll, the sidecar and Java'
    $stage = Join-Path $stageRoot 'x64'
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item (Get-LibMpv) (Join-Path $stage 'libmpv-2.dll')
    Copy-Item (Get-VulkanLoader) (Join-Path $stage 'vulkan-1.dll')
    Copy-Item $SidecarHome (Join-Path $stage 'sidecar') -Recurse
    Repair-SidecarScript (Join-Path $stage 'sidecar\bin\cs-sidecar.bat')
    Copy-Item (Get-Jre) (Join-Path $stage 'jre') -Recurse
    return $stage
}

# The installer build copies the staged files next to the program through tauri's `resources`.
function New-BundleConfig([string]$stage) {
    foreach ($name in 'libmpv-2.dll', 'vulkan-1.dll', 'sidecar', 'jre') {
        if (-not (Test-Path (Join-Path $stage $name))) { throw "$name is not staged; the installer would ship without it." }
    }
    $slash = { param($p) $p.Replace('\', '/') }
    $resources = [ordered]@{}
    foreach ($name in 'libmpv-2.dll', 'vulkan-1.dll', 'sidecar', 'jre') { $resources[(& $slash (Join-Path $stage $name))] = $name }
    $file = Join-Path $stageRoot 'tauri.bundle.x64.json'
    @{ bundle = @{ resources = $resources } } | ConvertTo-Json -Depth 6 | Set-Content $file -Encoding UTF8
    return $file
}

function Build-App([string]$configFile) {
    Write-Step "Building the app ($triple)"
    Invoke-Native 'Adding the Rust target' 'rustup' @('target', 'add', $triple)
    Push-Location $appDir
    try {
        $bundling = if ($Kind -eq 'portable') { @('--no-bundle') } else { @('--bundles', 'nsis') }
        $tauri = Join-Path $frontendDir 'node_modules\.bin\tauri.cmd'
        Invoke-Native 'Compiling and packaging (the first build takes several minutes)' $tauri (@('build', '--target', $triple, '--config', $configFile) + $bundling)
    }
    finally { Pop-Location }
}

function Copy-Installer {
    $setup = Get-ChildItem (Join-Path $repoRoot "target\$triple\release\bundle\nsis") -Filter '*-setup.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw 'The installer was not produced.' }
    $dest = Join-Path $OutDir "CloudStream_${version}_x64-setup.exe"
    Copy-Item $setup.FullName $dest -Force
    return $dest
}

function New-Portable([string]$stage) {
    Write-Step 'Making the single-exe portable app'
    $exe = Join-Path $repoRoot "target\$triple\release\desktop_tauri.exe"
    $payloadFolder = Join-Path $stageRoot 'single-exe-x64\payload'
    if (Test-Path (Split-Path $payloadFolder)) { Remove-Item (Split-Path $payloadFolder) -Recurse -Force }
    New-Item -ItemType Directory -Force $payloadFolder | Out-Null
    Copy-Item $exe (Join-Path $payloadFolder 'desktop_tauri.exe')
    foreach ($name in 'libmpv-2.dll', 'vulkan-1.dll') { Copy-Item (Join-Path $stage $name) $payloadFolder }
    foreach ($name in 'sidecar', 'jre') { Copy-Item (Join-Path $stage $name) $payloadFolder -Recurse }

    $archive = Join-Path $stageRoot 'payload-x64.tar.gz'
    if (Test-Path $archive) { Remove-Item $archive }
    Invoke-Native 'Archiving the payload' 'tar' @('-czf', $archive, '-C', $payloadFolder, '.')

    Assert-InPayload $archive 'vulkan-1.dll'
    $env:CS_PAYLOAD_ARCHIVE = $archive
    Invoke-Native 'Compiling the single-exe launcher' 'cargo' @('build', '--release', '--target', $triple, '-p', 'portable_launcher')

    $dest = Join-Path $OutDir "CloudStream_${version}_x64_portable.exe"
    Copy-Item (Join-Path $repoRoot "target\$triple\release\portable_launcher.exe") $dest -Force
    return $dest
}

# --- main -------------------------------------------------------------------------------------

if (-not (Test-Path (Join-Path $SidecarHome 'bin'))) {
    throw "The extension sidecar is not built at '$SidecarHome'. Run scripts\setup_sidecar.ps1 or pass -SidecarHome."
}
New-Item -ItemType Directory -Force $OutDir, $cache | Out-Null

if (-not (Test-Path (Join-Path $frontendDir 'node_modules\.bin\tauri.cmd'))) {
    Write-Step 'Installing the frontend tools (npm install)'
    Push-Location $frontendDir
    try { Invoke-Native 'npm install' 'npm.cmd' @('install', '--no-audit', '--no-fund') } finally { Pop-Location }
}

$stage = New-Stage
Build-App (New-BundleConfig $stage)

$results = @()
if ($Kind -eq 'all' -or $Kind -eq 'installer') { $results += Copy-Installer }
if ($Kind -eq 'all' -or $Kind -eq 'portable') { $results += New-Portable $stage }

Write-Step "Done in $(Get-Elapsed $started)"
foreach ($file in $results) {
    $item = Get-Item $file
    '{0,-52} {1,8:N1} MB  sha256 {2}' -f $item.Name, ($item.Length / 1MB), (Get-FileHash $file).Hash.Substring(0, 16) | Write-Host
}
Write-Host "Output folder: $OutDir"
