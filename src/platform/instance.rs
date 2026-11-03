use super::{Event, Target};
use windows::{
    Win32::{Foundation::*, System::Threading::*},
    core::{Result, w},
};
pub struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Instance {
    pub fn acquire(sender: async_channel::Sender<Event>) -> Result<Option<Self>> {
        unsafe {
            let mutex = CreateMutexW(None, false, w!("Local\\Clipdeck.Instance"))?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(mutex);
                for _ in 0..100 {
                    if let Ok(event) =
                        OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\Clipdeck.Open"))
                    {
                        let result = SetEvent(event);
                        let _ = CloseHandle(event);
                        result?;
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                return Err(windows::core::Error::from_win32());
            }
            let event = CreateEventW(None, false, false, w!("Local\\Clipdeck.Open"))?;
            let handle = event.0 as usize;
            std::thread::spawn(move || {
                let event = HANDLE(handle as *mut _);
                while WaitForSingleObject(event, INFINITE) == WAIT_OBJECT_0 {
                    if sender
                        .send_blocking(Event::Open(Target::capture()))
                        .is_err()
                    {
                        break;
                    }
                }
                let _ = CloseHandle(event);
            });
            Ok(Some(Self(mutex)))
        }
    }
}
