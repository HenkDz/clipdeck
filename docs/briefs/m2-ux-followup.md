Follow-up for the current UX work on Nour/m2-ux. Do these after your current task, in the same branch.

1. Screenshots are not recorded (bug)
   Cause: the size cap counts the raw bitmap. A full-screen screenshot on the owner's 3440×1392 monitor is a 32-bit DIB of ~18.3 MiB. Store admission adds DIB + PNG + thumbnail, so it exceeds the 16 MiB default and the clip is silently skipped. Large region captures hit it too.
   Fix:
   - Store images as PNG only. When the clipboard offers the registered "PNG" format (Snipping Tool does), use it directly: it keeps alpha and skips re-encoding. Otherwise encode from CF_DIBV5/CF_DIB as today.
   - Don't keep the raw DIB/DIBV5 bytes in `formats`. On paste, write the "PNG" format plus CF_DIB generated from the PNG, so both modern and old apps accept it.
   - The cap counts the stored PNG + thumbnail, not the source bitmap.
   - Schema v3 migration: convert existing image rows (drop the stored DIB blobs and keep the PNG), vacuum. Test the migration on a v2 database.
   - Tests: a 3440×1392 32-bit screenshot fixture is admitted under the default cap. Paste of a stored PNG produces a valid CF_DIB.
   - Desktop check: Win+Shift+S full screen and a region both appear in the list, and paste into Word or Chrome.
   - Also log skipped clips somewhere the owner can see. The release build has no console, so add a "Skipped N large clips" line in Clip details or the popup footer, or write a log file at %APPDATA%\Clipdeck\clipdeck.log.

2. Re-copied indicator (like Ditto)
   When a clip that's already in the history is copied again, Ditto marks the row with a colored bar on its left edge.
   - Add a `copies` count (schema v3, same migration). Re-copying increments it and promotes the clip, as today.
   - Rows with copies > 1 show a 3px accent bar on the left edge, in the theme's accent color, plus "copied N×" in the meta line.
   - The clip that's currently on the system clipboard gets a subtle marker too, e.g. a small dot. Track it from the last captured clip, or from our own last write.

3. "Open file location" (owner's request)
   - File clips: Show in folder opens Explorer with the file selected (SHOpenFolderAndSelectItems). Multiple files: their common folder, all selected if they share one.
   - Image clips carry pixels only; the clipboard doesn't say where the screenshot tool saved them. Resolve the file in this order, and remember the result on the clip:
     a. The clipboard itself names a file (CF_HDROP or the "FileNameW" format alongside the image): use it.
     b. ShareX (the owner's screenshot tool). When the clipboard owner process is ShareX.exe, read ShareX's own history. The database is `<Documents>\ShareX\History.db`, an SQLite table `History(FileName, FilePath, DateTime ISO-8601 with offset, Type, Tags JSON)`. On this machine Documents is OneDrive-redirected (C:\Users\nooro\OneDrive\Documents), so resolve it with the Known Folder API, not a hard-coded path. Open it read-only; ShareX may hold it open, so on SQLITE_BUSY retry briefly or read a temp copy. Take the newest `Type='Image'` row within ±10 s of the clip's capture time whose file exists. ShareX writes the file and history right around the clipboard copy, so do the lookup in the background a few seconds after capture, and again on demand.
        Bonus: the row's Tags JSON has {"WindowTitle","ProcessName"} of what was captured. Show it in the meta line, e.g. "Screenshot of Ditto · ShareX · 2 min ago".
        Also honor ShareX's ApplicationConfig.json (`UseCustomScreenshotsPath`/`CustomScreenshotsPath`) only if History.db is missing.
     c. Windows Snipping Tool: a PNG in %USERPROFILE%\Pictures\Screenshots (Known Folder) created within ±5 s with the same dimensions.
     d. Otherwise save the image to %USERPROFILE%\Pictures\Clipdeck\<yyyy-mm-dd_hh-mm-ss>.png on first request, then reveal it.
   - Verify (b) on the owner's machine against real ShareX captures already in the history: the newest entries are in OneDrive\Documents\ShareX\Screenshots\2026-11.
   - Text clips: no Show in folder item.
   - Put it in the right-click menu below Preview, and under More if the menu gets long.

Evidence as before: fmt, clippy -D warnings and tests clean. Desktop rows appended to docs/MILESTONE2_NOTES.md. Screenshots of the indicator and of Show in folder for a screenshot clip. Small commits, local only. Start your final release build for the owner when done.
