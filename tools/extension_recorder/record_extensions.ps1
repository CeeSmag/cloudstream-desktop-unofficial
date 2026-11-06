<#
.SYNOPSIS
    CloudStream Extension Response Recorder (High-Performance Parallel Edition)

.DESCRIPTION
    Automates the ingestion of a repository (e.g. MegaRepo), discovers all added repositories
    via cs-sidecar, downloads and test-loads every discovered extension across multiple concurrent
    workers in parallel, and records all replies and stderr logs into the script directory.

.PARAMETER RepoUrl
    The URL of the repository to record. Defaults to MegaRepo.

.PARAMETER MaxPlugins
    Limit the number of plugins to test (0 = test all discovered plugins).

.PARAMETER Workers
    Number of concurrent sidecar worker processes to run in parallel (default: 4).

.PARAMETER SidecarHome
    Path to the installed cs-sidecar directory. Auto-detected if omitted.

.PARAMETER Jdk21
    Path to the JDK 21 installation home directory. Auto-detected if omitted.

.PARAMETER TimeoutSec
    Timeout in seconds for each plugin load operation (default: 45).

.PARAMETER Untrusted
    If specified, enables static bytecode security verifier (blocks HttpURLConnection/threads).
    By default, trusted mode is enabled to match CloudStream verified behavior.

.EXAMPLE
    .\record_extensions.cmd
    # Runs the full multi-threaded audit (4 parallel workers, trusted mode)

.EXAMPLE
    .\record_extensions.cmd -Workers 8
    # Blazing-fast 8-worker parallel scan
#>
[CmdletBinding()]
param(
    [string]$RepoUrl = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json",
    [int]$MaxPlugins = 0,
    [int]$Workers = 4,
    [string]$SidecarHome = "",
    [string]$Jdk21 = "",
    [int]$TimeoutSec = 45,
    [switch]$SkipDownload,
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
if ($Jdk21) {
    $Arguments += @("--jdk21", $Jdk21)
}
if ($SkipDownload) {
    $Arguments += "--skip-download"
}
if ($Untrusted) {
    $Arguments += "--untrusted"
}

# Execute python script and pass through all output directly
& $PythonExe $Arguments
exit $LASTEXITCODE
