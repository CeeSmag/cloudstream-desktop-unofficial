# Builds the Windows release: an installer and a portable zip for each CPU type.
#
#   .\scripts\build_release.ps1                       # installer + portable, x64 and arm64
#   .\scripts\build_release.ps1 -Arch x64             # only 64-bit Intel/AMD
#   .\scripts\build_release.ps1 -Kind portable        # only the portable zip
#
# Every package carries what the app needs to run on a clean PC: libmpv, the extension sidecar
# and a Java runtime of its own (downloaded once into target\bundle-cache). Results land in .\dist.
param(
    [ValidateSet('x64', 'arm64')][string[]]$Arch = @('x64', 'arm64'),
    [ValidateSet('all', 'installer', 'portable')][string]$Kind = 'all',
    [string]$SidecarHome = 'C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar',
    # Where to take libmpv-2.dll from; when absent it is downloaded (mpv-winbuild, latest).
    [string]$LibMpvX64 = 'C:\cs3d\mpv\libmpv-2.dll',
    [string]$LibMpvArm64 = '',
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

# What differs between the two CPU types.
$targets = @{
    x64   = @{ Triple = 'x86_64-pc-windows-msvc'; Jre = 'x64'; Mpv = 'x86_64' }
    arm64 = @{ Triple = 'aarch64-pc-windows-msvc'; Jre = 'aarch64'; Mpv = 'aarch64' }
}

function Write-Step([string]$text) { Write-Host "`n==> $text" -ForegroundColor Cyan }

# Runs a program and stops if it fails. Programs such as rustup and cargo report progress on
# stderr, which Windows PowerShell would turn into errors, so only the exit code is judged.
function Invoke-Native([string]$what, [scriptblock]$command) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $command } finally { $ErrorActionPreference = $previous }
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE)." }
}

function Get-Download([string]$url, [string]$file) {
    if (Test-Path $file) { return }
    Write-Host "    downloading $url"
    New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
    Invoke-WebRequest -Uri $url -OutFile "$file.part" -UseBasicParsing
    Move-Item "$file.part" $file
}

