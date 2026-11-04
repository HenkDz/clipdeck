# Clipdeck milestone 2

## Milestone 2 review follow-up (2026-11-04)

Local follow-up on `Nour/m2-fixes`, based on `Nour/m2`; no remote operations.
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test -- --nocapture` pass: **17 passed, 1 ignored**. The ignored test
writes the real clipboard and was not rerun for this follow-up.

Schema v2 migrates v1 transactionally, retaining IDs, pin/recency metadata,
thumbnails and original format blobs. It rebuilds FTS and vacuums after the
conversion to reclaim old payload/index pages. Migration failure rolls back;
unknown future versions are rejected. Existing clips are preserved regardless
of the new capture cap. Conversion and vacuum need temporary disk space.

- Dedupe uses `UNIQUE(kind,hash)`, where hash is SHA-256 of UTF-8 kind, a NUL
  separator, and the full primary payload. `sha2` reuses locked version 0.11.1.
- When payload bytes equal UTF-8 text bytes, `payload` is SQL NULL; hydration
  derives it with `CAST(text AS BLOB)`. Distinct binary payloads remain blobs.
  Original clipboard formats, including Unicode/RTF/HTML, remain intact.
- Search lowercases only a UTF-8 boundary-safe prefix and caps the result at
  65,536 bytes. Both the search column and FTS index use that prefix. Text after
  it is available for pasting/editing, but is intentionally not searchable.
- Lists return `substr(text,1,512)`, metadata and thumbnails. Full text, payload
  and formats load in `hydrate()` for actions, including editing and opening
  links. File-count metadata is computed from the full file list; labels use
  `1 file` and `N files`.
- `max_clip_mb = 16` is in newly generated config.toml; existing configs use
  that default when absent. Values 1–128 are accepted, in MiB. Admission counts
  full text, distinct primary payload, thumbnail and all original format bytes;
  exceeding the cap logs a diagnostic and skips the entire write, including
  recopy updates. Oversized edits fail without modifying the stored clip.
  The 128 MiB per-format read guard remains an allocation safeguard.
- Context menus start at the left inset, leaving room for the submenu inside
  the 480px popup. Real desktop verification opened Paste as to the right,
  inspected all four transform items and checked Escape dismissal. The new
  `screenshots/paste-as.png` is an unmodified GPUI window capture from an
  isolated `%TEMP%\clipdeck-m2-review` fixture profile.

Reproducible size measurement: run
`cargo test v1_migration_preserves_content_and_reclaims_space -- --nocapture`.
The test creates an on-disk v1 database from `src/store/v1.sql` (the previous
schema and triggers), inserts one 5 MiB ASCII text clip (`x` repeated 5,242,880
times), its identical payload, a two-byte thumbnail and a three-byte format
fixture, then measures the database file before and after migration/vacuum and
WAL checkpoint. Result: **26,361,856 bytes (25.14 MiB) → 5,427,200 bytes
(5.18 MiB), 79.4% smaller**. This fixture's FTS content differs from the review's
31.5 MB example; the before/after comparison uses the same clip and SQLite build.
Tests also verify migration rollback/binary preservation, hash recopy promotion,
kind separation, UTF-8 search bounds, cap enforcement including rich formats,
and preview-only listing followed by exact hydration.

Local branch `Nour/m2`, Windows x64, Rust 1.99.1. No remote operations.
The implementation is present; Paint-specific acceptance remains blocked by
the host's missing Paint installation. WordPad is also absent; Word was used
for the explicitly permitted rich-text alternative.

## Build and test

```powershell
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build --release
# Stop Clipdeck first; this test writes the real clipboard:
cargo test live_listener_privacy_formats_and_own_marker -- --ignored --test-threads=1
```

The ordinary suite covers schema migration/version rejection, bundled FTS5,
retention and unpinning, dedupe promotion, literal/Unicode substring search,
privacy/own-write rejection, all five stored type round-trips, format blobs,
edits/deletion/reopening, type detection, DIB decoding, clamp math (including
negative monitor coordinates and scaled sizes), config fallback, transforms,
local calendar sections, and hotkey repeat suppression. Ordinary tests leave
the system clipboard alone. The opt-in listener test covers all three Windows
privacy formats and the process-specific own-write marker.

Final checks: formatting and Clippy with warnings denied are clean; **12 unit
tests pass, 1 desktop-mutating test is ignored by default**. The ignored test
was also run explicitly and passed.

## Desktop verification

Verified 2026-11-03/04. The browser host connector was unavailable. The mockup
was rendered with an isolated Chrome headless profile and inspected before
implementation. Real desktop checks used the local Orca CLI, targeted Win32
input/style/bounds probes, screen captures, and a separate Chrome profile with
the repository's `scripts/paste-target.html`. These are real GPUI captures,
not screenshots of the HTML mockup.

Fixture runs used `%TEMP%\clipdeck-m2-acceptance` as the child process's
`APPDATA`, keeping generated bulk history separate from the normal app data.
Native clipboard fixtures supplied Unicode text, RTF, CF_DIB and CF_HDROP.
The database was inspected independently using Python's SQLite module.

| Item | Result | How checked |
| --- | --- | --- |
| Text into Notepad | PASS | Selected history with the hotkey/search/Enter path. Read `Clipdeck daily-use text fixture` from the target's actual Text editor accessibility value. |
| Text into Chrome | PASS | Focused the local textarea, opened Clipdeck, searched and pressed Enter. Read the textarea's DOM value through the dedicated local tab: exact fixture text. |
| Rich text into WordPad | FAIL — unavailable | No WordPad executable/install is present. The allowed Word alternative is checked below. |
| Rich text into Word | PASS | Captured Unicode + RTF, pasted through Clipdeck into a fresh Word document. Visual inspection showed only the word `bold` in bold. |
| Image into Paint | FAIL — unavailable | `mspaint.exe` is absent and no Paint AppX package is installed. Capture produced a PNG and thumbnail; real DIB image paste into Word was exercised as the closest available target. This does not certify Paint. |
| Files into Explorer | PASS | Pasted a historical CF_HDROP clip into a dedicated empty folder. Independently read the resulting `clipdeck-file-fixture.txt` and matched its contents. |
| No taskbar entry | PASS | Native `GetWindowLongPtrW(GWL_EXSTYLE)` returned `0x200188`: TOOLWINDOW set, APPWINDOW clear, while `IsWindowVisible` was true. Release uses the Windows subsystem to avoid a console taskbar entry. This check does not use automation inventory. |
| Popup at all four screen edges | PASS | Current monitor work area `(0,0)-(3440,1392)`. Cursor corners produced popup rectangles `(0,0,480,480)`, `(2959,0,3439,480)`, `(0,911,480,1391)`, `(2959,911,3439,1391)`. Asserted containment with native window rectangles. |
| Persistence across restart | PASS | Quit and reopened against the same fixture DB; the pinned clip and newest clips survived. Store tests separately close and reopen an on-disk DB. |
| Retention | PASS | Changed fixture config to `history_size=2`, restarted, then queried `GROUP BY pinned ORDER BY pinned`: exactly two unpinned and one pinned. Unit test additionally checks re-copy promotion and expiry after unpinning. |
| Tray Pause | PASS | Clicked the actual notification-area menu. Popup displayed `Recording paused`. A unique paused fixture was absent from SQLite; after Resume, the same fixture appeared. See `paused.png`. |
| Tray Quit | PASS | Clicked the actual native Quit menu item. Asserted the app's native windows disappeared and confirmed the process exited. |
| Tray Settings | PASS | Clicked Settings; a Cursor window titled `config.toml` opened. Shell association is used first, with Notepad fallback when no editor association can be launched. |
| Tray Open | PASS | Selected native Open and verified the existing GPUI popup became visible. |
| Single instance | PASS | Launched a second executable with a five-second timeout. It exited successfully and made the original process's popup visible. |
| Excluded clip never stored | PASS | Wrote a unique Unicode fixture carrying `ExcludeClipboardContentFromMonitorProcessing`. Queried the live fixture DB for that exact text: zero rows. The opt-in test also checks the other exclusion formats. |
| 5,000-clip scroll | PASS | `--debug-seed` produced 5,003 rows including existing fixtures. Scrolled the real virtualized GPUI list repeatedly; screenshots show different rendered ranges. A 100-wheel-event input run took 1.44 s including intentional input pacing and capture delay; this is not an FPS benchmark. |
| Hotkey opens on press | PASS | Held the opening chord while asserting the popup was visible, then released it. A latch prevents repeat messages from replacing the original paste target. |
| Paste worker | PASS | Clipboard write, delay, focus restore, release wait and SendInput execute in a background thread. Notepad/Chrome/Word/Explorer checks exercised this path. No UI-thread sleep remains. |
| Inline pin/unpin/delete and editor | PASS | Clicked row buttons and asserted SQLite pin transitions. Opened Edit, saved a two-line replacement with Ctrl+S, checked exact stored text, then clicked Delete and checked row absence. |
| Filter keyboard dispatch | PASS | Tab selected Links, and producing Ctrl+3 using this host's AZERTY layout selected Images. Captures are `tab-filter.png` and `images-filter.png`. |
| Image Save As | PASS | Selected Save as file from the real image submenu, accepted the native Save As dialog, then checked the saved PNG signature and decoded dimensions (320×180). |
| Light/dark Windows app theme | PASS | Temporarily changed `AppsUseLightTheme`, broadcast `ImmersiveColorSet`, inspected the real popup in both modes, then restored the original value (0). |

The four requested screenshots are `popup-light.png`, `popup-dark.png`,
`context-menu.png` and `paste-as.png` under `docs/screenshots/`. Additional
captures show Pause, image actions and the seeded list.

## Idle release measurement

Hidden final release popup after the 5,000-clip run, 32 logical processors.
A 20.0428 s sample used 0.046875 CPU seconds: **0.234% of one logical CPU**,
or **0.00731% of this host**. Working set **112.820 MiB**; private committed
memory **94.727 MiB**. One sample on a shared desktop, not a comparative
benchmark. An earlier six-clip sample used the same CPU time with a 76.016 MiB
working set and 88.875 MiB private memory.

## Implementation choices and limits

- `history.db` and `config.toml` live under `%APPDATA%\Clipdeck`. Schema version
  is `PRAGMA user_version=2`; unknown future versions are rejected. Bundled
  SQLite verified FTS5 with the trigram tokenizer. Literal wildcard searches
  and short queries retain substring semantics; LIKE is the fallback when
  FTS5 creation is unavailable.
- Re-copy identity is type + primary payload. Text re-copies replace rich
  format blobs, preserve pin/creation time and promote recency. Images use
  canonical PNG payloads; original DIB/DIBV5 is retained for paste. File lists
  retain HDROP bytes. Source names come from the clipboard owner's executable
  FileDescription, with exe-name/foreground fallbacks.
- Payloads and full clipboard formats are hydrated only for actions. The list
  caches metadata/thumbnails and renders visible rows with GPUI `list` and
  variable-height section headers. Capture is bounded at 128 MiB per format;
  image decoding is bounded at 32 million pixels and 16,384 pixels per axis.
  Uncommon unsupported DIB headers are rejected with a diagnostic.
- The pinned GPUI crates are unmodified. The one additional dependency beyond
  the permitted list is **gpui-kit-assets =0.9.0**, already transitive in the
  lockfile. It supplies the component menu chevrons and application icons via
  its existing asset source; no SVG asset fork or copied crate is needed.
  `image =0.25.10` and TOML reuse already resolved versions. No vendoring or
  `[patch]` overrides are used.
- GPUI input owns some key bindings (notably Tab), so filter cycling captures
  the input actions. Keyboard shortcuts use the active keyboard layout: on
  AZERTY, producing a digit may require Shift. Menus and editor fields retain
  their own focus behavior. Reopening from the app itself preserves the last
  external paste target.
- Editing is explicitly plain text and clears the clip's rich formatting.
  Empty-search digit shortcuts take priority over starting a numeric search;
  use the type filters or type a nonnumeric part of the preview to find such
  clips. Image/file plain-text actions are disabled/omitted.
- Windows monitor/DPI APIs are used for placement; real mixed-DPI hardware was
  not available. Unit tests cover scaled and negative-origin geometry. Elevated
  targets, secure desktops and every Windows focus-lock policy remain outside
  the verified matrix. Refused focus/injection is logged rather than sent to
  an unverified foreground window.

## Run and seed

```powershell
cargo run --release
# Hotkey Ctrl+Shift+V; tray menu supplies Open, Pause, Settings and Quit.
# Changes to config.toml apply on restart.

# In a separate PowerShell, with Clipdeck stopped, isolate generated fixtures:
$env:APPDATA = Join-Path $env:TEMP 'clipdeck-m2-acceptance'
New-Item -ItemType Directory -Force "$env:APPDATA\Clipdeck" | Out-Null
@'
hotkey = 'Ctrl+Shift+V'
history_size = 6000
paste_delay_ms = 80
'@ | Set-Content "$env:APPDATA\Clipdeck\config.toml"
cargo run --release -- --debug-seed
```

Milestone 3 remains Ditto import, collections, sensitive-text masking/expiry,
settings UI and autostart. Installer and sync remain non-goals. Paint acceptance
and real mixed-DPI verification are outstanding validation work, not claims
of milestone 3 functionality.
