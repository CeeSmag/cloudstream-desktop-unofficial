<#
Runs the Kotlin tests of the JVM sidecar: the cs-sidecar module (sidecar/cs-sidecar/src/test), the
plugin runtime with our overlay (sidecar/overlay/plugin-runtime/src/test plus the pinned reference
tests it does not replace), which holds the plugin sandbox: bytecode verifier, call rewrites, stubs,
and the API gap scanner (sidecar/api-scanner/src/test).

The sidecar is built inside the CS3 desktop client's Gradle project (see setup_sidecar.ps1), so this
applies the overlay, copies our modules' sources and build files into <Root>\repo and runs
`:plugin-runtime:test :cs-sidecar:test :api-scanner:test` there. Run setup_sidecar.ps1 once first.

Usage:  powershell -File scripts\test_sidecar.ps1 [-Root C:\cs3d] [-Jdk21 <path to a JDK 21 home>]
#>
param(
    [string]$Root = "C:\cs3d",
    [string]$Jdk21 = ""
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Repo = Join-Path $Root "repo"
$Target = Join-Path $Repo "cs-sidecar"

if (-not (Test-Path $Target)) {
    throw "The sidecar project is not set up at '$Target'. Run scripts\setup_sidecar.ps1 first."
}
if ($Jdk21 -and (Test-Path (Join-Path $Jdk21 "bin\java.exe"))) { $env:JAVA_HOME = $Jdk21 }
if (-not $env:JAVA_HOME) { throw "Pass -Jdk21 <JDK 21 home> or set JAVA_HOME to a JDK 21." }

Write-Host "[test_sidecar] applying overlay patches into $Repo"
$Overlay = Join-Path $ProjectRoot "sidecar\overlay"
if (Test-Path $Overlay) {
    Copy-Item -Recurse -Force "$Overlay\*" $Repo
}

foreach ($Module in @("cs-sidecar", "api-scanner")) {
    $ModuleTarget = Join-Path $Repo $Module
    Write-Host "[test_sidecar] copying $Module into $ModuleTarget"
    if (-not (Test-Path $ModuleTarget)) {
        throw "Module '$Module' is not set up at '$ModuleTarget'. Run scripts\setup_sidecar.ps1 again."
    }
    Copy-Item -Recurse -Force (Join-Path $ProjectRoot "sidecar\$Module\src") $ModuleTarget
    Copy-Item -Force (Join-Path $ProjectRoot "sidecar\$Module\build.gradle.kts") (Join-Path $ModuleTarget "build.gradle.kts")
}

Push-Location $Repo
try {
    # --continue: report the failures of both modules, not only of the first one that fails.
    & .\gradlew.bat --no-daemon --continue "-Dorg.gradle.jvmargs=-Xmx4G -Dfile.encoding=UTF-8" ":plugin-runtime:test" ":cs-sidecar:test" ":api-scanner:test"
    if ($LASTEXITCODE -ne 0) { throw "The sidecar tests failed (exit code $LASTEXITCODE)." }
} finally {
    Pop-Location
}
Write-Host "[test_sidecar] done"
