# Patch the Tauri-generated installer.nsi to skip the "uninstall previous
# version?" confirmation page (silent overwrite on upgrade), then re-run
# makensis so the setup.exe actually reflects the patch.
#
# Why a separate file: build-release.bat used to inline this PowerShell via
# `powershell -Command "... `r`n ..."` and cmd.exe mangled the backtick
# escapes ("此时不应有 `r`n"), aborting the whole build with exit 255 before
# signing. -File avoids every layer of cmd quoting.
#
# Called by build-release.bat after `cargo tauri build`; safe to re-run
# (idempotent patch + full rebuild of the versioned setup.exe).
param(
  [string]$NsiDir = (Join-Path $PSScriptRoot "..\src-tauri\target\release\nsis\x64")
)

$nsi = Join-Path $NsiDir "installer.nsi"
if (-not (Test-Path $nsi)) {
  Write-Host "[patch] installer.nsi not found at $nsi; skipping"
  exit 0
}

# The template is UTF-8 without BOM (contains 十七° in the defines) — read and
# write it as UTF-8 explicitly; PS5 would otherwise round-trip as ANSI.
$content = [System.IO.File]::ReadAllText($nsi)
if ($content -match "\[TokenStep patch\]") {
  Write-Host "[patch] already applied"
} elseif ($content -match "(?m)^Function PageReinstall\r?$") {
  $nl = if ($content.Contains("`r`n")) { "`r`n" } else { "`n" }
  $insert = "  ; [TokenStep patch] skip this page entirely (silent overwrite, no uninstall prompt)$nl  Abort$nl"
  $content = $content -replace "(?m)^(Function PageReinstall\r?\n)", "`$1$insert"
  [System.IO.File]::WriteAllText($nsi, $content)
  Write-Host "[patch] applied"
} else {
  Write-Host "[patch] PageReinstall marker not found; skipping rebuild"
  exit 0
}

# Version comes from the nsi itself so this never drifts from the build.
if ($content -notmatch '(?m)^!define VERSION "([^"]+)"') {
  Write-Host "[patch] VERSION define not found; aborting rebuild"
  exit 1
}
$version = $Matches[1]

# Prefer Bin\makensis.exe (469KB, the real compiler). The 91KB nsis.exe at
# the NSIS root is a GUI stub that silently exits 1 in console mode — the
# old build-release.bat preferred it, so the patch step quietly failed and
# NO released setup.exe ever carried the silent-overwrite patch until now.
$makensis = @(
  (Join-Path $env:LOCALAPPDATA "tauri\NSIS\Bin\makensis.exe"),
  (Join-Path $env:LOCALAPPDATA "tauri\NSIS\nsis.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $makensis) {
  Write-Host "[patch] makensis not found; keeping original setup.exe"
  exit 0
}

Write-Host "[patch] Re-running makensis to rebuild setup.exe"
& $makensis $nsi | Out-Null
if ($LASTEXITCODE -ne 0) {
  Write-Host "[patch][warn] makensis failed; keeping original setup.exe"
  exit 0
}

# The template hard-codes `!define OUTFILE "nsis-output.exe"`; Tauri renames
# it after its own run. A standalone re-run writes nsis-output.exe again —
# move it to the versioned bundle name so sign.bat picks it up.
$out = Join-Path $NsiDir "nsis-output.exe"
if (Test-Path $out) {
  $bundleDir = Join-Path $PSScriptRoot "..\src-tauri\target\release\bundle\nsis"
  if (-not (Test-Path $bundleDir)) { New-Item -ItemType Directory -Path $bundleDir | Out-Null }
  $dest = Join-Path $bundleDir "TokenStep_${version}_x64-setup.exe"
  Move-Item -Force $out $dest
  Write-Host "[patch] setup.exe rebuilt: $dest"
} else {
  Write-Host "[patch][warn] nsis-output.exe not produced; keeping original setup.exe"
}
