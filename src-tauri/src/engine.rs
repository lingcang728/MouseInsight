use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
    VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS,
    VK_PAUSE, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SCROLL, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, MSLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

const LLMHF_INJECTED: u32 = 0x0000_0001;
const LLMHF_LOWER_IL_INJECTED: u32 = 0x0000_0002;
const VK_MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xFF);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub id: String,
    pub button: String,
    pub mode: String,
    pub keys: Vec<String>,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    pub autostart: bool,
    pub paused: bool,
    pub mappings: Vec<Mapping>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            autostart: false,
            paused: false,
            mappings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Pulse {
    pub button: String,
    pub down: bool,
    pub t: u64,
    pub swallowed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub config: AppConfig,
    pub xmbc_running: bool,
    pub last: Option<Pulse>,
    pub listening: bool,
}

enum Cmd {
    Pulse(Pulse),
    Fire { button: String, down: bool },
    Record(Vec<String>),
    RecordCancel,
}

struct Engine {
    cfg: RwLock<AppConfig>,
    paused: AtomicBool,
    listening: AtomicBool,
    recording: AtomicBool,
    last: RwLock<Option<Pulse>>,
    tx: Sender<Cmd>,
    held: RwLock<HashSet<String>>,
    injected: RwLock<Vec<String>>,
    record_buf: RwLock<Vec<String>>,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static HOOK_TID: AtomicU32 = AtomicU32::new(0);

fn is_primary(button: &str) -> bool {
    button == "left" || button == "right"
}

fn engine() -> &'static Engine {
    ENGINE.get().expect("engine not started")
}

pub fn config_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

fn save_config(cfg: &AppConfig) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(config_path(), s);
    }
}

pub fn xmbc_running() -> bool {
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let mut pe = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    pe.szExeFile
                        .iter()
                        .take_while(|c| **c != 0)
                        .copied()
                        .collect::<Vec<_>>()
                        .as_slice(),
                );
                if name.eq_ignore_ascii_case("XMouseButtonControl.exe") {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

pub fn snapshot() -> Snapshot {
    let e = engine();
    Snapshot {
        config: e.cfg.read().clone(),
        xmbc_running: xmbc_running(),
        last: e.last.read().clone(),
        listening: e.listening.load(Ordering::Relaxed),
    }
}

pub fn set_mappings(mappings: Vec<Mapping>) {
    let e = engine();
    let mut cfg = e.cfg.write();
    cfg.mappings = mappings;
    save_config(&cfg);
}

pub fn set_theme(theme: String) {
    let e = engine();
    let mut cfg = e.cfg.write();
    cfg.theme = theme;
    save_config(&cfg);
}

pub fn set_paused(paused: bool) {
    let e = engine();
    e.paused.store(paused, Ordering::Relaxed);
    if paused {
        release_all();
    }
    let mut cfg = e.cfg.write();
    cfg.paused = paused;
    save_config(&cfg);
}

pub fn shutdown() {
    if let Some(e) = ENGINE.get() {
        e.paused.store(true, Ordering::Relaxed);
        e.recording.store(false, Ordering::Relaxed);
        e.listening.store(false, Ordering::Relaxed);
        release_all();
    }
    let tid = HOOK_TID.load(Ordering::Relaxed);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn set_autostart_flag(on: bool) {
    let e = engine();
    let mut cfg = e.cfg.write();
    cfg.autostart = on;
    save_config(&cfg);
}

pub fn arm_listen() {
    engine().listening.store(true, Ordering::Relaxed);
}

pub fn arm_record() {
    let e = engine();
    e.record_buf.write().clear();
    e.recording.store(true, Ordering::Relaxed);
}

pub fn disarm_record() {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
}

pub fn add_record_key(key: String) {
    let e = engine();
    {
        let mut buf = e.record_buf.write();
        if !buf.iter().any(|k| k == &key) {
            buf.push(key);
        }
    }
    let _ = e.tx.try_send(Cmd::Record(e.record_buf.read().clone()));
}

pub fn take_record_keys() -> Vec<String> {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
    e.record_buf.read().clone()
}

pub fn start(
    on_pulse: impl Fn(Pulse) + Send + 'static,
    on_listen: impl Fn(String) + Send + 'static,
    on_record: impl Fn(Vec<String>) + Send + 'static,
    on_record_cancel: impl Fn() + Send + 'static,
) {
    let (tx, rx) = unbounded::<Cmd>();
    let cfg = load_config();
    let paused = cfg.paused;
    let _ = ENGINE.set(Engine {
        paused: AtomicBool::new(paused),
        listening: AtomicBool::new(false),
        recording: AtomicBool::new(false),
        last: RwLock::new(None),
        cfg: RwLock::new(cfg),
        tx,
        held: RwLock::new(HashSet::new()),
        injected: RwLock::new(Vec::new()),
        record_buf: RwLock::new(Vec::new()),
    });

    thread::Builder::new()
        .name("mi-worker".into())
        .spawn(move || worker_loop(rx, on_pulse, on_listen, on_record, on_record_cancel))
        .expect("worker");

    thread::Builder::new()
        .name("mi-hook".into())
        .spawn(hook_loop)
        .expect("hook");
}

fn worker_loop(
    rx: Receiver<Cmd>,
    on_pulse: impl Fn(Pulse),
    on_listen: impl Fn(String),
    on_record: impl Fn(Vec<String>),
    on_record_cancel: impl Fn(),
) {
    let mut toggle_on: HashMap<String, bool> = HashMap::new();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Pulse(p) => {
                let e = engine();
                let mut captured = false;
                if e.listening.load(Ordering::Relaxed) && p.down {
                    e.listening.store(false, Ordering::Relaxed);
                    captured = true;
                    on_listen(p.button.clone());
                }
                *e.last.write() = Some(p.clone());
                if captured || !is_primary(&p.button) {
                    on_pulse(p);
                }
            }
            Cmd::Fire { button, down } => {
                apply_mapping(&button, down, &mut toggle_on);
            }
            Cmd::Record(keys) => on_record(keys),
            Cmd::RecordCancel => on_record_cancel(),
        }
    }
}

