"""Native updater acceptance. Requires a debug baseline, a newer release EXE, and the signing account."""
import argparse, ctypes, hashlib, io, json, os, secrets, shutil, sqlite3, subprocess, time
from pathlib import Path
from PIL import Image

repo = Path(__file__).resolve().parents[1]
flags = subprocess.CREATE_NO_WINDOW

def run(args, **kwargs):
    return subprocess.run(args, check=True, creationflags=flags, **kwargs)

def processes():
    command = "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); Get-Process | Where-Object ProcessName -in @('clipdeck','helper') | Select-Object Id,Path | ConvertTo-Json -Compress"
    result = run(['powershell','-NoProfile','-Command',command],capture_output=True,text=True,encoding='utf-8')
    data = json.loads(result.stdout or '[]')
    return data if isinstance(data,list) else [data]

def stop_fixture(root):
    assert root.resolve().is_relative_to((repo/'target/updater-acceptance').resolve())
    k = ctypes.WinDLL('kernel32',use_last_error=True)
    k.OpenProcess.argtypes=[ctypes.c_ulong,ctypes.c_int,ctypes.c_ulong];k.OpenProcess.restype=ctypes.c_void_p
    k.TerminateProcess.argtypes=[ctypes.c_void_p,ctypes.c_uint];k.CloseHandle.argtypes=[ctypes.c_void_p]
    for process in processes():
        if process.get('Path') and Path(process['Path']).resolve().is_relative_to(root.resolve()):
            handle=k.OpenProcess(1,False,process['Id'])
            if handle:k.TerminateProcess(handle,0);k.CloseHandle(handle)

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def seed(profile):
    folder=profile/'Clipdeck';folder.mkdir(parents=True)
    (folder/'config.toml').write_text('history_size=50\nmax_clip_mb=16\ncheck_updates=false\n',encoding='utf-8')
    db=sqlite3.connect(folder/'history.db')
    db.executescript('''CREATE TABLE clips(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,text TEXT NOT NULL,search TEXT NOT NULL,payload BLOB,hash BLOB NOT NULL,thumbnail BLOB NOT NULL,source TEXT NOT NULL,created INTEGER NOT NULL,updated INTEGER NOT NULL,rank INTEGER NOT NULL,pinned INTEGER NOT NULL DEFAULT 0,copies INTEGER NOT NULL DEFAULT 1,file_path TEXT NOT NULL DEFAULT '',screenshot_title TEXT NOT NULL DEFAULT '',source_exe TEXT NOT NULL DEFAULT '',pastes INTEGER NOT NULL DEFAULT 0,plain_pastes INTEGER NOT NULL DEFAULT 0,file_kinds TEXT NOT NULL DEFAULT '',UNIQUE(kind,hash));
    CREATE TABLE formats(clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,name TEXT NOT NULL,data BLOB NOT NULL,PRIMARY KEY(clip_id,name));CREATE INDEX clips_order ON clips(pinned DESC,rank DESC);PRAGMA user_version=5;''')
    png=io.BytesIO();Image.new('RGBA',(5,3),(40,100,210,180)).save(png,format='PNG');png=png.getvalue()
    for id,kind,text,payload in [(1,'text','Updater private history fixture',None),(2,'image','5×3',png)]:
        hash=hashlib.sha256(kind.encode()+b'\0'+(payload or text.encode())).digest()
        db.execute('INSERT INTO clips(id,kind,text,search,payload,hash,thumbnail,source,created,updated,rank,pinned,copies,pastes,plain_pastes) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
                   (id,kind,text,text.lower(),payload,hash,png if payload else b'','Acceptance fixture',1793800000,1793800000,id,1,3,2,1))
    db.commit();db.close()
    return folder/'history.db'

def contents(path):
    db=sqlite3.connect(path)
    try:
        assert db.execute('pragma quick_check').fetchone()[0]=='ok'
        return db.execute('pragma user_version').fetchone()[0],db.execute('select * from clips order by id').fetchall()
    finally:db.close()

