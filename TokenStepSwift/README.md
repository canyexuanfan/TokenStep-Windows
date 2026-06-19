# TokenStep SwiftUI

Native SwiftUI shell for TokenStep.

The canonical local run path is:

```bash
./script/build_swiftui_and_run.sh --verify
```

This project intentionally uses a direct `swiftc` build script right now instead of SwiftPM because the local Command Line Tools install reports a `PackageDescription` manifest link failure even after updating CLT. The run script also applies a temporary VFS overlay that hides a stale `module.modulemap` left in the CLT include directory, without modifying `/Library/Developer`.

The existing PyObjC prototype is still available through:

```bash
./script/build_pyobjc_and_run.sh --verify
```
