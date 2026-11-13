use crate::{
    protocol::Key,
    state::{Captured, Shared},
};
use std::{
    collections::HashMap,
    io,
    sync::{atomic::Ordering, Arc, OnceLock},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
const TAG: usize = 0x4B455943;

pub fn start_hook(shared: Arc<Shared>) {
    let _ = SHARED.set(shared.clone());
    std::thread::spawn(move || unsafe {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if hook.is_null() {
            shared.status(format!(
                "Keyboard hook failed: {}",
                io::Error::last_os_error()
            ));
            return;
        }
        shared.hook_ready.store(true, Ordering::SeqCst);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        shared.hook_ready.store(false, Ordering::SeqCst);
        shared.pause();
        UnhookWindowsHookEx(hook);
    });
}

unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(l as *const KBDLLHOOKSTRUCT);
        // Ignore all synthetic keys: a receiving PC never re-broadcasts input.
        if k.flags & LLKHF_INJECTED == 0 {
            if let Some(shared) = SHARED.get() {
                let up = k.flags & LLKHF_UP != 0;
                if k.vkCode == VK_F12 as u32 {
                    if !up {
                        shared.pause();
                    }
                } else if !shared.receiver.load(Ordering::Relaxed) {
                    let epoch = shared.epoch.load(Ordering::SeqCst);
                    if epoch & 1 == 1 {
                        let event = Captured {
                            epoch,
                            key: Key {
                                vk: k.vkCode as u16,
                                scan: k.scanCode as u16,
                                extended: k.flags & LLKHF_EXTENDED != 0,
                                up,
                            },
                        };
                        if shared.input_tx.try_send(event).is_err() {
                            shared.pause();
                        }
                    }
                }
            }
        }
    }
    // Never suppress local typing, including F12.
    CallNextHookEx(std::ptr::null_mut(), code, w, l)
}

pub trait Injector {
    fn inject(&mut self, key: Key) -> io::Result<()>;
}
pub struct WindowsInjector;
impl Injector for WindowsInjector {
    fn inject(&mut self, key: Key) -> io::Result<()> {
        if key.vk == VK_F12 {
            return Ok(());
        }
        let flags = (if key.up { KEYEVENTF_KEYUP } else { 0 })
            | (if key.extended {
                KEYEVENTF_EXTENDEDKEY
            } else {
                0
            })
            | (if key.scan != 0 { KEYEVENTF_SCANCODE } else { 0 });
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: if key.scan == 0 { key.vk } else { 0 },
                    wScan: key.scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: TAG,
                },
            },
        };
        if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } != 1 {
            return Err(io::Error::other(
                "Windows blocked keyboard input (check app privilege level)",
            ));
        }
        Ok(())
    }
}

pub struct HeldKeys<I: Injector> {
    held: HashMap<(u16, u16, bool), Key>,
    injector: I,
}
impl<I: Injector> HeldKeys<I> {
    pub fn new(injector: I) -> Self {
        Self {
            held: HashMap::new(),
            injector,
        }
    }
    pub fn apply(&mut self, key: Key) -> io::Result<()> {
        if key.vk == VK_F12 || key.vk > 255 || key.scan > 255 {
            return Err(io::Error::other("Invalid key"));
        }
        if key.up && !self.held.contains_key(&key.identity()) {
            return Ok(());
        }
        self.injector.inject(key)?;
        if key.up {
            self.held.remove(&key.identity());
        } else {
            self.held.insert(key.identity(), key);
        }
        Ok(())
    }
    pub fn release(&mut self) {
        // Keep failures for retry on Drop. Never release keys this session didn't press.
        self.held.retain(|_, key| {
            let mut up = *key;
            up.up = true;
            self.injector.inject(up).is_err()
        });
    }
}
impl<I: Injector> Drop for HeldKeys<I> {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Fake(Arc<std::sync::Mutex<Vec<Key>>>);
    impl Injector for Fake {
        fn inject(&mut self, k: Key) -> io::Result<()> {
            self.0.lock().unwrap().push(k);
            Ok(())
        }
    }
    #[test]
    fn release_after_repeat_and_disconnect() {
        let log = Arc::new(std::sync::Mutex::new(vec![]));
        let k = Key {
            vk: 65,
            scan: 30,
            extended: false,
            up: false,
        };
        {
            let mut held = HeldKeys::new(Fake(log.clone()));
            held.apply(k).unwrap();
            held.apply(k).unwrap();
        }
        let events = log.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events[2].up);
    }
    #[test]
    fn stray_up_is_not_injected() {
        let log = Arc::new(std::sync::Mutex::new(vec![]));
        HeldKeys::new(Fake(log.clone()))
            .apply(Key {
                vk: 65,
                scan: 30,
                extended: false,
                up: true,
            })
            .unwrap();
        assert!(log.lock().unwrap().is_empty());
    }
}
