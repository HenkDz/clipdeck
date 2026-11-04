# Clipdeck milestone 2

## Settings window (2026-11-04)

Settings is now a view in the popup, opened from the header menu
(Settings), Ctrl+, in the list, or the tray's Settings item, which used to
open config.toml in an editor and now opens the popup on Settings.
Every change applies immediately and is written to config.toml in place;
nothing needs a restart and nobody needs to edit the file.

- **Open Clipdeck** shortcut: click the field and press a chord (Ctrl, Alt
  or Win plus a key; Esc cancels). The global hotkey is re-registered live;
  if another app owns the chord, the old one stays and an error explains it.
- **List density**: Comfortable / Compact segmented control.
- **Keep open on top**: switch.
- **Clips to keep** (100 to 100,000), **Largest clip** (1 to 128 MB) and
  **Paste delay** (0 to 1,000 ms) use − / + steppers over sensible presets.
  Lowering the history below the number of unpinned clips stored shows
  "Keeping N removes the M oldest unpinned clips" with Cancel / Remove;
  nothing is deleted until Remove.
- Pause stays in the tray, where its checkmark lives.

Tests: preset stepping and snapping, chord → shortcut conversion and
validation, live history-limit and size-cap changes in the store.
`cargo fmt`, Clippy with warnings denied and `cargo test` (**48 passed,
2 ignored**) pass.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Change shortcut | PASS | Recorded Ctrl+Alt+B: it opened the popup, Ctrl+Shift+V no longer did, config.toml said `Ctrl+Alt+B`. Recorded Ctrl+Shift+V back. |
| Density, keep open, largest clip, clips to keep | PASS | Each control changed config.toml (`compact`, `keep_open = true`, `32`, `500`) and back again. |
| History shrink confirmation | PASS | With 1,000 unpinned clips (`--debug-seed` fixture), − showed the prompt and left the history at 1,000; Cancel kept it; Remove left exactly 500. |
| Fits the default 480×520 popup | PASS | `after-settings-dark.png`, `-light.png`, `-recording-dark.png`, `-confirm-dark.png`. |
| Tray Settings item | Not desktop-checked | The tray menu was not driven; it sends Open then ShowSettings through the same paths as the hotkey. |

## ShareX copies failing while Clipdeck runs (2026-11-04)

Owner report: annotated ShareX screenshots reached neither Clipdeck nor
Ditto, and could not be pasted. Cause: Clipdeck read the clipboard as soon
as it changed and kept it open while decoding the image and building the
thumbnail. .NET's `Clipboard.SetDataObject(data, copy: true, …)`, which
ShareX uses, publishes the data and then reopens the clipboard to render
it; with Clipdeck still holding it, the retries ran out and the copy failed,
so nothing was left on the clipboard for anyone.

Fix: clipboard reads wait until updates have been quiet for 150 ms (bursts
become one read), and while the clipboard is open only bytes are copied;
PNG/DIB decoding and the thumbnail run afterwards on the worker thread.

| Check | Before | After | How |
| --- | --- | --- | --- |
| ShareX-style copy of a 3440×1392 detailed PNG | FAIL 3/3 ("Requested Clipboard operation did not succeed", ~550 ms) | PASS 5/5 (~68 ms) | PowerShell STA script: `DataObject` with Bitmap + PNG stream, `SetDataObject($do, $true, 5, 100)`, against the fixture profile. With Clipdeck stopped the same script passed 3/3. |
| Image still recorded | n/a | PASS | Stored as a 3440×1392 image clip; one copy raised `copies` by exactly one. |
| Text capture, privacy formats, own writes | PASS | PASS | Plain text recorded; opt-in live clipboard test rerun and passed. |

The ShareX image editor itself was not driven; the script uses the same
.NET clipboard call with a ShareX-like data object.

## Per-file context menu in the file preview (2026-11-04)

Right-clicking a file in the gallery, or the large single-file preview,
selects it and opens a menu for that file only: Paste this file (Enter),
Copy this file, Paste path as text, Paste file name, then Preview (grid
only), Open in default app and Show in folder. Missing files keep only
the two text entries enabled. Pastes count on the clip as before (file
subsets as normal pastes, path/name as plain text); Copy leaves the popup
open. Opening and Explorer selection run off the UI thread; failures show
in the status line. Focus returns to the gallery when the menu closes, so
arrows, Space and Enter keep working.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Paste one file from its menu | PASS | Explorer folder holding only `placeholder.txt`; gallery of the 9-file clip, right-click photo.jpg, Paste this file: the folder then held `photo.jpg` and `placeholder.txt` only. |
| Paste file name | PASS | Right-click ditto.png, Paste file name into a Tk text box: its text was exactly `ditto.png`. |
| Copy this file | PASS | Right-click the `assets` folder tile, Copy this file: the clipboard's CF_HDROP listed only `...\assets`; the popup stayed open and → / Space still worked. |
| Menu on the large preview | PASS | `after-file-menu-dark.png`; grid menu in `after-gallery-menu-dark.png` / `-light.png`. |

