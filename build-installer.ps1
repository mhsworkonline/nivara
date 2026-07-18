# build-installer.ps1 — produce the release installer (NSIS + MSI).
#
# The background monitor (nivara-monitor.exe) is a second [[bin]] in the same
# cargo package; Tauri bundles it automatically next to nivara.exe. No sidecar
# staging is needed (using externalBin AND the package bin causes a WiX ICE30
# duplicate-component error, so we rely on the package bin only).

# Note: native tools (cargo) write progress to stderr; do NOT use
# $ErrorActionPreference='Stop' here or PS 5.1 will abort on that stderr.
$root = $PSScriptRoot
$srcTauri = Join-Path $root "src-tauri"

Write-Host "Running tauri build (frontend + both bins + installers)..."
Push-Location $root
npm run tauri build
$rc = $LASTEXITCODE
Pop-Location
if ($rc -ne 0) { throw "tauri build failed" }

$bundle = Join-Path $srcTauri "target\release\bundle"
Write-Host "`nInstaller output:"
Get-ChildItem -Recurse -Path $bundle -Include *.exe, *.msi -ErrorAction SilentlyContinue |
    ForEach-Object { Write-Host "  $($_.FullName)" }

# Final step: dist\ holds exactly one file — the current NSIS installer.
# Retry on transient locks (e.g. antivirus scanning the freshly written exe).
$nsis = Get-ChildItem (Join-Path $bundle "nsis\*.exe") -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $nsis) { throw "no NSIS installer found to copy into dist" }
$dist = Join-Path $root "dist"
$dest = Join-Path $dist $nsis.Name
if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Path $dist | Out-Null }

$done = $false
for ($i = 1; $i -le 10 -and -not $done; $i++) {
    try {
        Get-ChildItem $dist -Force | Remove-Item -Recurse -Force -ErrorAction Stop
        Copy-Item $nsis.FullName $dest -Force -ErrorAction Stop
        $done = $true
    } catch {
        Write-Host "dist copy attempt $i failed (locked?); retrying..."
        Start-Sleep -Seconds 3
    }
}
if (-not $done) { throw "could not copy installer into dist (file stayed locked)" }

# Verify: dist holds exactly the fresh installer (matching source timestamp).
$got = @(Get-ChildItem $dist -Force)
if ($got.Count -ne 1 -or $got[0].Name -ne $nsis.Name -or $got[0].LastWriteTime -ne $nsis.LastWriteTime) {
    throw "dist verification failed: $($got.Name -join ', ')"
}
Write-Host "dist\ now contains only the current installer: $($nsis.Name)"
