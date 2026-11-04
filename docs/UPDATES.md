# Updates and release publishing

The portable updater is embedded in Clipdeck. It uses Windows WinHTTP for HTTPS,
CNG ECDSA P-256 to verify signed metadata, and SHA-256 to verify the executable.
The ZIP remains the manual installation package; the updater downloads the raw
versioned EXE to avoid a runtime ZIP dependency. Existing Rust dependency versions
are unchanged; the `windows` crate enables WinHTTP and cryptography features.

## Signing identity

The public key is `assets/update-public-key.bin`. The corresponding non-exportable
private key is held by the Windows user key store under the name
`Clipdeck.UpdateSigning.v1`, Microsoft Software Key Storage Provider. It is never
included in Git or release assets. `scripts/sign-update.ps1` refuses to sign with a
key whose public portion differs from the checked-in key.

Keep this Windows signing profile available. Losing the key requires a manual
bootstrap/key transition; changing the public key alone breaks existing clients.
Update signing does not give the executable an Authenticode signature or remove
SmartScreen prompts.

## Build and package

Close a running development copy before rebuilding its executable. With Rust
1.99.1, Visual Studio Build Tools/Windows SDK and Python installed:

```powershell
powershell -NoProfile -File scripts/build-release.ps1 -Notes docs/releases/v0.1.4.md -Schema 5
```

The script runs fmt, clippy with warnings denied, tests and the release build,
checks Windows version metadata, packages the manual ZIP, signs the updater
manifest and writes checksums into `target/dist/v<version>/`.

The signing key has already been initialized for this repository. On a new
project identity only, `sign-update.ps1 -Initialize -Manifest <file>` creates a
key and writes its public key if no public key exists. Do not initialize a
replacement identity casually.

Publish these assets together on a stable `v<version>` GitHub release:

- `clipdeck-<version>-windows-x64.zip`
- `clipdeck-<version>-windows-x64.exe`
- `clipdeck-update.toml`
- `clipdeck-update.toml.sig`
- `SHA256SUMS.txt`

`schema` is the highest database schema the candidate can open. Update it when
changing migrations. The manifest format currently accepts stable three-part
versions. It has bounded size, executable size, checksum and notes fields.
The publisher must build the corresponding version into Windows file metadata.

## Install and recovery protocol

1. A manual check, or opted-in daily check, fetches the latest release's manifest
   and detached signature. Only HTTPS GitHub/release asset hosts are allowed;
   redirects, metadata size, body size and request duration are bounded.
2. The manifest signature is verified before it is parsed or offered. Versions
   at or below the running version are never offered as updates.
3. The candidate downloads to a private staging folder. Its exact size, SHA-256,
   x64 PE architecture and native Windows version must match the signed manifest.
4. On Update and restart, a verified candidate is staged beside the original
   executable. A copy of the running executable starts in helper mode before the
   single-instance guard. It validates the parent PID, creation time, image path,
   previous hash, signature and candidate again, then acknowledges readiness.
5. Clipdeck quits normally. The helper waits for the actual process to exit,
   creates and integrity-checks a consistent SQLite `VACUUM INTO` snapshot, and
   uses Windows `ReplaceFileW` with an executable backup.
6. The new process verifies its restart job, opens/migrates history and initializes
   the application, then acknowledges startup before consuming clipboard events.
   This prevents automatic recovery from discarding newly captured clips.
7. Failure before acknowledgement stops only the attempted child, restores the
   history snapshot and previous executable, and restarts the old version. Failed
   migrated history and recovery artifacts are preserved. An external database
   viewer or restrictive filesystem permissions can prevent recovery; the helper
   then reports the recovery location instead of overwriting locked files.

The helper never elevates privileges or kills the original app. Read-only folders,
download/signature failures and startup timeouts leave an explicit error. Recovery
backups are retained for inspection. Do not replace the database with an older
snapshot after the new version has acknowledged startup and captured more clips.

## Tests

Ordinary tests cover signed metadata tampering, stable version ordering and path
injection, metadata limits, redirect restrictions, bounded job data and a WAL
snapshot restored after a failed schema change.

`scripts/test-updater.py` runs three native process scenarios using isolated
APPDATA/LOCALAPPDATA folders: success, a locked executable, and a deliberately
failing candidate that changes schema and deletes fixture rows before exiting.
The script checks text, image bytes, copy/paste counters, executable hashes and
startup confirmation. It only terminates processes inside its own fixture root.

The baseline must be a debug build with an earlier version, the same updater code
and the same public key. Its debug-only `--test-update <stage>` entry point uses
the production signature checks and helper. This test entry point is absent from
normal release builds. Pillow is used only to create image fixtures.

```powershell
python scripts/test-updater.py --baseline <internal-debug-baseline.exe> --candidate target/release/clipdeck.exe
```

A published v0.1.1 binary cannot update itself; it predates this mechanism.
