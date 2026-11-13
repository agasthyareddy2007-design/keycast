use crate::{input, network, protocol, state::Shared};
use std::{
    collections::BTreeMap,
    net::Ipv4Addr,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};

const ROLE: usize = 10;
const CODE: usize = 11;
const SHOW: usize = 12;
const START: usize = 13;
const PAUSE: usize = 14;
const PREV: usize = 15;
const NEXT: usize = 16;
const ROW: usize = 100;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn label(hwnd: HWND, s: &str) {
    SetWindowTextW(hwnd, wide(s).as_ptr());
}
unsafe fn text(hwnd: HWND) -> String {
    let mut buf = vec![0; GetWindowTextLengthW(hwnd) as usize + 1];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n as usize])
}
unsafe fn control(
    parent: HWND,
    class: &str,
    title: &str,
    style: u32,
    rect: [i32; 4],
    id: usize,
    font: HFONT,
) -> HWND {
    let h = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(title).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect[0],
        rect[1],
        rect[2],
        rect[3],
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, font as usize, 1);
    h
}
struct Ui {
    hwnd: HWND,
    font: HFONT,
    heading_font: HFONT,
    shared: Arc<Shared>,
    runtime: Option<network::Runtime>,
    code: String,
    receiver: bool,
    role: HWND,
    code_edit: HWND,
    show: HWND,
    start: HWND,
    pause: HWND,
    banner: HWND,
    status: HWND,
    section: HWND,
    metric: HWND,
    hint: HWND,
    prev: HWND,
    next: HWND,
    rows: Vec<(Ipv4Addr, HWND, HWND)>,
    page: usize,
    last_status: String,
    last_banner: String,
    last_metric: String,
}
impl Ui {
    unsafe fn new(hwnd: HWND) -> Self {
        let font = CreateFontW(
            -17,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let heading_font = CreateFontW(
            -32,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let (tx, rx) = mpsc::sync_channel(1024);
        let shared = Arc::new(Shared {
            epoch: AtomicU64::new(0),
            receiver: AtomicBool::new(false),
            hook_ready: AtomicBool::new(false),
            peers: Mutex::new(BTreeMap::new()),
            status: Mutex::new("Choose a mode and use the same pairing code on all PCs.".into()),
            input_tx: tx,
            sent: AtomicU64::new(0),
        });
        input::start_hook(shared.clone());
        network::dispatcher(shared.clone(), rx);
        control(
            hwnd,
            "STATIC",
            "KEYCAST",
            0,
            [28, 22, 250, 44],
            0,
            heading_font,
        );
        control(
            hwnd,
            "STATIC",
            "ONE KEYBOARD. EVERY SCREEN.",
            0,
            [290, 38, 450, 28],
            0,
            font,
        );
        control(
            hwnd,
            "STATIC",
            "Wireless keyboard broadcasting  /  Windows + Wi-Fi",
            0,
            [30, 76, 720, 28],
            0,
            font,
        );
        let banner = control(
            hwnd,
            "STATIC",
            "SET UP THIS PC",
            0,
            [30, 118, 730, 30],
            0,
            font,
        );
        let role = control(
            hwnd,
            "BUTTON",
            "This is a receiving PC",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            [30, 157, 280, 28],
            ROLE,
            font,
        );
        control(
            hwnd,
            "STATIC",
            "Pairing code",
            0,
            [30, 200, 130, 28],
            0,
            font,
        );
        let code = protocol::new_code();
        let code_edit = control(
            hwnd,
            "EDIT",
            &code,
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
            [160, 196, 430, 32],
            CODE,
            font,
        );
        let show = control(
            hwnd,
            "BUTTON",
            "Show",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            [610, 198, 100, 28],
            SHOW,
            font,
        );
        SendMessageW(code_edit, EM_SETLIMITTEXT, 64, 0);
        let start = control(
            hwnd,
            "BUTTON",
            "Find my PCs",
            WS_TABSTOP,
            [30, 247, 240, 42],
            START,
            font,
        );
        let pause = control(
            hwnd,
            "BUTTON",
            "Start broadcasting",
            WS_TABSTOP,
            [290, 247, 250, 42],
            PAUSE,
            font,
        );
        EnableWindow(pause, 0);
        let section = control(
            hwnd,
            "STATIC",
            "QUICK START",
            0,
            [30, 314, 560, 28],
            0,
            font,
        );
        let prev = control(
            hwnd,
            "BUTTON",
            "<",
            WS_TABSTOP,
            [655, 305, 48, 34],
            PREV,
            font,
        );
        let next = control(
            hwnd,
            "BUTTON",
            ">",
            WS_TABSTOP,
            [713, 305, 48, 34],
            NEXT,
            font,
        );
        ShowWindow(prev, SW_HIDE);
        ShowWindow(next, SW_HIDE);
        let hint = control(hwnd,"STATIC","1. Open Keycast on every PC on the same Wi-Fi.\r\n\r\n2. On other PCs, choose Receive and paste this pairing code.\r\n\r\n3. On this PC, find receivers, check them, and start broadcasting.",0,[30,355,740,200],0,font);
        let metric = control(hwnd, "STATIC", "", 0, [30, 570, 740, 26], 0, font);
        let status = control(hwnd, "STATIC", "", 0, [30, 609, 740, 62], 0, font);
        control(
            hwnd,
            "STATIC",
            "F12  /  PAUSE INSTANTLY       Click to resume. No keystrokes are saved.",
            0,
            [30, 690, 750, 28],
            0,
            font,
        );
        control(
            hwnd,
            "STATIC",
            "Pause before passwords. Match keyboard layouts. Restart to change mode.",
            0,
            [30, 724, 750, 28],
            0,
            font,
        );
        SetTimer(hwnd, 1, 80, None);
        Self {
            hwnd,
            font,
            heading_font,
            shared,
            runtime: None,
            code,
            receiver: false,
            role,
            code_edit,
            show,
            start,
            pause,
            banner,
            status,
            section,
            metric,
            hint,
            prev,
            next,
            rows: vec![],
            page: 0,
            last_status: String::new(),
            last_banner: String::new(),
            last_metric: String::new(),
        }
    }
    unsafe fn command(&mut self, id: usize) {
        match id {
            ROLE if self.runtime.is_none() => {
                self.receiver = SendMessageW(self.role, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                label(
                    self.start,
                    if self.receiver {
                        "Enable receiver"
                    } else {
                        "Find my PCs"
                    },
                );
                if self.receiver {
                    label(self.code_edit, "");
                    label(self.hint,"Paste the code from your main PC, then enable this receiver.\r\n\r\nKeep Keycast open and focus the window you want to type into.\r\n\r\nF12 on this PC stops incoming typing and releases held keys.");
                } else {
                    label(self.code_edit, &self.code);
                    label(self.hint,"Use this pairing code on your other PCs.\r\n\r\nFind receivers, check their boxes, then start broadcasting.");
                }
            }
            SHOW => {
                let show = SendMessageW(self.show, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                SendMessageW(
                    self.code_edit,
                    EM_SETPASSWORDCHAR,
                    if show { 0 } else { 0x25cf },
                    0,
                );
                InvalidateRect(self.code_edit, null(), 1);
            }
            START if self.runtime.is_none() => {
                if let Some(code) = protocol::normalize_code(&text(self.code_edit)) {
                    self.code = code;
                    self.shared.receiver.store(self.receiver, Ordering::SeqCst);
                    if self.receiver {
                        self.shared.resume();
                    }
                    self.runtime = Some(network::start(
                        self.shared.clone(),
                        self.receiver,
                        self.code.clone(),
                    ));
                    EnableWindow(self.role, 0);
                    EnableWindow(self.start, 0);
                    SendMessageW(self.code_edit, EM_SETREADONLY, 1, 0);
                    label(
                        self.section,
                        if self.receiver {
                            "THIS PC IS A RECEIVER"
                        } else {
                            "SELECT YOUR RECEIVERS"
                        },
                    );
                    label(
                        self.hint,
                        if self.receiver {
                            "This PC will appear on your main PC.\r\n\r\nSelect it there, then focus an app here to receive typing.\r\n\r\nOnly a main PC with your pairing code can send input."
                        } else {
                            "Searching your Wi-Fi for receivers...\r\n\r\nAllow Keycast through Windows Firewall on Private networks.\r\nGuest Wi-Fi or client isolation can block discovery."
                        },
                    );
                } else {
                    self.shared
                        .status("Enter the main PC's full 32-character hexadecimal pairing code.");
                }
            }
            PAUSE if self.runtime.is_some() => {
                if self.shared.active() {
                    self.shared.pause();
                } else {
                    self.shared.resume();
                }
            }
            PREV => {
                self.page = self.page.saturating_sub(1);
            }
            NEXT => {
                self.page += 1;
            }
            id if (ROW..ROW + 6).contains(&id) => {
                if let Some((ip, check, _)) = self.rows.get(id - ROW) {
                    let selected = SendMessageW(*check, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    network::select(
                        self.shared.clone(),
                        self.runtime.as_ref().unwrap().stop.clone(),
                        *ip,
                        self.code.clone(),
                        selected,
                    );
                }
            }
            _ => (),
        }
        self.refresh();
    }
    unsafe fn refresh(&mut self) {
        let active = self.shared.active();
        let banner = if self.runtime.is_none() {
            "SET UP THIS PC"
        } else if !active {
            "PAUSED / PRIVATE     -     Click below to resume"
        } else if self.receiver {
            "RECEIVING ENABLED     -     F12 to stop incoming typing"
        } else {
            "BROADCASTING     -     F12 to pause"
        };
        if banner != self.last_banner {
            label(self.banner, banner);
            self.last_banner = banner.into();
        }
        let status = self.shared.status.lock().unwrap().clone();
        if status != self.last_status {
            label(self.status, &status);
            self.last_status = status;
        }
        if self.runtime.is_none() {
            EnableWindow(
                self.start,
                self.shared.hook_ready.load(Ordering::SeqCst) as i32,
            );
            return;
        }
        let peers: Vec<_> = self
            .shared
            .peers
            .lock()
            .unwrap()
            .iter()
            .map(|(ip, p)| (*ip, p.name.clone(), p.status.clone(), p.selected))
            .collect();
        let selected = peers.iter().filter(|p| p.3).count();
        let ready = peers
            .iter()
            .filter(|p| p.3 && p.2 == "Paired / ready")
            .count();
        EnableWindow(
            self.pause,
            (active || self.receiver || (ready > 0 && ready == selected)) as i32,
        );
        let button = if active {
            "Pause now   [F12]"
        } else if self.receiver {
            "Enable receiving"
        } else {
            "Start broadcasting"
        };
        if text(self.pause) != button {
            label(self.pause, button);
        }
        // Code cannot be selected/copied during broadcasting (Ctrl+C would be mirrored).
        EnableWindow(self.code_edit, (!active) as i32);
        EnableWindow(self.show, (!active) as i32);
        if !self.receiver {
            self.page = self.page.min(peers.len().saturating_sub(1) / 6);
            let visible: Vec<_> = peers.iter().skip(self.page * 6).take(6).collect();
            let ids: Vec<_> = visible.iter().map(|p| p.0).collect();
            if self.rows.iter().map(|p| p.0).collect::<Vec<_>>() != ids {
                for (_, check, status) in self.rows.drain(..) {
                    DestroyWindow(check);
                    DestroyWindow(status);
                }
                for (i, p) in visible.iter().enumerate() {
                    let y = 350 + i as i32 * 35;
                    let check = control(
                        self.hwnd,
                        "BUTTON",
                        "",
                        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                        [30, y, 410, 28],
                        ROW + i,
                        self.font,
                    );
                    let status = control(
                        self.hwnd,
                        "STATIC",
                        "",
                        0,
                        [450, y + 3, 315, 28],
                        0,
                        self.font,
                    );
                    self.rows.push((p.0, check, status));
                }
            }
            for ((_, check, status), p) in self.rows.iter().zip(visible) {
                let title = format!("{}   ({})", p.1, p.0);
                if text(*check) != title {
                    label(*check, &title);
                }
                if text(*status) != p.2 {
                    label(*status, &p.2);
                }
                SendMessageW(
                    *check,
                    BM_SETCHECK,
                    if p.3 {
                        BST_CHECKED as usize
                    } else {
                        BST_UNCHECKED as usize
                    },
                    0,
                );
            }
            ShowWindow(self.hint, if peers.is_empty() { SW_SHOW } else { SW_HIDE });
            ShowWindow(self.prev, if peers.len() > 6 { SW_SHOW } else { SW_HIDE });
            ShowWindow(self.next, if peers.len() > 6 { SW_SHOW } else { SW_HIDE });
            EnableWindow(self.prev, (self.page > 0) as i32);
            EnableWindow(self.next, ((self.page + 1) * 6 < peers.len()) as i32);
        }
        let metric = format!(
            "{} paired receivers   |   {} key events {}",
            ready,
            self.shared.sent.load(Ordering::Relaxed),
            if self.receiver { "received" } else { "sent" }
        );
        if metric != self.last_metric {
            label(self.metric, &metric);
            self.last_metric = metric;
        }
    }
}
impl Drop for Ui {
    fn drop(&mut self) {
        self.shared.pause();
        if let Some(runtime) = &self.runtime {
            runtime.stop.store(true, Ordering::SeqCst);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        unsafe {
            DeleteObject(self.font);
            DeleteObject(self.heading_font);
        }
    }
}
unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        if !ptr.is_null() {
            drop(Box::from_raw(ptr));
        }
        PostQuitMessage(0);
        return 0;
    }
    if msg == WM_COMMAND || msg == WM_TIMER {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
        if !ptr.is_null() {
            if msg == WM_TIMER {
                (*ptr).refresh();
            } else if (w >> 16) as u32 == BN_CLICKED {
                (*ptr).command(w & 0xffff);
            }
            return 0;
        }
    }
    DefWindowProcW(hwnd, msg, w, l)
}
pub fn run() {
    unsafe {
        SetProcessDPIAware();
        let instance = GetModuleHandleW(null());
        let class = wide("KeycastWindowV1");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 810,
            bottom: 775,
        };
        AdjustWindowRect(&mut rect, style, 0);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("Keycast 0.1.1 | Wireless Keyboard").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        let app = Box::new(Ui::new(hwnd));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(app) as isize);
        ShowWindow(hwnd, SW_SHOW);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
