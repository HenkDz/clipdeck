use super::manifest::Version;
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::*, Storage::FileSystem::*, System::Threading::*, UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PWSTR, w},
};

pub struct Parent(HANDLE);
pub struct UpdateLock(HANDLE);
impl Drop for UpdateLock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl UpdateLock {
    pub fn acquire() -> Result<Self, String> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\Clipdeck.Update"))
                .map_err(|e| e.to_string())?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return Err("Another Clipdeck update is already running".into());
            }
            Ok(Self(handle))
        }
    }
}
pub fn updating() -> Result<bool, String> {
    match unsafe {
        OpenMutexW(
            SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000),
            false,
            w!("Local\\Clipdeck.Update"),
        )
    } {
        Ok(handle) => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            Ok(true)
        }
        Err(error)
            if error.code() == windows::core::HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.to_string()),
    }
}
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Parent {
    pub fn open(pid: u32) -> Result<Self, String> {
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map(Self)
        .map_err(|e| e.to_string())
    }
    pub fn created(&self) -> Result<u64, String> {
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe { GetProcessTimes(self.0, &mut created, &mut exit, &mut kernel, &mut user) }
            .map_err(|e| e.to_string())?;
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
    pub fn image(&self) -> Result<PathBuf, String> {
        let mut path = vec![0u16; 32768];
        let mut size = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                self.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut size,
            )
        }
        .map_err(|e| e.to_string())?;
        PathBuf::from(String::from_utf16(&path[..size as usize]).map_err(|e| e.to_string())?)
            .canonicalize()
            .map_err(|e| e.to_string())
    }
    pub fn wait(&self) -> Result<(), String> {
        if unsafe { WaitForSingleObject(self.0, 30_000) } == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err("Clipdeck did not close; the executable was not replaced".into())
        }
    }
}
pub fn replace(target: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    if backup.exists() {
        return Err("Update backup already exists; nothing was replaced".into());
    }
    unsafe {
        ReplaceFileW(
            &HSTRING::from(target.as_os_str()),
            &HSTRING::from(replacement.as_os_str()),
            &HSTRING::from(backup.as_os_str()),
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    }
    .map_err(|e| e.to_string())
}
pub fn version(path: &Path) -> Result<Version, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos).map_err(|e| e.to_string())?;
    if &dos[..2] != b"MZ" {
        return Err("Update is not a Windows executable".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    file.seek(SeekFrom::Start(u64::from(offset)))
        .map_err(|e| e.to_string())?;
    let mut pe = [0; 6];
    file.read_exact(&mut pe).map_err(|e| e.to_string())?;
    if &pe[..4] != b"PE\0\0" || pe[4..] != [0x64, 0x86] {
        return Err("Update is not a Windows x64 executable".into());
    }
    unsafe {
        let name = HSTRING::from(path.as_os_str());
        let size = GetFileVersionInfoSizeW(&name, None);
        if size == 0 || size > 1_048_576 {
            return Err("Update has no valid Windows version metadata".into());
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&name, None, size, data.as_mut_ptr().cast())
            .map_err(|e| e.to_string())?;
        let mut info = std::ptr::null_mut();
        let mut length = 0;
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut info, &mut length).as_bool()
            || info.is_null()
            || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
        {
            return Err("Invalid executable version metadata".into());
        }
        let version = std::ptr::read_unaligned(info.cast::<VS_FIXEDFILEINFO>());
        if version.dwSignature != 0xfeef04bd || version.dwFileVersionLS & 0xffff != 0 {
            return Err("Unsupported executable version metadata".into());
        }
        format!(
            "{}.{}.{}",
            version.dwFileVersionMS >> 16,
            version.dwFileVersionMS & 0xffff,
            version.dwFileVersionLS >> 16
        )
        .parse()
    }
}
pub fn error_dialog(message: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            w!("Clipdeck update"),
            MB_OK | MB_ICONERROR,
        );
    }
}
