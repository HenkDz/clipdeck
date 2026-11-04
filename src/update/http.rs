use std::{ffi::c_void, io::Write};
use windows::{
    Win32::Networking::WinHttp::*,
    core::{HSTRING, PCWSTR},
};

struct Handle(*mut c_void);
impl Handle {
    fn new(handle: *mut c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err(windows::core::Error::from_win32().to_string())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn url_parts(url: &str) -> Result<(&str, &str), String> {
    let (host, path) = url
        .strip_prefix("https://")
        .and_then(|url| url.split_once('/'))
        .ok_or("Updates require HTTPS")?;
    if ![
        "github.com",
        "release-assets.githubusercontent.com",
        "objects.githubusercontent.com",
    ]
    .contains(&host)
        || path.bytes().any(|b| b <= 32 || b == b'\\' || b == b'#')
        || url.len() > 8192
    {
        return Err("Update download redirected to an untrusted address".into());
    }
    Ok((host, path))
}
fn header(request: &Handle, kind: u32) -> Result<String, String> {
    let mut bytes = 8192u32;
    let mut value = vec![0u16; 4096];
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            kind,
            PCWSTR::null(),
            Some(value.as_mut_ptr().cast()),
            &mut bytes,
            std::ptr::null_mut(),
        )
    }
    .map_err(|e| e.to_string())?;
    Ok(String::from_utf16_lossy(&value[..bytes as usize / 2])
        .trim_end_matches('\0')
        .to_owned())
}

pub fn get(
    url: &str,
    output: &mut dyn Write,
    limit: u64,
    mut progress: impl FnMut(u64),
) -> Result<u64, String> {
    let mut url = url.to_owned();
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(if limit <= 16_384 { 30 } else { 300 });
    for _ in 0..8 {
        if started.elapsed() > timeout {
            return Err("Update request timed out; the installed version is unchanged".into());
        }
        let (host, path) = url_parts(&url)?;
        unsafe {
            let session = Handle::new(WinHttpOpen(
                &HSTRING::from(concat!("Clipdeck/", env!("CARGO_PKG_VERSION"))),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))?;
            WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 20_000).map_err(|e| e.to_string())?;
            let connection = Handle::new(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0))?;
            let request = Handle::new(WinHttpOpenRequest(
                connection.0,
                windows::core::w!("GET"),
                &HSTRING::from(format!("/{path}")),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ))?;
            WinHttpSetOption(
                Some(request.0.cast_const()),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes()),
            )
            .map_err(|e| e.to_string())?;
            WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| e.to_string())?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| e.to_string())?;
            let status = header(&request, WINHTTP_QUERY_STATUS_CODE)?;
            if ["301", "302", "303", "307", "308"].contains(&status.as_str()) {
                let location = header(&request, WINHTTP_QUERY_LOCATION)?;
                url = if location.starts_with('/') && !location.starts_with("//") {
                    format!("https://{host}{location}")
                } else {
                    location
                };
                continue;
            }
            if status != "200" {
                return Err(if status == "404" {
                    "This release does not provide signed updates yet".into()
                } else {
                    format!("Update server returned HTTP {status}")
                });
            }
            let mut total = 0u64;
            let mut buffer = [0u8; 65_536];
            loop {
                if started.elapsed() > timeout {
                    return Err(
                        "Update download timed out; the installed version is unchanged".into(),
                    );
                }
                let mut read = 0;
                WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                )
                .map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                total += u64::from(read);
                if total > limit {
                    return Err("Update exceeds its declared size limit".into());
                }
                output
                    .write_all(&buffer[..read as usize])
                    .map_err(|e| e.to_string())?;
                progress(total);
            }
            return Ok(total);
        }
    }
    Err("Too many update redirects".into())
}
pub fn bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut data = vec![];
    get(url, &mut data, limit, |_| {})?;
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirects_cannot_downgrade_tls_or_change_to_an_untrusted_host() {
        assert!(url_parts("https://github.com/HenkDz/clipdeck").is_ok());
        assert!(url_parts("https://release-assets.githubusercontent.com/x?a=1%2F2").is_ok());
        for bad in [
            "http://github.com/x",
            "https://github.com.evil.test/x",
            "https://github.com@evil.test/x",
            "https://github.com:80/x",
            "https://evil.test/x",
            "https://github.com/x\r\nHeader:y",
        ] {
            assert!(url_parts(bad).is_err(), "{bad}");
        }
    }
}
