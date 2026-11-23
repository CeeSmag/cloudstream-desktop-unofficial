<#
.SYNOPSIS
    CloudStream extension recorder: what the desktop runtime lacks for video plugins.

.DESCRIPTION
    Discovers every repository of a seed repository (e.g. MegaRepo) via cs-sidecar, keeps the video
    plugins only (the app's content rule), then either scans them statically for Android /
    CloudStream API gaps (-Mode scan, default: replies\api_gaps.md) or loads each one in sidecar
    workers (-Mode load: replies\summary.md). Excluded plugins: replies\excluded_plugins.json.

.PARAMETER Mode
    scan (default): static API gap report. load: load every plugin in sidecar workers.

.PARAMETER RepoUrl
    The URL of the repository to record. Defaults to MegaRepo.

.PARAMETER MaxPlugins
    Limit the number of plugins to test (0 = test all discovered plugins).

.PARAMETER Workers
    Number of concurrent sidecar worker processes to run in parallel (default: 4).

.PARAMETER SidecarHome
    Path to the installed cs-sidecar directory. Auto-detected if omitted.

.PARAMETER ScannerHome
    Path to the installed api-scanner directory (scan mode). Auto-detected if omitted
    (scripts\setup_sidecar.ps1, then gradlew :api-scanner:installDist in C:\cs3d\repo).

.PARAMETER Jdk21
    Path to the JDK 21 installation home directory. Auto-detected if omitted.

.PARAMETER TimeoutSec
    Timeout in seconds for each plugin load operation (default: 45).

.PARAMETER Untrusted
    If specified, enables static bytecode security verifier (blocks HttpURLConnection/threads).
    By default, trusted mode is enabled to match CloudStream verified behavior.

.EXAMPLE
    .\record_extensions.cmd
    # Static API gap scan of every video plugin

.EXAMPLE
    .\record_extensions.cmd -Mode load -Workers 8
    # Load every video plugin in 8 parallel sidecars
#>
[CmdletBinding()]
param(
    [ValidateSet("scan", "load")]
    [string]$Mode = "scan",
    [string]$RepoUrl = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json",
    [int]$MaxPlugins = 0,
    [int]$Workers = 4,
    [string]$SidecarHome = "",
    [string]$ScannerHome = "",
    [string]$Jdk21 = "",
    [int]$TimeoutSec = 45,
    [switch]$Untrusted
)

$ErrorActionPreference = "Stop"
$ScriptDir = $PSScriptRoot

# Locate Python
$PythonExe = (Get-Command python.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1)
if (-not $PythonExe) {
    $PythonExe = (Get-Command py.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1)
}
if (-not $PythonExe) {
    Write-Error "Python 3 is required to run the extension recorder, but was not found in PATH."
    exit 1
}

$RecorderPy = Join-Path $ScriptDir "recorder.py"
if (-not (Test-Path $RecorderPy)) {
    Write-Error "Could not find recorder.py in $ScriptDir"
    exit 1
}

$Arguments = @(
    $RecorderPy,
    "--mode", $Mode,
    "--repo-url", $RepoUrl,
    "--timeout", $TimeoutSec,
    "--workers", $Workers
)

if ($MaxPlugins -gt 0) {
    $Arguments += @("--max-plugins", $MaxPlugins)
}
if ($SidecarHome) {
    $Arguments += @("--sidecar-home", $SidecarHome)
}
if ($ScannerHome) {
    $Arguments += @("--scanner-home", $ScannerHome)
}
if ($Jdk21) {
    $Arguments += @("--jdk21", $Jdk21)
}
if ($Untrusted) {
    $Arguments += "--untrusted"
}

# Execute python script and pass through all output directly
& $PythonExe $Arguments
exit $LASTEXITCODE
