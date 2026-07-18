# copy-exes.ps1 — fast dev/test build (NO installer, offline).
#
# Produces SELF-CONTAINED debug binaries and copies them into dist\:
#   - nivara.exe          dashboard UI with the frontend EMBEDDED (no dev server
#                         needed — plain `cargo build` makes a dev-mode exe that
#                         expects http://localhost:1420, which is why it showed a
#                         blank / "localhost refused to connect" screen)
#   - nivara-monitor.exe  windowless background agent
#
# Uses `tauri build --debug --no-bundle`: debug profile (fast), frontend baked
# in, and NO NSIS/MSI/WebView2 bundling (that's build-installer.ps1, on demand).

$root = $PSScriptRoot
$dist = Join-Path $root "dist"
$targetDir = Join-Path $root "src-tauri\target\debug"

Write-Host "Building self-contained debug app (tauri build --debug --no-bundle)..."
Push-Location $root
# Run via cmd so the `--` passthrough args reach the tauri CLI intact under PS 5.1.
cmd /c "npm run tauri build -- --debug --no-bundle"
$rc = $LASTEXITCODE
Pop-Location
if ($rc -ne 0) { throw "tauri debug build failed" }

if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Path $dist | Out-Null }

foreach ($exe in @("nivara.exe", "nivara-monitor.exe")) {
    $src = Join-Path $targetDir $exe
    if (-not (Test-Path $src)) { throw "missing build output: $src" }
    Copy-Item $src (Join-Path $dist $exe) -Force
    Write-Host "copied $exe -> dist\"
}

Write-Host "Done. Run:  $dist\nivara.exe"