fn apply_mapping(button: &str, down: bool, toggle_on: &mut HashMap<String, bool>) {
    let e = engine();
    let cfg = e.cfg.read();
    let Some(m) = cfg.mappings.iter().find(|m| m.button == button) else {
        return;
    };
    if m.keys.is_empty() {
        return;
    }
    match m.mode.as_str() {
        "hold" => {
            if down {
                press_keys(&m.keys);
                e.held.write().insert(button.to_string());
            } else {
                release_keys(&m.keys);
                e.held.write().remove(button);
            }
        }
        "toggle" => {
            if !down {
                return;
            }
            let on = toggle_on.entry(button.to_string()).or_insert(false);
            if *on {
                release_keys(&m.keys);
                *on = false;
            } else {
                press_keys(&m.keys);
                *on = true;
            }
        }
        _ => {
            if down {
                press_keys(&m.keys);
                release_keys(&m.keys);
            }
        }
    }
}

fn hook_loop() {
    unsafe {
        HOOK_TID.store(GetCurrentThreadId(), Ordering::Relaxed);
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0)
            .expect("SetWindowsHookEx WH_MOUSE_LL");
        let kbd = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), None, 0)
            .expect("SetWindowsHookEx WH_KEYBOARD_LL");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(mouse);
        let _ = UnhookWindowsHookEx(kbd);
        release_all();
    }
}

const LLKHF_UP: u32 = 0x80;
const LLKHF_INJECTED_KBD: u32 = 0x10;

unsafe extern "system" fn kbd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if kb.flags.0 & LLKHF_INJECTED_KBD != 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let down = kb.flags.0 & LLKHF_UP == 0
        && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN);

    if down && (kb.vkCode == VK_PAUSE.0 as u32 || kb.vkCode == VK_SCROLL.0 as u32) {
        if let Some(e) = ENGINE.get() {
            e.paused.store(true, Ordering::Relaxed);
            e.recording.store(false, Ordering::Relaxed);
        }
        release_all();
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some(eng) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    if !eng.recording.load(Ordering::Relaxed) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if down {
        if kb.vkCode == VK_ESCAPE.0 as u32 {
            eng.recording.store(false, Ordering::Relaxed);
            let _ = eng.tx.try_send(Cmd::RecordCancel);
        } else if let Some(tok) = vk_to_token(kb.vkCode) {
            let mut buf = eng.record_buf.write();
            if !buf.iter().any(|k| k == &tok) {
                buf.push(tok);
            }
            let snap = buf.clone();
            drop(buf);
            let _ = eng.tx.try_send(Cmd::Record(snap));
        }
    }
    // Swallow while recording so Typeless / other global hotkeys cannot steal the chord.
    LRESULT(1)
}

