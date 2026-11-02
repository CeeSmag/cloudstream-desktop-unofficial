<#
Builds the JVM extension sidecar.

Clones the GPL-3.0 CS3 desktop client at a pinned commit (it provides the DEX->JVM
converter, Android stubs and the original CloudStream library), overlays our
sidecar/cs-sidecar module, and builds it. Output: <Root>\repo\cs-sidecar\build\install\cs-sidecar

Usage:  powershell -File scripts\setup_sidecar.ps1 [-Root C:\cs3d] [-Jdk21 <path to a JDK 21 home>]
Needs JDK 21 (the desktop client's sandbox uses the Security Manager, removed in JDK 24+).
Keep -Root short: the CloudStream library has very long file paths.
#>
param(
    [string]$Root = "C:\cs3d",
    [string]$Jdk21 = ""
)

$ErrorActionPreference = "Stop"
$PinnedCommit = "2d5a86ce2e7466494a9084776dcddd3626f633dd"
$RepoUrl = "https://github.com/errorcode26/CS3-desktop-client-unofficial.git"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Repo = Join-Path $Root "repo"

function Write-Step($message) { Write-Host "[setup_sidecar] $message" }

function Find-Jdk21 {
    if ($Jdk21 -and (Test-Path (Join-Path $Jdk21 "bin\java.exe"))) { return $Jdk21 }
    $candidates = @(
        "C:\Program Files\Java\jbr-21*",
        "C:\Program Files\Java\jdk-21*",
        "C:\Program Files\Eclipse Adoptium\jdk-21*",
        "C:\Program Files\JetBrains\*\jbr"
    )
    foreach ($pattern in $candidates) {
        foreach ($dir in Get-ChildItem -Path $pattern -Directory -ErrorAction SilentlyContinue) {
            $java = Join-Path $dir.FullName "bin\java.exe"
            if ((Test-Path $java) -and ((& $java -version 2>&1 | Select-Object -First 1) -match '"21\.')) {
                return $dir.FullName
            }
        }
    }
    throw "No JDK 21 found. Install one or pass -Jdk21 <path>."
}

if (-not (Test-Path (Join-Path $Repo ".git"))) {
    Write-Step "cloning desktop client into $Repo"
    New-Item -ItemType Directory -Force $Root | Out-Null
    git -c core.longpaths=true clone --no-checkout $RepoUrl $Repo
}
Push-Location $Repo
try {
    git config core.longpaths true
    git fetch --quiet origin $PinnedCommit
    git checkout --quiet $PinnedCommit
    git submodule update --init --depth 1
} finally { Pop-Location }

Write-Step "installing sidecar module"
$Target = Join-Path $Repo "cs-sidecar"
if (Test-Path $Target) { Remove-Item -Recurse -Force $Target }
Copy-Item -Recurse (Join-Path $ProjectRoot "sidecar\cs-sidecar") $Target
$Settings = Join-Path $Repo "settings.gradle.kts"
if (-not (Select-String -Path $Settings -Pattern 'include\(":cs-sidecar"\)' -Quiet)) {
    Add-Content $Settings "`ninclude("":cs-sidecar"")"
}

$env:JAVA_HOME = Find-Jdk21
Write-Step "building with JAVA_HOME=$env:JAVA_HOME"
Push-Location $Repo
try {
    & .\gradlew.bat --no-daemon "-Dorg.gradle.jvmargs=-Xmx4G -Dfile.encoding=UTF-8" ":cs-sidecar:installDist"
    if ($LASTEXITCODE -ne 0) { throw "gradle build failed" }
} finally { Pop-Location }

Write-Step "done. Set CS_SIDECAR_HOME=$Target\build\install\cs-sidecar and CS_SIDECAR_JAVA_HOME=$env:JAVA_HOME"
