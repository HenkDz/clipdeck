Clipdeck design pass and the owner's second round of UX asks. Branch Nour/m2-polish, based on Nour/m2-ux (milestone 2 + fixes + UX follow-up, all unmerged). Read docs/SPEC.md, docs/mockup.html (the visual target), docs/MILESTONE2_NOTES.md and src/ui/* first. Rust 1.99.1, the pinned crates in Cargo.toml; never vendor or patch, no new dependencies without asking.

You were picked for this because the previous workers wrote solid code but weak visual design. The owner's verdict on the current popup: "we really really need to work on UI and spacing". Judge every screen by eye against the mockup, not by "it compiles".

## 1. Design pass (the main job)

Bring every surface up to the mockup's quality, light and dark:
- One spacing scale (4/8/12/16 px) and one type scale (row title 13–14px, meta 12px, section labels 11px uppercase-tracking or small muted). No ad-hoc paddings.
- Header: search field with a leading search icon, placeholder "Search your clipboard", a real close button (icon, not a "×" label) aligned to the field's height. Filter chips: consistent height, selected chip filled, others quiet; add a **Files** chip (All, Links, Images, Text, Files, Pinned).
- Rows: clear title/meta hierarchy, aligned leading column (same width for icons and thumbnails), thumbnails with rounded corners and a hairline border, inline actions vertically centered and not crowding the title. Section headers ("Today", "Earlier") lighter than rows, not a bordered row.
- Re-copied bar (copies > 1) and the "on clipboard now" dot: keep them, make them look intentional (bar full row height, inset from the top/bottom radius if any).
- Image rows: title is the screenshot title when known ("Screenshot of Ditto"), otherwise "Image"; dimensions go in meta.
- Footer: shortcut hints as small key caps (`Enter` paste), evenly spaced, one line, truncating gracefully.
- Preview, Edit and Details views are currently raw text (e.g. the "Close preview · Space / Esc" string). Give each a proper header bar (back arrow, title, actions as buttons) and a body with padding; image preview fit-to-panel on a checkerboard or neutral backdrop with dimensions/size under it.
- Context menu and "Paste as…" submenu: icons aligned, separators between the SPEC's groups, shortcut hints right-aligned.
- "Skipped N large clips" and errors: a subtle status line, not loose text.
Before/after screenshots of every surface in light and dark go in docs/screenshots/polish/.

## 2. Copying several files (bug + UX)

Bug: copying 5 files (from Orca's file tree) recorded nothing new; the top row was an older screenshot marked "copied 2×". `platform/capture.rs` checks for an image first, and the source also put a bitmap of one file on the clipboard, so the copy was stored as that image and deduped onto the existing screenshot.
Fix: CF_HDROP wins whenever it lists more than one file, or its single file isn't an image. Only a single image file + bitmap stays an image clip (with `file_path`), as today. Unit-test the routing.

UX for a multi-file clip (one row per copy, never one row per file):
- Leading thumbnail: a small mosaic of the first files — up to 3 thumbnails fanned/stacked or a 2×2 grid, shell thumbnails for images and videos, type icons for the rest (doc, PDF, archive, code, folder) — with a count badge when there are more. Generate it off the UI thread at capture and store it as the clip thumbnail.
- Title: one file → its file name. Several → "5 files" followed by the names, truncated ("ui.png, notes.pdf, demo.mp4 +2").
- Meta: type breakdown + source + time ("3 images · 1 video · 1 PDF · Orca · 2 min ago").
- Preview (Space): a gallery grid of every file (thumbnail + name; missing files dimmed with "Missing"). Arrows move, Enter pastes only that file, Space opens a single file's large preview, Esc goes back.
- Paste pastes all files. "Paste as…" adds: Paths as text, File names only, Only the images (N) when the set is mixed, and a "One file ▸" submenu.
- Filters: a files clip shows under Files; under Images too when every file is an image.

## 3. Paste indicator

Replace the green "✓ pasted N×" text. Track pastes per format (normal paste vs plain text; schema migration with a test). Show it on the matching inline action: that paste icon is tinted with the theme's success green, with a tiny "×N" under the icon when N ≥ 2. When the row isn't selected and has pastes, show the same green icon (+ ×N) in the trailing slot, quiet enough not to compete with the title.

## 4. Compact list, resizable popup, keep on top

- Density: Comfortable (current two-line rows with thumbnails) and Compact (one line ~30px: 16px type icon or tiny thumbnail, text, short time "2m" right-aligned, indicators kept). Toggle from a header menu button and Ctrl+D; persist as `density` in config.toml.
- Resize: the popup can be resized from its edges and bottom-right corner (frameless window: handle WM_NCHITTEST in the platform layer if GPUI doesn't). Min ~360×320. Persist the size in config.toml and reuse it on every open, clamped to the monitor's work area.
- Keep open (always on top): a pin-window toggle in the header. The popup is already topmost; this mode stops it from hiding on focus loss and after paste, lets the user drag it by the header, and remembers its position. Esc/close still hides it. Persist the toggle. Paste in this mode goes to the last real app (foreground::last_app()), like tray mode.

## 5. Actions on hover (owner follow-up)

Show a row's inline actions on mouse hover, not only on the selected row. Hover gets a light background tint; the keyboard selection keeps its stronger style and does not move with the mouse. The actions are drawn over the right end of the row with a short fade from the row background, so the text never reflows or jumps. The same applies in compact mode and while the popup is inactive (tray and keep-on-top modes). On hover, the paste count icon becomes the paste button with its green tint and ×N.

## Evidence

fmt, clippy -D warnings and tests clean. Unit tests for file routing, the type breakdown, paste counters and migration, density/size config round-trips. Desktop rows appended to docs/MILESTONE2_NOTES.md with PASS/FAIL and how each was checked (multi-file copy from Explorer with mixed types, paste all/one file, resize + reopen, keep-on-top + paste into Notepad). Screenshots as above. Small commits, local only, no push.

The owner runs a build from another worktree; single-instance means your launches only signal it. You may stop it (taskkill /IM clipdeck.exe) to verify. When done, start your final release build so the owner can test it, and report.
