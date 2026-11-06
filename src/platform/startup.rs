use std::path::{Path, PathBuf};
use windows::{
    Win32::{Foundation::RPC_E_CHANGED_MODE, System::Com::*, UI::Shell::*},
    core::{HSTRING, Interface, w},
};

fn shortcut() -> Result<PathBuf, String> {
    Ok(super::locations::known_folder(&FOLDERID_Startup)?.join("Clipdeck.lnk"))
}

pub fn enabled() -> Result<bool, String> {
    shortcut()?.try_exists().map_err(|e| e.to_string())
}

pub fn set(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    set_at(&shortcut()?, enabled.then_some(exe.as_path()))
}

fn set_at(path: &Path, exe: Option<&Path>) -> Result<(), String> {
    let Some(exe) = exe else {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    };
    let parent = exe.parent().ok_or("Application folder is unavailable")?;
    let temp = path.with_extension("lnk.tmp");
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if initialized != RPC_E_CHANGED_MODE {
            initialized.ok().map_err(|e| e.to_string())?;
        }
        let result = (|| -> windows::core::Result<()> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(exe.as_os_str()))?;
            link.SetWorkingDirectory(&HSTRING::from(parent.as_os_str()))?;
            link.SetDescription(w!("Clipdeck clipboard history"))?;
            link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;
            let file: IPersistFile = link.cast()?;
            file.Save(&HSTRING::from(temp.as_os_str()), true)
        })();
        if initialized.is_ok() {
            CoUninitialize();
        }
        result.map_err(|e| e.to_string())?;
    }
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_shortcut_round_trips_unicode_paths_and_disables_idempotently() {
        let folder = std::env::temp_dir().join(format!("clipdeck-startup-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Clipdeck.lnk");
        let exe = folder.join("Clipdeck été.exe");
        set_at(&path, Some(&exe)).unwrap();
        set_at(&path, Some(&exe)).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(set_at(&path, Some(Path::new(""))).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                let file: IPersistFile = link.cast().unwrap();
                file.Load(&HSTRING::from(path.as_os_str()), STGM_READ)
                    .unwrap();
                let mut target = [0u16; 32768];
                link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
                    .unwrap();
                let end = target.iter().position(|c| *c == 0).unwrap();
                assert_eq!(HSTRING::from_wide(&target[..end]).to_os_string(), exe);
            }
            CoUninitialize();
        }
        set_at(&path, None).unwrap();
        set_at(&path, None).unwrap();
        assert!(!path.exists());
        assert!(set_at(&folder.join("missing/Clipdeck.lnk"), Some(&exe)).is_err());
        std::fs::remove_dir(&folder).unwrap();
    }
}