An Explorer quirk seen while testing: after the popup closes, an Explorer
window showing an **empty** folder ignores Ctrl+V entirely, including one
typed directly to it, because focus does not return to its empty file view.
With any item in the folder, pastes land normally.

## Design pass and second UX round (2026-11-04)

Local branch `Nour/m2-polish` on top of `Nour/m2-ux`; no push. Rust 1.99.1,
pinned crates unchanged, no new dependencies, no vendoring or patches.
Screenshots are in `screenshots/polish/`: `before-*` from the previous
release build (same HEAD as `m2-ux`) and `after-*` from this build, each in
light and dark. All captures are real GPUI windows from an isolated fixture
profile (`APPDATA=%TEMP%\clipdeck-polish`); the fixture database was given
realistic source names and a curated order with SQL, nothing else.

### Design

- One spacing scale (4/8/12/16) and one type scale (13 titles, 12 meta,
  11 labels and key caps) in `ui/theme.rs`, with the mockup's palette for
  light and dark. Overlay colors are pre-blended so rings and fades match.
- Header: search field with a leading icon and "Search your clipboard",
  then keep-open, view-options and close icon buttons at the field's 32 px
  height. Chips All, Links, Images, Text, Files, Pinned: 24 px, selected
  filled, others outlined and quiet. Section labels are small muted text.
- Rows: 52 px comfortable rows; a 44 px leading column shared by type tiles,
  thumbnails (rounded, hairline border) and file mosaics. Title 13 px, meta
  12 px muted. Re-copied bar (3 px, inset 10 px from the row radius) and the
  "on clipboard now" dot (on the leading tile's corner, ringed in the row
  color, with a tooltip). Image rows: "Screenshot of X" or "Image", with
  dimensions in the meta line.
- Inline actions (owner item 5): drawn over the right end of the row with a
  24 px fade from the row color, so text never reflows. They show on the
  selected row and on hover; hover gets a light tint, the keyboard selection
  keeps its stronger color and does not follow the mouse. This works in
  compact mode and while the popup is inactive (tray and keep-open).
- Footer: key caps with labels, spread on one line; hints that do not fit
  the current width are dropped from the end. Each view has its own hints.
- Preview, Edit, Details, the file gallery and single-file preview share a
  48 px header bar (back, title, quiet detail, buttons; primary filled) and a
  padded body. Images fit a neutral well with size/dimensions under them.
- Context menu: an icon per command, SPEC groups (Preview/Show/Open, Paste,
  Pin/Edit, Delete, More) separated, key caps right-aligned, disabled
  entries muted. It opens at the left inset so submenus open rightwards.
- Status line above the footer for errors (danger icon, click to dismiss),
  "Skipped N large clips" and "Recording paused".

### Copying several files

- Bug fixed: `files::route` makes CF_HDROP win unless it names exactly one
  image file and a bitmap came with it. Covered by
  `routes_drop_lists_ahead_of_bitmaps`. The original report reproduced on
  the old build: a 5-file copy with a bitmap became an image clip.
- At capture (off the UI thread) each file gets a kind code (folder checks
  for the first 1,000) and the clip gets an 88×64 mosaic: up to four cells of
  Shell thumbnails (images, videos) or Shell type icons; more than four files
  leave the last cell for a "+N" badge drawn by the row.
- Title: one file shows its name; several show "5 files" plus
  "ui.png, notes.pdf, demo.mp4 +2". Meta: "3 images · 1 video · 1 PDF ·
  Orca · 2 min ago" (three groups at most, the rest folded into "N more").
- Space opens a gallery of every file (Shell thumbnails, names, missing
  files dimmed with "Missing"); arrows move, Enter pastes that file only,
  Space opens its large preview, Esc goes back. A one-file clip opens
  straight into the large preview.
- Paste pastes all files. Paste as… adds Paths as text, File names only,
  Only the images (N) for mixed sets, and a One file ▸ submenu (first 40).
  Subsets are written as CF_HDROP with Preferred DropEffect = copy.