function Assert-ArmToolchain {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $found = $false
    if (Test-Path $vswhere) {
        $found = [bool](& $vswhere -products '*' -requires 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' -property installationPath)
    }
    if (-not $found) {
        throw @"
The ARM64 C++ build tools are not installed, and the arm64 build needs them.
Install them once (as administrator), then run this script again:
  & "`${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\setup.exe" modify --installPath "<your Visual Studio / Build Tools folder>" --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --passive
or tick "MSVC v143 - VS 2022 C++ ARM64/ARM64EC build tools" in the Visual Studio Installer.
Or build only the other CPU type with:  .\scripts\build_release.ps1 -Arch x64
"@
    }
}

function Get-Jre([string]$arch) {
    $name = $targets[$arch].Jre
    $folder = Join-Path $cache "jre-$name"
    if (Test-Path (Join-Path $folder 'bin\java.exe')) { return $folder }
    Write-Step "Java runtime for $arch (Temurin 21)"
    $api = "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=$name&image_type=jre&os=windows"
    $release = (Invoke-RestMethod -Uri $api -UseBasicParsing) | Select-Object -First 1
    $zip = Join-Path $cache (Split-Path $release.binary.package.link -Leaf)
    Get-Download $release.binary.package.link $zip
    $unpacked = Join-Path $cache "jre-$name-unpacked"
    if (Test-Path $unpacked) { Remove-Item $unpacked -Recurse -Force }
    Expand-Archive $zip -DestinationPath $unpacked
    $inner = Get-ChildItem $unpacked -Directory | Select-Object -First 1
    if (Test-Path $folder) { Remove-Item $folder -Recurse -Force }
    Move-Item $inner.FullName $folder
    Remove-Item $unpacked -Recurse -Force
    return $folder
}

function Get-LibMpv([string]$arch, [string]$given) {
    if ($given -and (Test-Path $given)) { return $given }
    if ($given) { throw "libmpv not found at '$given'." }
    $name = $targets[$arch].Mpv
    $file = Join-Path $cache "libmpv-$name\libmpv-2.dll"
    if (Test-Path $file) { return $file }
    Write-Step "libmpv for $arch (mpv-winbuild)"
    $latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest' -UseBasicParsing
    $asset = $latest.assets | Where-Object { $_.name -match "^mpv-dev-$name-\d" } | Select-Object -First 1
    if (-not $asset) { throw "No libmpv package for $name in the latest mpv-winbuild release." }
    $archive = Join-Path $cache $asset.name
    Get-Download $asset.browser_download_url $archive
    $unpacked = Join-Path $cache "libmpv-$name-unpacked"
    New-Item -ItemType Directory -Force $unpacked | Out-Null
    Invoke-Native 'Unpacking libmpv' { tar -xf $archive -C $unpacked }
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

# Lays out what ships beside the program: libmpv-2.dll, sidecar\, jre\.
function New-Stage([string]$arch, [string]$libMpv, [string]$jre) {
    $stage = Join-Path $stageRoot $arch
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item $libMpv (Join-Path $stage 'libmpv-2.dll')
    Copy-Item $SidecarHome (Join-Path $stage 'sidecar') -Recurse
    Repair-SidecarScript (Join-Path $stage 'sidecar\bin\cs-sidecar.bat')
    Copy-Item $jre (Join-Path $stage 'jre') -Recurse
    return $stage
}

# The installer build copies the staged files next to the program through tauri's `resources`.
function New-BundleConfig([string]$arch, [string]$stage) {
    $slash = { param($p) $p.Replace('\', '/') }
    $config = @{
        bundle = @{
            resources = [ordered]@{
                (& $slash (Join-Path $stage 'libmpv-2.dll')) = 'libmpv-2.dll'
                (& $slash (Join-Path $stage 'sidecar'))      = 'sidecar'
                (& $slash (Join-Path $stage 'jre'))          = 'jre'
            }
        }
    }
    $file = Join-Path $stageRoot "tauri.bundle.$arch.json"
    $config | ConvertTo-Json -Depth 6 | Set-Content $file -Encoding UTF8
    return $file
}

function Build-App([string]$arch, [string]$configFile) {
    $triple = $targets[$arch].Triple
    Write-Step "Building $arch ($triple)"
    Invoke-Native 'rustup target add' { rustup target add $triple }
    Push-Location $appDir
    try {
        $bundling = if ($Kind -eq 'portable') { @('--no-bundle') } else { @('--bundles', 'nsis') }
        Invoke-Native "tauri build ($arch)" {
            & (Join-Path $frontendDir 'node_modules\.bin\tauri.cmd') build --target $triple --config $configFile @bundling
        }
    }
    finally { Pop-Location }
}

function Copy-Installer([string]$arch) {
    $triple = $targets[$arch].Triple
    $setup = Get-ChildItem (Join-Path $repoRoot "target\$triple\release\bundle\nsis") -Filter '*-setup.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw "The $arch installer was not produced." }
    $dest = Join-Path $OutDir "CloudStream_${version}_${arch}-setup.exe"
    Copy-Item $setup.FullName $dest -Force
    return $dest
}

function New-Portable([string]$arch, [string]$stage) {
    $triple = $targets[$arch].Triple
    $exe = Join-Path $repoRoot "target\$triple\release\desktop_tauri.exe"
    $folder = Join-Path $stageRoot "portable-$arch\CloudStream"
    if (Test-Path (Split-Path $folder)) { Remove-Item (Split-Path $folder) -Recurse -Force }
    New-Item -ItemType Directory -Force $folder | Out-Null
    Copy-Item $exe (Join-Path $folder 'CloudStream.exe')
    Copy-Item (Join-Path $stage 'libmpv-2.dll') $folder
    Copy-Item (Join-Path $stage 'sidecar') $folder -Recurse
    Copy-Item (Join-Path $stage 'jre') $folder -Recurse
    # The marker makes the app keep its data in the "data" folder next to it.
    Set-Content (Join-Path $folder 'portable.txt') 'Everything CloudStream stores is kept in the "data" folder next to this file. Delete this file to use the normal user folder instead.'
    $zip = Join-Path $OutDir "CloudStream_${version}_${arch}_portable.zip"
    if (Test-Path $zip) { Remove-Item $zip }
    Invoke-Native 'Zipping the portable app' { tar -a -cf $zip -C (Split-Path $folder) 'CloudStream' }
    return $zip
}

# --- main -------------------------------------------------------------------------------------

if (-not (Test-Path (Join-Path $SidecarHome 'bin'))) {
    throw "The extension sidecar is not built at '$SidecarHome'. Run scripts\setup_sidecar.ps1 or pass -SidecarHome."
}
if ($Arch -contains 'arm64') { Assert-ArmToolchain }
New-Item -ItemType Directory -Force $OutDir, $cache | Out-Null

if (-not (Test-Path (Join-Path $frontendDir 'node_modules\.bin\tauri.cmd'))) {
    Write-Step 'Installing the frontend tools (npm install)'
    Push-Location $frontendDir
    try { Invoke-Native 'npm install' { npm.cmd install --no-audit --no-fund } } finally { Pop-Location }
}

$results = @()
foreach ($a in $Arch) {
    $given = if ($a -eq 'x64') { $LibMpvX64 } else { $LibMpvArm64 }
    if ($a -eq 'x64' -and $given -and -not (Test-Path $given)) { $given = '' }
    $stage = New-Stage $a (Get-LibMpv $a $given) (Get-Jre $a)
    Build-App $a (New-BundleConfig $a $stage)
    if ($Kind -ne 'portable') { $results += Copy-Installer $a }
    if ($Kind -ne 'installer') { $results += New-Portable $a $stage }
}

Write-Step 'Done'
foreach ($file in $results) {
    $item = Get-Item $file
    '{0,-52} {1,8:N1} MB  sha256 {2}' -f $item.Name, ($item.Length / 1MB), (Get-FileHash $file).Hash.Substring(0, 16) | Write-Host
}
Write-Host "Output folder: $OutDir"