def failure_exe(root):
    rc = sorted(Path(os.environ['ProgramFiles(x86)']).glob('Windows Kits/10/bin/*/x64/rc.exe'))[-1]
    resource=root/'failure.rc'
    resource.write_text('''1 VERSIONINFO
FILEVERSION 0,1,3,0
PRODUCTVERSION 0,1,3,0
FILEOS 0x40004L
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "FileVersion", "0.1.3\\0"
  END
 END
END
''')
    run([str(rc),'/nologo','/fo',str(root/'failure.res'),str(resource)],capture_output=True)
    source=root/'failure.rs'
    source.write_text('''#![windows_subsystem="windows"]
use std::os::windows::process::CommandExt;
fn main(){
 let _=std::process::Command::new("python").args(["-c", "import os,sqlite3; from pathlib import Path; db=sqlite3.connect(Path(os.environ['APPDATA'])/'Clipdeck'/'history.db'); db.executescript('PRAGMA user_version=6; DELETE FROM clips;'); db.close()"])
 .creation_flags(0x08000000).status();
 std::process::exit(77);
}
''')
    exe=root/'failure.exe'
    run(['rustc',str(source),'-o',str(exe),'-C',f'link-arg={root / "failure.res"}'],capture_output=True)
    return exe,'0.1.3'

def wait_for(predicate,seconds=45):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        if predicate():return
        time.sleep(.15)
    raise AssertionError('Acceptance timed out')

def test(case,baseline,candidate):
    root=(repo/'target/updater-acceptance'/f'{case} space été {secrets.token_hex(4)}').resolve()
    root.mkdir(parents=True)
    app=root/'app';app.mkdir();target=app/'clipdeck.exe';shutil.copyfile(baseline,target)
    old_hash=digest(target)
    env=dict(os.environ,APPDATA=str(root/'roaming'),LOCALAPPDATA=str(root/'local'))
    db=seed(root/'roaming');before=contents(db)
    stage=root/'local/Clipdeck/updates'/secrets.token_hex(16);stage.mkdir(parents=True)
    payload,version=(failure_exe(root) if case=='startup-failure' else (candidate,'0.1.2'))
    shutil.copyfile(payload,stage/'download.exe')
    schema=6 if case=='startup-failure' else 5
    (stage/'update.toml').write_text(f'format=1\nversion="{version}"\nsize={payload.stat().st_size}\nsha256="{digest(payload)}"\nschema={schema}\nnotes="Isolated native acceptance"\n',encoding='utf-8')
    run(['powershell','-NoProfile','-File',str(repo/'scripts/sign-update.ps1'),'-Manifest',str(stage/'update.toml')],capture_output=True)
    (stage/'update.toml.sig').rename(stage/'update.sig')
    locked=None
    if case=='locked':
        k=ctypes.WinDLL('kernel32',use_last_error=True)
        k.CreateFileW.argtypes=[ctypes.c_wchar_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p];k.CreateFileW.restype=ctypes.c_void_p
        k.CloseHandle.argtypes=[ctypes.c_void_p]
        locked=k.CreateFileW(str(target),0x80000000,3,None,3,0,None)
        assert locked not in (None,ctypes.c_void_p(-1).value)
    try:
        parent=subprocess.Popen([str(target),'--test-update',str(stage)],env=env,creationflags=flags,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        if case=='success':
            wait_for(lambda:(stage/'helper.ready').exists())
            duplicate=subprocess.Popen([str(target)],env=env,creationflags=flags,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            assert duplicate.wait(timeout=3)==0,'A second launch entered the running application during update'
            print('PASS concurrent launch: installation guard prevents another app instance',flush=True)
        if case=='success':
            wait_for(lambda:(stage/'app.ready').exists())
            assert digest(target)==digest(candidate)
            assert contents(db)==before
            assert contents(stage/'history-before.db')==before
            print('PASS success: exact release installed, new app ready, text/image history and counters unchanged',flush=True)
        else:
            wait_for(lambda:(stage/'rolled-back').exists())
            assert digest(target)==old_hash
            assert contents(db)==before
            assert (stage/'helper.error').exists() or (stage/'rolled-back').exists()
            if case=='startup-failure':
                failed=list((root/'roaming/Clipdeck').glob('history.failed-*.db'))
                assert failed and contents(failed[0])[0]==6
            print(f'PASS {case}: previous executable restored, schema v5 text/image history and counters intact',flush=True)
        parent.wait(timeout=10)
        print('Evidence:',root,flush=True)
    finally:
        if locked:k.CloseHandle(locked)
        stop_fixture(root)

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--case',choices=['success','locked','startup-failure','all'],default='all')
    args=parser.parse_args()
    assert not any(p.get('Path') and Path(p['Path']).name.lower()=='clipdeck.exe' for p in processes()),'Close Clipdeck before running native updater tests'
    for case in (['success','locked','startup-failure'] if args.case=='all' else [args.case]):
        test(case,args.baseline.resolve(),args.candidate.resolve())