- File clips show under Files, and under Images when every file is an image.
  Lists now return the full path list for file clips (other kinds remain
  truncated to 512 characters).

### Paste indicator

Schema v5 adds `plain_pastes` (and `file_kinds`); v4 counts stay as normal
pastes since their format was not recorded. Normal paste and plain-text
paste (including text transforms, Paths as text and File names only) count
separately. The matching inline action is tinted green with "×N" under the
icon from the second paste; unselected rows show the same quiet icons in the
trailing slot. The "✓ pasted N×" text is gone.

### Density, resizing, keep open

- Comfortable and Compact (30 px rows, 16 px icon or tiny thumbnail,
  right-aligned short time) from the header menu or Ctrl+D; saved as
  `density` in config.toml.
- The frameless popup resizes from its edges and a wider bottom-right grip
  (own WM_NCHITTEST), minimum 360×320 (WM_GETMINMAXINFO). Size is saved as
  `width`/`height` and reused at every open, clamped to the work area.
- Keep open on top (header toggle, saved as `keep_open`): no hiding on focus
  loss or after paste, drag by the end of the chip row (grip shown), position
  saved as `x`/`y` and reused, pastes go to `foreground::last_app()`. Esc and
  close still hide it.
- Settings are written by replacing top-level keys in place, so comments and
  other keys survive (`set_key`, tested).
- Placement is posted to the window's own message loop. A SetWindowPos made
  during a GPUI update delivered WM_SIZE while the window was borrowed, so
  GPUI kept laying out at the old size; GPUI's deferred first placement is
  also re-applied after activation. This removes the oversized first frame
  visible in `before-list-dark.png`.

### Checks

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
`cargo test`: **45 passed, 2 ignored**. The opt-in real clipboard test
(`live_listener_privacy_formats_and_own_marker`) was rerun and passes. New
tests: file routing, type breakdown, titles, kind codes, CF_HDROP layout,
mosaic cells, Shell alpha conversion, row titles/meta, filter membership,
v4→v5 migration, separate normal/plain counters and their reset on edit,
full file lists, density/size/keep/position config round-trips and
fallbacks, work-area fitting, resize hit-testing, file transforms, sizes.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Multi-file copy from Explorer, mixed types | PASS | In Explorer, Ctrl+A and Ctrl+C on 9 items (folder, zip, mp4, png ×2, jpg, rs, pdf, docx). SQLite shows one `files` clip with kinds `fzvicpidi`, a mosaic thumbnail and the CF_HDROP blob. Row: `after-explorer-9-files-dark.png`; gallery: `after-gallery-*.png`. |
| 5 files + bitmap (Orca tree case) | PASS | Fixture put CF_HDROP of 5 files plus a DIB of the first. Old build stored a 1280×720 image; this build stores a 5-file clip (`after-list-*.png`, "5 files ui.png, notes.pdf, demo.mp4 +2"). |
| Paste one file | PASS | Explorer focused on an empty folder, hotkey, Space on the 9-file clip, → to ditto.png, Enter. The folder then contained only `ditto.png`. |
| Paste all files | PASS | Same folder emptied, hotkey, Enter on the clip: all 9 items arrived, including `assets\logo.svg`. Clip `pastes` = 2. |
| Resize + reopen | PASS | Dragged the bottom-right grip: 600×600, then below the minimum: 360×320 (`after-resized-min-dark.png`), then 620×450. config.toml held each size; after hiding, the hotkey reopened at the cursor at 620×450. |
| Keep on top + paste into a text app | PASS (not Notepad) | Keep-open on, hotkey from the target, Enter: text pasted, popup stayed visible and the target kept focus. Clicking a row in the inactive popup and pressing Enter, or the inline plain-text button, pasted again; counters rose. Dragging by the chip-row end moved it; config saved x/y; hiding and pressing the hotkey elsewhere reopened at the saved spot. The target was a Tk text window of mine: Notepad merged my file into an existing Notepad window holding another unsaved document, so I did not type into Notepad. |
| Keep-open Shift+Enter | PASS with human timing | With a 60 ms gap between injected keys the plain paste arrives. A single zero-gap injected Shift+Enter chord was counted as sent but not inserted by the Tk box; normal mode pasted the same chord fine. |
| Hover actions while inactive | PASS | Kept-open popup behind the focused target; hovering rows showed the tint and overlay, the pasted row's icon green with ×2 (`after-inactive-hover-dark.png`). |
| Missing files, error status | PASS | A 3-file clip whose middle file was deleted: gallery shows it dimmed and "Missing"; Open in default app shows the error line (`after-status-error-dark.png`). |
| Light and dark | PASS | Every `after-*` surface captured in both; `AppsUseLightTheme` toggled and restored to 0. |
| Windows 10 fallback, mixed DPI | Not checked | Single 100%-scale Windows 11 monitor. |

