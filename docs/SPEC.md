# Clipdeck — product spec

A keyboard-first clipboard history for Windows. Search-first popup, clips grouped by type,
the common actions on the row itself, and a short right-click menu. Built in Rust on GPUI.

Ditto (sabrogden/Ditto, GPL-3.0, C++/MFC) is the reference for Windows clipboard edge cases.
Read it to understand behavior; never copy its code — Clipdeck is not GPL.

## UX

### Popup

- Opens at the mouse cursor on the global hotkey (default `Ctrl+Shift+V`, configurable later).
  Frameless, always on top, no taskbar entry. Closes on `Esc` or focus loss.
- Focus lands in the search box. Typing filters the list immediately (substring, case-insensitive).
- Filter chips under the search: All, Links, Images, Text, Pinned.
- Sections: Pinned (top), then Today / Earlier.
- Each row: type icon (link, email, text, image thumbnail, file list, sensitive), one-line preview,
  meta line (source app · relative time). Images show a thumbnail plus dimensions.
- Selected row shows inline actions: paste as plain text, pin, delete.
- Footer shows the shortcuts: `Enter` paste · `Shift+Enter` plain text · `Ctrl+P` pin · `Del` delete.
- `1`–`9` paste the nth visible clip when the search box is empty.

### Right-click menu (8 items, grouped by task)

1. Paste · Paste as plain text · Paste as… (submenu, options depend on clip type)
2. Pin · Add to collection · Edit
3. Delete
4. More (submenu: Properties, Export, and everything advanced)

"Paste as…" for text: UPPERCASE, lowercase, Trim spaces, One line. Images: Save as file.
Links: Open.

### Sensitive clips

- Never record a clip whose clipboard carries `ExcludeClipboardContentFromMonitorProcessing`,
  or `CanIncludeInClipboardHistory` = 0, or `Clipboard Viewer Ignore`. Password managers set these.
- Later: heuristic masking of password-like text with auto-expiry.

## Architecture

| Concern | Choice |
|---|---|
| UI | `gpui-component` 0.9.0 (pins `gpui-pre` =0.3.11; depend on that, aliased as `gpui`) |
| Clipboard listener | `AddClipboardFormatListener` on a hidden message-only window (`windows` crate) |
| Global hotkey | `global-hotkey` crate |
| Paste-back | Remember the foreground HWND before showing the popup; on paste, write the clip to the clipboard, hide the popup, restore focus, send Ctrl+V with `SendInput` |
| Storage | SQLite + FTS5 via `rusqlite` (bundled) — milestone 2 |
| Tray | `tray-icon` — milestone 2 |
| Import | Read Ditto's `Ditto.db` — milestone 3 |

Keep the Win32 code behind a small `platform` module so the UI layer never touches raw handles.

## Milestones

1. **Spike** — prove the risky Windows parts with a minimal UI (see below).
2. **Real app** — SQLite history, images and file lists, tray icon, pinning, filter chips,
   right-click menu, settings for hotkey and history size.
3. **Polish** — Ditto import, paste-as transforms, collections, sensitive-text masking.

### Spike acceptance

- Copying text in any app adds it to an in-memory history (newest first, de-duplicated).
- Clips flagged by the exclusion formats above are not recorded.
- The hotkey opens the popup at the cursor with focus in the search box; typing filters.
- Arrow keys move the selection; `Enter` pastes the selected clip into the window that was
  focused before the popup opened (verified with Notepad and a browser text field); `Esc` closes.
- Pasting a clip does not re-record it as a new clip at the top.
- The app idles near 0% CPU.
