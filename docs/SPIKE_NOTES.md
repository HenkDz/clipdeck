# Clipdeck milestone 1 — Windows spike

Verified on 2026-11-03, Windows x64, stable-x86_64-pc-windows-msvc,
`rustc 1.92.0` (spike run; now pinned to 1.99.1), MSVC/Windows SDK. Local branch
`Nour/spike`; no remote operations. Application-owned Rust files are below
300 lines each. Vendored upstream source retains its original layout/licenses.

## Run and verify

```powershell
cargo run
# Ctrl+Shift+V opens history; type to search, Up/Down, Enter, Escape.
cargo fmt --check
cargo build
cargo clippy -- -D warnings
cargo test
# Stop Clipdeck first: this opt-in test replaces the real clipboard.
cargo test live_listener_privacy_formats_and_own_marker -- --ignored --test-threads=1
```

The app starts without a visible window. History starts empty and is lost on
exit. Close the popup using its system close command, or terminate the process,
to exit; Escape/focus loss only hides it. No tray or persistent storage exists.
Diagnostics go to stderr and contain no clip contents.

## Acceptance results

| Acceptance | Result and evidence |
| --- | --- |
| Text capture, newest first, dedupe | PASS. Copied test text from Notepad, saw it in the real GPUI popup. Multiple distinct entries displayed. Unit test repeats `one, two, one` and asserts exactly `one, two`. |
| Exclusion formats | PASS with real Win32 clipboard/listener fixtures: `ExcludeClipboardContentFromMonitorProcessing` present, `Clipboard Viewer Ignore` present, and `CanIncludeInClipboardHistory` DWORD 0 emitted no clip. DWORD 1 and ordinary Unicode text emitted clips. This was a separate test process, not a password-manager integration test. |
| Hotkey, cursor, search focus, filtering | PASS through desktop interaction. Ctrl+Shift+V created/activated the popup at the pointer. Search had a caret/focus without a click. Typing `BETA` immediately filtered to matching text. Unicode case-insensitive substring behavior also has a unit test. |
| Arrows, Enter, Notepad and browser paste-back | PASS. Down/Up changed selection. Enter pasted `Clipdeck Beta` into Notepad. Later, selecting the older `Clipdeck verified paste` inserted exactly that string into the local Chrome textarea in `scripts/paste-target.html`. Final-build Notepad check also inserted the captured multiline text. |
| Escape and focus loss | PASS. Escape hid the popup; switching to another app hid it. Hotkey reopened the same native window with an empty, focused search. |
| Own paste does not re-record/promote | PASS. After pasting the third of three entries into Chrome, reopening showed the same three entries in the same order. Live clipboard test confirms own-write marker recognition; history unit test asserts own writes neither insert nor promote. |
| Near-zero idle CPU | PASS for this debug build/host. Hidden popup after interaction: 20.0 s sample consumed 0.109375 CPU seconds, 0.546875% of one logical CPU, approximately 0.0171% of this 32-logical-CPU host. RSS 86.375 MiB; private committed memory 80.020 MiB. One sample, not a release benchmark. |

All final checks passed: `cargo build`, `cargo clippy -- -D warnings`,
`cargo fmt --check`, `cargo test` (3 pass, 1 intentionally ignored), and the
explicit live clipboard test (1 pass). The ordinary test run leaves the real
clipboard alone.

Desktop verification used native app automation and visual/accessibility
observations. The automation's text-entry helper also temporarily writes the
clipboard; those writes can legitimately appear in history. Actual copying and
paste-back were exercised separately. Clipboard notifications expose current
contents; transient intermediate values overwritten before a listener reads
them cannot be reconstructed.

## GPUI findings and compromises

- Exact versions: component 0.9.0 and gpui-pre 0.3.11, with one resolved GPUI
  package. This GPUI release separates the Windows platform implementation;
  startup uses matching `gpui-pre-windows =0.3.11` and `Application::with_platform`.
  The convenience platform crate uses unstable `cfg_select!` on Rust 1.92, so
  the Windows-only app calls the Windows platform constructor directly.
- The pinned crates need a newer compiler than Rust 1.92; `rust-toolchain.toml`
  pins Rust 1.99.1, which builds them unmodified (no vendored or patched sources).
- Component setup follows its local source: `init`, `Root`, `InputState`,
  `Input`, and `ActiveTheme`. The spike uses a simple scrollable list with a
  scroll handle; it does not introduce a second search/list state machine.
- Frameless rendering works with `titlebar: None`. Native `SetWindowPos`
  supplies topmost positioning in physical cursor coordinates; `ShowWindow`
  hides the window, and reopening reuses its GPUI entity and native window.
  Destroying/recreating the last window initially caused app exit; reuse fixes
  that lifecycle problem.
- **Taskbar entry remains while visible.** `WS_EX_TOOLWINDOW` with
  `WS_EX_APPWINDOW` removed hid the popup from the desktop automation inventory,
  preventing keyboard acceptance verification through that tool. The delivered
  spike keeps a normal native window with no titlebar. This is an automation
  visibility compromise, not proof that GPUI cannot suppress a taskbar entry.
- Cursor placement is not clamped to the monitor work area; edge positions and
  mixed-DPI/multi-monitor layouts need milestone 2 acceptance. Rows are eager,
  in-memory, and display previews; Enter pastes the complete original text.

## Focus restoration findings

1. Capture foreground through `GetForegroundWindow` in the global-hotkey
   callback before opening the popup. The UI receives an opaque target only.
2. Write Unicode text plus a process-specific registered own-write marker;
   hide the popup; defer restoration/paste by 80 ms so Enter can be released.
   Synchronously restoring/sending inside Enter handling initially failed to
   insert text reliably. The deferred path passed both Notepad and Chrome.
3. Try `SetForegroundWindow` directly. Final-build diagnostics reported
   `Direct foreground restore: true` and the Notepad content visibly changed.
4. If refused, temporarily `AttachThreadInput` to the foreground thread, retry,
   then detach. This fallback exists but was not forced/verified on this host.
   The foreground HWND is checked before injection, and held modifiers/Enter
   cause cancellation after a bounded wait. `SendInput` must accept all four
   keyboard events (Ctrl down, V down/up, Ctrl up).
5. ALT-key trick and `AllowSetForegroundWindow` were not needed or tested.
   No fake ALT press is shipped. A cooperative target could grant foreground
   rights, but ordinary Notepad/browser targets do not provide that integration.

No claim is made for elevated targets (UIPI can block injection), secure
desktops, games, remote sessions, target-window closure/reuse races, or all
Windows foreground-lock policies. Paste failure is reported to stderr rather
than injecting into an unverified foreground window. Clipboard writes replace
the current clipboard; history is process-local and unbounded in this spike.
Ditto source was neither used nor copied.

## Recommended milestone 2 start

Make a repeatable desktop focus
matrix (including forced activation denial, elevated targets, monitor edges,
mixed DPI, and no-taskbar window testing with an appropriate harness). Then add
SQLite persistence/retention and tray lifecycle around these platform boundaries,
followed by images/files and the spec's richer list actions. Keep exclusion
checks and own-write tagging in front of every future persistence writer.