Notepad note: the empty `keep-target.txt` tab I opened is still in that
existing Notepad window; nothing was typed there and its other tab was not
modified (one click landed in its text area).

## Open file location follow-up (2026-11-04)

The context menu now includes **Open file location** directly below Preview.
For file clips it opens Explorer with the first original file selected; a missing
file reports an error. For image/screenshot clips it exports the stored PNG to
the same `%TEMP%/Clipdeck-preview/<pid>/clip-<id>.png` used by Preview and
selects that file. SQLite remains the authoritative image storage; this folder
contains on-demand exports. The action is disabled for text/link/email clips.
Export, path checking and native Shell selection run off the UI thread.

No crate versions changed; the existing Windows dependency enables its
`Win32_UI_Shell_Common` feature for native folder selection.
`cargo fmt --check`, Clippy with warnings denied, all tests (**20 passed,
1 ignored**), and the release build pass. Tests verify exact image export
bytes, original file selection with spaces/commas/Unicode, first-file routing,
and errors for missing/empty file lists.

| Desktop item | Result | How checked |
| --- | --- | --- |
| File and screenshot Open file location | FAIL - desktop verification incomplete | Automation could not discover the release popup (`app_not_found`); no Explorer-selection PASS claimed. Path/export behavior is unit-tested. |

The rebuilt `m2-ux/target/release/clipdeck.exe` is left running for owner testing.

## Owner UX follow-up (2026-11-04)

Local worktree: `m2-ux`, branch `Nour/m2-ux`, based on `Nour/m2-fixes`
at `f3a97e8`. The owner's existing worktree/branch was not moved. No push.
Rust 1.99.1 and the existing crate versions; no vendoring or patches.

Popup uses DWM round corners and the theme border color. The client border
is used only when DWM corner preference is unavailable, with square corners.
The native resize inset is removed so content fills the window.

Tray opens now use a separate inactive mouse mode, WS_EX_NOACTIVATE,
SW_SHOWNOACTIVATE and MA_NOACTIVATE. The remembered app is restored immediately.
Row single-click selects; double-click and the inline Paste button paste.
Mouse paste checks the already-foreground target; keyboard paste retains
foreground restoration. Clicking search activates keyboard mode. The tray
toggles visibility, the header has an accessible close button, and the existing
foreground hook hides mouse mode when another application becomes foreground.

**Owner correction during verification:** Preview and Space open image/file
clips in their default application, instead of displaying media inside the
popup. Stored image clips are written unchanged to
`%TEMP%/Clipdeck-preview/<pid>/clip-<id>.png`. These temporary exports remain
available to the viewer; automatic cleanup is deferred. A file-list Preview
opens its first file. Text Preview remains full, read-only and scrollable;
Space/Esc or its close control dismiss it. Hydration/export/open work runs in
the background. New image/video file captures store a small Shell thumbnail;
old file rows acquire one when copied again. Missing files report an error.
There is no video decoder or in-popup media/duration panel after the correction.

