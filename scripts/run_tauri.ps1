# Starts the Tauri desktop app for manual testing: the Vite dev server first, then the app window.
# Closing the window stops both. Paths can be overridden with parameters.
param(
    [string]$DataDir = 'C:\cs3d\appdata2',
    [string]$LibMpv = 'C:\cs3d\mpv\libmpv-2.dll',
    [string]$SidecarHome = 'C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar',
    [string]$JavaHome = 'C:\Program Files\JetBrains\IntelliJ IDEA Community Edition 2025.2.4\jbr',
    [string]$LogLevel = 'info,desktop_tauri=debug'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$frontendDir = Join-Path $repoRoot 'crates\desktop_tauri\frontend'
$devServerPort = 5173

function Assert-Exists([string]$path, [string]$what) {
    if (-not (Test-Path $path)) {
        throw "$what not found at '$path'. Pass the right location with the matching parameter."
    }
}

function Wait-ForPort([int]$port, [int]$timeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($timeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-NetConnection -ComputerName localhost -Port $port -InformationLevel Quiet -WarningAction SilentlyContinue) {
            return
        }
        Start-Sleep -Milliseconds 500
    }
    throw "The Vite dev server did not start on port $port within $timeoutSeconds seconds."
}

Assert-Exists $LibMpv 'libmpv'
Assert-Exists $SidecarHome 'The extension sidecar'
Assert-Exists $JavaHome 'The JDK'

$env:CS_DATA_DIR = $DataDir
$env:CS_LIBMPV_PATH = $LibMpv
$env:CS_SIDECAR_HOME = $SidecarHome
$env:CS_SIDECAR_JAVA_HOME = $JavaHome
$env:RUST_LOG = $LogLevel

$vite = Start-Process -FilePath 'npm.cmd' -ArgumentList 'run', 'dev' -WorkingDirectory $frontendDir -WindowStyle Hidden -PassThru
try {
    Wait-ForPort $devServerPort 60
    Write-Host "Vite is up (pid $($vite.Id)). Starting the app; close its window to stop everything."
    Push-Location $repoRoot
    cargo run -p desktop_tauri
}
finally {
    Pop-Location -ErrorAction SilentlyContinue
    if ($vite -and -not $vite.HasExited) {
        taskkill /T /F /PID $vite.Id | Out-Null
    }
}
