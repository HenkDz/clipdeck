"""Prepare portable and signed-update release assets after a validated Windows build."""
import argparse,hashlib,json,shutil,subprocess,tomllib,zipfile
from pathlib import Path

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--notes',type=Path,required=True)
parser.add_argument('--schema',type=int,default=5)
args=parser.parse_args()
version=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
assert len(version.split('.'))==3 and all(p.isdigit() for p in version.split('.'))
notes=args.notes.read_text(encoding='utf-8')
assert len(notes.encode('utf-8'))<=4000 and '\0' not in notes
assert 1<=args.schema<=10000
exe=root/'target/release/clipdeck.exe'
assert 65536<=exe.stat().st_size<=128*1024*1024
dist=root/'target/dist'/f'v{version}';dist.mkdir(parents=True,exist_ok=True)
asset=dist/f'clipdeck-{version}-windows-x64.exe';shutil.copyfile(exe,asset)
hash=hashlib.sha256(asset.read_bytes()).hexdigest()
manifest=dist/'clipdeck-update.toml'
manifest.write_bytes(f'format = 1\nversion = "{version}"\nsize = {asset.stat().st_size}\nsha256 = "{hash}"\nschema = {args.schema}\nnotes = {json.dumps(notes,ensure_ascii=False)}\n'.encode('utf-8'))
subprocess.run(['powershell','-NoProfile','-File',str(root/'scripts/sign-update.ps1'),'-Manifest',str(manifest)],check=True)
assert manifest.with_suffix('.toml.sig').stat().st_size==64
archive=dist/f'clipdeck-{version}-windows-x64.zip'
readme=f'''Clipdeck {version} - Windows x64

Quit any running Clipdeck copy from its tray menu, then unzip and run clipdeck.exe.
Ctrl+Shift+V opens the popup. Use Check for updates in the tray menu or Settings
to download a verified update, then choose Update and restart.

History and settings stay in %APPDATA%\\Clipdeck. No installer is required.
Daily update checks are optional and off by default. Updates contact GitHub;
clipboard contents stay local. Automatic binary replacement needs a writable app folder.

Windows 10 or 11, 64-bit. The executable is unsigned; SmartScreen may ask for
More info, then Run anyway. Signed update metadata is separate from Windows code signing.
If VCRUNTIME140.dll is missing: https://aka.ms/vs/17/release/vc_redist.x64.exe
Source and documentation: https://github.com/HenkDz/clipdeck
'''
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    z.write(asset,'clipdeck.exe');z.writestr('README.txt',readme)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None and z.read('clipdeck.exe')==asset.read_bytes()
files=[archive,asset,manifest,manifest.with_suffix('.toml.sig')]
(dist/'SHA256SUMS.txt').write_bytes(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in files).encode())
(dist/'notes.md').write_text(notes,encoding='utf-8')
print('Prepared and verified:',dist)
