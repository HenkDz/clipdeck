use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{DataExchange::GetClipboardOwner, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR, PWSTR},
};
pub fn name() -> String {
    unsafe {
        let hwnd = GetClipboardOwner()
            .ok()
            .filter(|h| !h.is_invalid())
            .unwrap_or_else(|| GetForegroundWindow());
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return "Unknown app".into();
        };
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        if result.is_err() {
            return "Unknown app".into();
        }
        path.truncate(len as usize);
        path.push(0);
        let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
        if size > 0 && size < 16 * 1024 * 1024 {
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, data.as_mut_ptr().cast())
                .is_ok()
            {
                let mut ptr = std::ptr::null_mut();
                let mut count = 0;
                let query: Vec<u16> = "\\VarFileInfo\\Translation\0".encode_utf16().collect();
                if VerQueryValueW(
                    data.as_ptr().cast(),
                    PCWSTR(query.as_ptr()),
                    &mut ptr,
                    &mut count,
                )
                .as_bool()
                    && count >= 4
                {
                    let lang = std::slice::from_raw_parts(ptr.cast::<u16>(), 2);
                    let query: Vec<u16> = format!(
                        "\\StringFileInfo\\{:04x}{:04x}\\FileDescription\0",
                        lang[0], lang[1]
                    )
                    .encode_utf16()
                    .collect();
                    if VerQueryValueW(
                        data.as_ptr().cast(),
                        PCWSTR(query.as_ptr()),
                        &mut ptr,
                        &mut count,
                    )
                    .as_bool()
                        && count > 1
                    {
                        return String::from_utf16_lossy(std::slice::from_raw_parts(
                            ptr.cast::<u16>(),
                            count as usize - 1,
                        ));
                    }
                }
            }
        }
        std::path::Path::new(&String::from_utf16_lossy(&path[..path.len() - 1]))
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown app".into())
    }
}
