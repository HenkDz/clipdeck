The owner-selected **stacked clipboard** icon was generated with OpenAI imagegen.
Prompt: two overlapping rounded cards, a cream front card with a blue clipboard
tab, a blue rear card, a warm charcoal rounded tile and transparent exterior.

`clipdeck.png` is the selected source. `clipdeck.ico` contains 16, 24, 32, 48,
64, 128 and 256 px images for Explorer and native windows. `tray.png` is 32 px.
These assets are embedded in the executable; no external icon files are required
to run Clipdeck.

To regenerate the ICO and tray PNG after replacing the source, install Pillow
and run `python scripts/generate-icons.py`. Normal Rust builds use the checked-in
assets and the Windows SDK resource compiler, without Python or extra Rust crates.
