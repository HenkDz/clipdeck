use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result},
};

pub(super) fn set_window(hwnd: HWND) -> Result<()> {
    unsafe {
        let module = GetModuleHandleW(None)?;
        let dpi = GetDpiForWindow(hwnd);
        for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let size = GetSystemMetricsForDpi(metric, dpi);
            let icon = LoadImageW(
                Some(module.into()),
                PCWSTR(std::ptr::without_provenance(1)), // Integer resource ID.
                IMAGE_ICON,
                size,
                size,
                LR_SHARED,
            )?;
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(kind as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }
    Ok(())
}
