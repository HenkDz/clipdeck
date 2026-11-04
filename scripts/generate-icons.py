"""Convert the selected PNG into Windows icon assets (requires Pillow)."""
from pathlib import Path
from PIL import Image

assets = Path(__file__).resolve().parents[1] / "assets"
source = Image.open(assets / "clipdeck.png").convert("RGBA")
assert source.width == source.height, "The app icon must be square"
source.save(
    assets / "clipdeck.ico",
    sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)],
)
source.resize((32, 32), Image.Resampling.LANCZOS).save(assets / "tray.png")
with Image.open(assets / "clipdeck.ico") as icon:
    assert icon.info["sizes"] == {(n, n) for n in (16, 24, 32, 48, 64, 128, 256)}
print("Generated clipdeck.ico (16–256 px) and tray.png (32 px)")