fn vk_to_token(vk: u32) -> Option<String> {
    Some(match vk {
        0xA2 => "LControl".into(),
        0xA3 => "RControl".into(),
        0xA4 => "LAlt".into(),
        0xA5 => "RAlt".into(),
        0xA0 => "LShift".into(),
        0xA1 => "RShift".into(),
        0x5B => "LWin".into(),
        0x5C => "RWin".into(),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        0x08 => "Backspace".into(),
        0x2E => "Delete".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x25 => "ArrowLeft".into(),
        0x27 => "ArrowRight".into(),
        0x26 => "ArrowUp".into(),
        0x28 => "ArrowDown".into(),
        0xBD => "Minus".into(),
        0xBB => "Equal".into(),
        0xBC => "Comma".into(),
        0xBE => "Period".into(),
        0xBF => "Slash".into(),
        0xC0 => "Backquote".into(),
        0xDB => "BracketLeft".into(),
        0xDC => "Backslash".into(),
        0xDD => "BracketRight".into(),
        0xDE => "Quote".into(),
        0xBA => "Semicolon".into(),
        0x11 => "LControl".into(),
        0x12 => "LAlt".into(),
        0x10 => "LShift".into(),
        other if (0x41..=0x5A).contains(&other) => ((other as u8) as char).to_string(),
        other if (0x30..=0x39).contains(&other) => ((other as u8) as char).to_string(),
        other if (0x70..=0x87).contains(&other) => format!("F{}", other - 0x6F),
        _ => return None,
    })
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let Some(eng) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    if ms.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some((button, down, is_wheel)) = classify(wparam.0 as u32, ms.mouseData) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let paused = eng.paused.load(Ordering::Relaxed);
    let mapped = if paused {
        false
    } else {
        let cfg = eng.cfg.read();
        cfg.mappings
            .iter()
            .any(|m| m.button == button && !m.keys.is_empty())
    };

    // Never swallow left/right. Intercepting them locks every other window.
    let swallow = mapped && !paused && !is_primary(&button);
    let _ = eng.tx.try_send(Cmd::Pulse(Pulse {
        button: button.clone(),
        down,
        t,
        swallowed: swallow,
    }));
    if mapped && !paused {
        let _ = eng.tx.try_send(Cmd::Fire {
            button: button.clone(),
            down,
        });
        if is_wheel {
            let _ = eng.tx.try_send(Cmd::Fire {
                button,
                down: false,
            });
        }
    }
    if swallow {
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn classify(msg: u32, mouse_data: u32) -> Option<(String, bool, bool)> {
    let xhi = ((mouse_data >> 16) & 0xffff) as u16;
    match msg {
        WM_LBUTTONDOWN => Some(("left".into(), true, false)),
        WM_LBUTTONUP => Some(("left".into(), false, false)),
        WM_RBUTTONDOWN => Some(("right".into(), true, false)),
        WM_RBUTTONUP => Some(("right".into(), false, false)),
        WM_MBUTTONDOWN => Some(("middle".into(), true, false)),
        WM_MBUTTONUP => Some(("middle".into(), false, false)),
        WM_XBUTTONDOWN if xhi == 1 => Some(("xbutton1".into(), true, false)),
        WM_XBUTTONUP if xhi == 1 => Some(("xbutton1".into(), false, false)),
        WM_XBUTTONDOWN if xhi == 2 => Some(("xbutton2".into(), true, false)),
        WM_XBUTTONUP if xhi == 2 => Some(("xbutton2".into(), false, false)),
        WM_MOUSEWHEEL => {
            let delta = xhi as i16;
            if delta > 0 {
                Some(("wheelup".into(), true, true))
            } else {
                Some(("wheeldown".into(), true, true))
            }
        }
        WM_MOUSEHWHEEL => None,
        _ => None,
    }
}

struct KeySpec {
    vk: VIRTUAL_KEY,
    extended: bool,
}

fn key_spec(name: &str) -> Option<KeySpec> {
    let n = name.trim();
    let (vk, extended) = match n {
        "LControl" | "Control" | "Ctrl" => (VK_LCONTROL, false),
        "RControl" => (VK_RCONTROL, true),
        "LAlt" | "Alt" => (VK_LMENU, false),
        "RAlt" => (VK_RMENU, true),
        "LShift" | "Shift" => (VK_LSHIFT, false),
        "RShift" => (VK_RSHIFT, false),
        "LWin" | "Meta" | "Win" => (VK_LWIN, true),
        "RWin" => (VK_RWIN, true),
        "Space" => (VK_SPACE, false),
        "Enter" => (VK_RETURN, false),
        "Tab" => (VK_TAB, false),
        "Escape" | "Esc" => (VK_ESCAPE, false),
        "Backspace" => (VK_BACK, false),
        "Delete" => (VK_DELETE, true),
        "Insert" => (VK_INSERT, true),
        "Home" => (VK_HOME, true),
        "End" => (VK_END, true),
        "PageUp" => (VK_PRIOR, true),
        "PageDown" => (VK_NEXT, true),
        "ArrowLeft" => (VK_LEFT, true),
        "ArrowRight" => (VK_RIGHT, true),
        "ArrowUp" => (VK_UP, true),
        "ArrowDown" => (VK_DOWN, true),
        "Minus" | "-" => (VK_OEM_MINUS, false),
        "Equal" | "=" => (VK_OEM_PLUS, false),
        "Comma" | "," => (VK_OEM_COMMA, false),
        "Period" | "." => (VK_OEM_PERIOD, false),
        "Slash" | "/" => (VK_OEM_2, false),
        "Backquote" | "`" => (VK_OEM_3, false),
        "BracketLeft" | "[" => (VK_OEM_4, false),
        "Backslash" | "\\" => (VK_OEM_5, false),
        "BracketRight" | "]" => (VK_OEM_6, false),
        "Quote" | "'" => (VK_OEM_7, false),
        "Semicolon" | ";" => (VK_OEM_1, false),
        other if other.len() == 1 => {
            let c = other.chars().next()?.to_ascii_uppercase() as u8;
            if c.is_ascii_alphanumeric() {
                (VIRTUAL_KEY(c as u16), false)
            } else {
                return None;
            }
        }
        other if other.starts_with('F') => {
            if let Ok(n) = other[1..].parse::<u16>() {
                if (1..=24).contains(&n) {
                    (VIRTUAL_KEY(VK_F1.0 + n - 1), false)
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(KeySpec { vk, extended })
}

fn make_raw_input(vk: VIRTUAL_KEY, extended: bool, down: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) as u16 };
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn make_input(spec: &KeySpec, down: bool) -> INPUT {
    make_raw_input(spec.vk, spec.extended, down)
}

fn send_key(spec: &KeySpec, down: bool) {
    let input = make_input(spec, down);
    unsafe {
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn send_chord(keys: &[String], down: bool) {
    let specs: Vec<KeySpec> = if down {
        keys.iter().filter_map(|k| key_spec(k)).collect()
    } else {
        keys.iter().rev().filter_map(|k| key_spec(k)).collect()
    };
    if specs.is_empty() {
        return;
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(specs.len() * 2 + 2);

    if down {
        for s in &specs {
            inputs.push(make_input(s, true));
        }
    } else {
        let mut had_alt = false;
        let mut had_win = false;

        for s in &specs {
            inputs.push(make_input(s, false));
            match s.vk {
                VK_LMENU | VK_RMENU => {
                    had_alt = true;
                    inputs.push(make_raw_input(VK_MENU, false, false));
                }
                VK_LCONTROL | VK_RCONTROL => {
                    inputs.push(make_raw_input(VK_CONTROL, false, false));
                }
                VK_LSHIFT | VK_RSHIFT => {
                    inputs.push(make_raw_input(VK_SHIFT, false, false));
                }
                VK_LWIN | VK_RWIN => {
                    had_win = true;
                }
                _ => {}
            }
        }

        // Send unassigned mask key (0xFF) down + up to prevent Windows
        // from activating the active window's menu bar (SC_KEYMENU) or Start Menu.
        if had_alt || had_win {
            inputs.push(make_raw_input(VK_MASK_KEY, false, true));
            inputs.push(make_raw_input(VK_MASK_KEY, false, false));
        }
    }

    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn press_keys(keys: &[String]) {
    if let Some(e) = ENGINE.get() {
        e.injected.write().extend(keys.iter().cloned());
    }
    send_chord(keys, true);
}

fn release_keys(keys: &[String]) {
    send_chord(keys, false);
    if let Some(e) = ENGINE.get() {
        let mut inj = e.injected.write();
        for k in keys {
            if let Some(i) = inj.iter().rposition(|x| x == k) {
                inj.remove(i);
            }
        }
    }
}

fn force_release_all_modifiers() {
    let inputs = [
        make_raw_input(VK_LMENU, false, false),
        make_raw_input(VK_RMENU, true, false),
        make_raw_input(VK_MENU, false, false),
        make_raw_input(VK_LCONTROL, false, false),
        make_raw_input(VK_RCONTROL, true, false),
        make_raw_input(VK_CONTROL, false, false),
        make_raw_input(VK_LSHIFT, false, false),
        make_raw_input(VK_RSHIFT, false, false),
        make_raw_input(VK_SHIFT, false, false),
        make_raw_input(VK_LWIN, true, false),
        make_raw_input(VK_RWIN, true, false),
        make_raw_input(VK_MASK_KEY, false, true),
        make_raw_input(VK_MASK_KEY, false, false),
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn release_all() {
    let Some(e) = ENGINE.get() else {
        return;
    };
    let keys = {
        let mut inj = e.injected.write();
        let k = inj.clone();
        inj.clear();
        e.held.write().clear();
        k
    };
    for k in keys.iter().rev() {
        if let Some(spec) = key_spec(k) {
            send_key(&spec, false);
        }
    }
    force_release_all_modifiers();
}


