use windows::Win32::{
    Foundation::*,
    System::{SystemInformation::GetLocalTime, Time::*},
};
pub fn is_today(timestamp: i64) -> bool {
    let ticks = (timestamp.max(0) as u64)
        .saturating_add(11_644_473_600)
        .saturating_mul(10_000_000);
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        if FileTimeToSystemTime(&file, &mut utc).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err()
        {
            return false;
        }
        let now = GetLocalTime();
        (now.wYear, now.wMonth, now.wDay) == (local.wYear, local.wMonth, local.wDay)
    }
}
fn local(timestamp: i64) -> Result<SYSTEMTIME, String> {
    let ticks = (timestamp.max(0) as u64 + 11_644_473_600).saturating_mul(10_000_000);
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        FileTimeToSystemTime(&file, &mut utc).map_err(|e| e.to_string())?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).map_err(|e| e.to_string())?;
    }
    Ok(local)
}
pub fn stamp(timestamp: i64) -> Result<String, String> {
    let local = local(timestamp)?;
    Ok(format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute, local.wSecond
    ))
}
/// Local date and time for people, e.g. "2026-11-04 09:48".
pub fn when(timestamp: i64) -> String {
    local(timestamp)
        .map(|t| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute
            )
        })
        .unwrap_or_default()
}
#[cfg(test)]
mod tests {
    #[test]
    fn local_calendar() {
        assert!(super::is_today(crate::clip::now()));
        assert!(!super::is_today(crate::clip::now() - 172800));
    }
}