Final source checks: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`:
**19 passed, 1 ignored**. The existing real-clipboard test was not rerun.
`cargo build --release` passed. One intermediate build encountered a running
executable lock; stopping that copy and rebuilding succeeded.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Before border, light/dark | PASS | Actual original release captures: `screenshots/ux-before-light.png`, `ux-before-dark.png`. |
| Single native border, light/dark | PASS | Actual release popup captures: `screenshots/ux-popup-light.png`, `ux-popup-dark.png`; content fills the 480px window. |
| Image/video file row thumbnail | PASS for video | Copied a generated two-second H.264 MP4 from Explorer. Actual captured row shows its frame in `screenshots/ux-video-row.png`. Image-file row was not separately checked. |
| External image Preview | FAIL - verification incomplete | Implemented owner correction; GUI verification was stopped by physical Escape before the final external viewer check. |
| External video Preview | FAIL - verification incomplete | Preview is first in the actual context menu. External player verification was stopped by physical Escape. |
| Text Preview and Space/Esc | FAIL - verification incomplete | Implemented; final text desktop check was not completed. |
| Chrome tray double-click at caret | FAIL - verification incomplete | Automation activates target windows before input, preventing reliable proof of uninterrupted inactive-popup focus. No PASS claimed. |
| Notepad tray double-click at caret | FAIL - verification incomplete | Same tool limitation; Notepad target was restored and inspected, but final tray paste was not certified. |
| Tray toggle, search mode, close, foreground dismissal | FAIL - verification incomplete | Implemented with unit-tested mode transitions; final desktop matrix was interrupted. |
| Hotkey activated path | PASS for opening | Actual Ctrl+Shift+V reopened/reset and placed the release popup. Final paste regression was not completed. |
| Windows 10 fallback | FAIL - unavailable | This host is Windows 11; fallback remains source-verified only. |

Desktop automation stopped when the physical Escape key interrupted Computer
Use. No further UI actions were attempted. The final release executable was
left running from `m2-ux/target/release/clipdeck.exe` for owner testing.
The earlier small in-popup media screenshots were discarded because they
describe behavior superseded by the owner's correction.

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

## Schema v3 and screenshot source follow-up (2026-11-04)

Local branch `Nour/m2-ux`; no push. The owner's v2 database was backed up
consistently to this worktree's ignored `target/history-before-v3.db` before
starting the migrated release. The live database reports schema v3.

- Capture prefers the registered PNG format and preserves its bytes/alpha.
  DIB-only capture still encodes PNG. Images store PNG plus thumbnail, without
  raw DIB/DIBV5 format blobs. Admission counts PNG plus thumbnail. Paste writes
  registered PNG plus a regenerated bottom-up 32-bit CF_DIB. The existing
  exclusion formats and own-write suppression remain in place.
- Transactional v3 migration preserves image PNGs, IDs, pin/history metadata,
  and non-image formats, removes image format blobs, adds copy/source metadata,
  and vacuums existing databases. Migration failure rolls back. The v2 disk
  fixture includes a 19,000,000-byte raw DIB blob and verifies reclaimed space.
- Re-copy increments persisted `copies` and promotes the clip. Rows show a
  3px theme-accent bar and `copied N×`. A clipboard-sequence-checked marker
  tracks the last captured or own-written clip. Oversized admission skips are
  visible as `Skipped N large clips` below the footer for the current run.
- Show in folder sits below Preview for file/image clips and is omitted for
  text. Files sharing a directory are all selected using native Shell PIDLs;
  different subfolders open their common parent. Files on unrelated drives
  report that no common folder exists.
- Images retain explicit CF_HDROP/FileNameW paths when supplied. Otherwise,
  ShareX captures consult its read-only History.db through Known Documents
  with a bounded SQLite busy timeout, newest existing image within ±10 seconds,
  and WindowTitle/ProcessName tags. A delayed background lookup runs three
  seconds after capture and lookup retries on demand. A remembered source is
  guarded by clip ID, timestamp and PNG bytes before updating metadata.
- If ShareX History.db is missing, its custom screenshot folder is consulted
  when enabled. The bounded directory-depth scan matches PNG dimensions and
  capture time. Snipping Tool fallback scans Known Pictures/Screenshots for
  matching PNG dimensions within ±5 seconds. If no source is found, first
  Preview/Show in folder exports to Known Pictures/Clipdeck with a local
  `yyyy-mm-dd_hh-mm-ss.png` name. Collisions use a clip-ID suffix and never
  overwrite a different file. The resolved path is remembered on the clip.
  This supersedes the temporary-image-export behavior described above.

Final checks: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
ordinary tests (**26 passed, 2 ignored**), and the release build pass.
Both ignored tests were also run explicitly and passed: real clipboard
privacy/own-write/PNG/DIB round-trip, and read-only real ShareX lookup.
Tests cover v2 migration/vacuum and rollback, the 3440×1392 32-bit capture
under the default 16 MiB stored-size cap, regenerated DIB pixel layout,
PNG alpha, timestamp offsets and boundaries, custom-path precedence,
Snipping dimensions, safe export collisions and common-folder routing.
Crate versions remain unchanged; no vendoring or patches.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Owner v2 history migration | PASS | Consistent backup before launch; independent SQLite query reports user_version=3 and existing image rows remain. |
| Large screenshot admission | PASS - fixture | A real 3440×1392 DIB buffer exceeds 16 MiB; the ordinary test records its PNG with zero skips and regenerates a valid same-size DIB. This is not a Snipping Tool UI check. |
| Registered PNG capture and paste formats | PASS | With the app stopped, the opt-in real clipboard test supplies PNG alongside a malformed DIB, verifies exact captured PNG bytes, writes the stored image and reads valid PNG/CF_DIB back. Privacy exclusions and own-write suppression pass. |
| Re-copy indicator | PASS | Copied the dedicated Notepad fixture twice. Actual popup shows accent bar and `copied 2×`; `screenshots/v3-copy-indicator.png`. |
| Current-clipboard marker | PASS - native test | Real clipboard own-write test asserts the current clip ID; the visual dot was not separately certified. |
| Real ShareX lookup | PASS | Read-only integration check resolves Known Documents to OneDrive/Documents and matches the real Clipdeck ShareX capture at Unix 1793777997 to `ShareX/Screenshots/2026-11/claude_zzQJyTesj6.png`. Newest ShareX entry also resolves. |
| Screenshot Show in folder | PASS for opening folder | Clicked Show in folder for the real ShareX screenshot. Independent foreground read shows `2026-11 - File Explorer`. Actual menu screenshot: `screenshots/v3-show-folder-menu.png`. Explorer selection screenshot/selection inspection was interrupted. |
| Win+Shift+S full screen and region | FAIL - verification incomplete | Computer Use was stopped by physical Escape before these UI captures. |
| Image paste into Word or Chrome | FAIL - verification incomplete | The Word launch helper failed with `accessibility window-opened handler did not become ready`; later GUI verification was stopped. Native clipboard format tests do not certify either target. |
| Snipping/custom-path/Pictures fallback and multi-file selection | FAIL - desktop verification incomplete | Source/path logic is unit-tested; these final GUI cases were not completed. |

No GUI actions were attempted after the physical Escape interruption.
The final release remains running from
`C:/Users/nooro/orca/workspaces/clipdeck/m2-ux/target/release/clipdeck.exe`
for owner testing.

## Copy selection and pasted indicators (2026-11-04)

Ctrl+C in the list/search view now copies the selected clip in its original
format to the system clipboard while keeping the popup open. It uses the
existing Copy command, moves the current-clipboard dot, and does not increment
copy or paste counts. The shortcut is shown in the footer and More / Copy to
clipboard. Ctrl+V retains its normal input behavior. Editor copy/paste remains
standard.

Schema v4 adds a persistent paste count with zero for existing clips.
After Clipdeck successfully writes the clipboard and sends the paste chord
to the verified target, the main store writer increments that clip's count.
Rows display a separate green `✓ pasted N×` badge. Copy counts and the blue
re-copy bar remain independent. Re-copying preserves the paste count and does
not reorder history as a side effect of marking a paste. Changed text edits
reset the pasted status; saving unchanged text preserves it. Delayed paste
completions match the original content hash, so editing/deleting/reusing an ID
cannot mark different content.

The badge counts paste chords **sent through Clipdeck**, including Enter,
double-click, inline Paste, plain text and transform actions. It does not
observe Ctrl+V/context-menu pastes made directly inside other applications,
and SendInput success cannot certify that a target accepted a particular format.

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test` (**28 passed, 2 ignored**) and the release build pass. New tests
cover v3-to-v4 migration, persistent separate copy/paste counts, unchanged and
changed edits, and stale completions after ID reuse. Existing opt-in clipboard
and ShareX tests were not rerun for this change.

| Desktop item | Result | How checked |
| --- | --- | --- |
| Ctrl+C selected clip | PASS | Filtered to the dedicated recopy fixture and invoked Ctrl+C. Read the full `Clipdeck recopy indicator fixture` from Windows Unicode clipboard; the search contained only `recopy indicator fixture`. Popup remained open; stored counters remained copies=2, pastes=0. |
| Paste count after actual paste | PASS | Opened Clipdeck for the dedicated `paste-counter-target.txt` Notepad window and pressed Enter on the fixture. Notepad's accessibility text contains the fixture followed by its original target text; independent SQLite read reports pastes=1, copies=2. |
| Pasted row badge | PASS | Reopened and filtered the actual release popup: green `✓ pasted 1×`, blue re-copy bar, `copied 2×`, clipboard dot and Ctrl+C footer are visible in `screenshots/v4-pasted-indicator.png`. |

A consistent pre-migration backup is in the ignored
`target/history-before-v4.db`. Final release remains running from this
worktree's `target/release/clipdeck.exe`. Changes are local only.
