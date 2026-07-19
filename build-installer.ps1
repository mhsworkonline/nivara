# build-installer.ps1 — produce the release installer (NSIS + MSI).
#
#   .\build-installer.ps1 -Version 0.2.0   # bump version, then build
#   .\build-installer.ps1                   # build at the current version
#
# Versioned in-place upgrades: distribute the new setup.exe; running it over an
# existing install upgrades the program files. User data (SQLite DB, screenshots,
# password) lives in %LOCALAPPDATA%\Nivara and is NOT touched by install/upgrade/
# uninstall, so it always carries over. Bump the version each release — upgrades
# are recognized by version number.
#
# The background monitor (nivara-monitor.exe) is a second [[bin]] in the same
# cargo package; Tauri bundles it automatically next to nivara.exe. No sidecar
# staging is needed (using externalBin AND the package bin causes a WiX ICE30
# duplicate-component error, so we rely on the package bin only).

param([string]$Version)

# Note: native tools (cargo) write progress to stderr; do NOT use
# $ErrorActionPreference='Stop' here or PS 5.1 will abort on that stderr.
$root = $PSScriptRoot
$srcTauri = Join-Path $root "src-tauri"

if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version must be MAJOR.MINOR.PATCH, e.g. 0.2.0" }
    Write-Host "Bumping version -> $Version"
    # Read/write UTF-8 WITHOUT BOM via .NET (PS 5.1's Set-Content -Encoding utf8
    # adds a BOM and can mangle non-ASCII, which corrupts JSON/TOML).
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    function Edit-File($path, [scriptblock]$transform) {
        $text = [System.IO.File]::ReadAllText($path, $utf8)
        $text = & $transform $text
        [System.IO.File]::WriteAllText($path, $text, $utf8)
    }
    $conf = Join-Path $srcTauri "tauri.conf.json"
    $pkg = Join-Path $root "package.json"
    Edit-File $conf { param($t) $t -replace '("version"\s*:\s*")\d+\.\d+\.\d+(")', "`${1}$Version`${2}" }
    Edit-File $pkg { param($t) $t -replace '("version"\s*:\s*")\d+\.\d+\.\d+(")', "`${1}$Version`${2}" }
    # Cargo.toml: only the first version line ([package]); dependency versions
    # (two-part like "0.58") won't match the three-part pattern.
    $cargo = Join-Path $srcTauri "Cargo.toml"
    Edit-File $cargo { param($t) [regex]::new('(?m)^version\s*=\s*"\d+\.\d+\.\d+"').Replace($t, "version = `"$Version`"", 1) }
    Write-Host "Version set in tauri.conf.json, package.json, Cargo.toml."
}

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
