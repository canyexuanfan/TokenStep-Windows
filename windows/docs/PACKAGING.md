# Packaging & Release Guide (Windows)

This document defines **how a Windows release of TokenStep is built, named,
and shipped**. Follow it every time you cut a new version so the artifacts
never get mixed up again (yes, this exists because someone — me — kept
double-clicking an old versioned exe).

## Version source of truth

There is exactly **one** place that defines the version:

- `windows/src-tauri/tauri.conf.json` → `"version": "0.1.1"`

Bump this number first. Everything else (installer name, versioned exe,
GitHub Release tag) is derived from it automatically.

## Artifact naming convention

After a successful release build, the repo root (`TokenStep/`) contains:

| File | Purpose |
|------|---------|
| `TokenStep.exe` | Standalone exe, **no version** in the name — always the latest. For quick testing. |
| `TokenStep_v<ver>.exe` | Standalone exe, **versioned** (e.g. `TokenStep_v0.1.1.exe`). For distribution / end users. |
| `TokenStep_<ver>_x64-setup.exe` | NSIS installer, versioned (e.g. `TokenStep_0.1.1_x64-setup.exe`). |

**Rule:** when telling someone to "download the latest version", always point
them at the **versioned** files. The unversioned `TokenStep.exe` is a
convenience copy that gets overwritten every build — never distribute it.

## Build + sign + deploy, in one command

From the `windows/` directory:

```bat
scripts\build-release.bat
```

This runs `cargo tauri build`, then `scripts\sign.bat`, which:

1. Reads the version from `tauri.conf.json`.
2. Signs both `tokenstep.exe` and the NSIS installer with the self-signed cert.
3. Copies the signed files to the repo root with the correct versioned names.

You can also run signing alone (after a build):

```bat
scripts\sign.bat                REM auto-detect cert by subject
scripts\sign.bat <thumbprint>   REM use a specific cert thumbprint
```

## Full release checklist

1. **Bump version** in `windows/src-tauri/tauri.conf.json`.
2. **Update `CHANGELOG.md`** with what changed.
3. **Build + sign + deploy:**
   ```bat
   cd windows
   scripts\build-release.bat
   ```
4. **Verify** the three artifacts in the repo root (right-click → Properties →
   Details → should show the new version + a valid digital signature).
5. **Commit + push:**
   ```bat
   git add -A
   git commit -m "release: vX.Y.Z-windows"
   git push origin main
   ```
6. **Create a GitHub Release** tagged `vX.Y.Z-windows` and attach:
   - `TokenStep_v<ver>.exe`
   - `TokenStep_<ver>_x64-setup.exe`
7. **Sanity check** the download on a clean machine.

## Why these names?

- The `_v` prefix on the standalone exe (`TokenStep_v0.1.1.exe`) visually
  distinguishes it from the installer (`TokenStep_0.1.1_x64-setup.exe`) at a
  glance in File Explorer.
- Keeping an unversioned `TokenStep.exe` around means dev scripts and
  shortcuts can point at a stable path without editing them every release.
- The installer keeps the Tauri/NSIS default name shape
  (`TokenStep_<ver>_x64-setup.exe`) so Windows "Programs and Features"
  and upgrade detection keep working.
