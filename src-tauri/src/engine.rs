use arc_swap::ArcSwap;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
#[cfg(any(target_os = "macos", test))]
use std::sync::atomic::AtomicI64;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(target_os = "windows")]
use windows::Win32::System::Power::{
    RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification, HPOWERNOTIFY,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0,
    INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MAPVK_VK_TO_VSC, MOD_NOREPEAT, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F1, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
    VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA,
    VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_PAUSE, VK_PRIOR, VK_RCONTROL,
    VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SPACE, VK_TAB, VK_UP,
};
#[cfg(target_os = "windows")]
use windows::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_MOUSE;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, KillTimer, PostThreadMessageW, RegisterClassW, RegisterDeviceNotificationW,
    SetTimer, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, UnregisterDeviceNotification,
    DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
    DEV_BROADCAST_DEVICEINTERFACE_W, HC_ACTION, HDEVNOTIFY, HWND_MESSAGE, KBDLLHOOKSTRUCT,
    MSG, MSLLHOOKSTRUCT, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DEVICECHANGE,
    WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_POWERBROADCAST, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
    WM_WTSSESSION_CHANGE, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW,
};

#[cfg(not(target_os = "windows"))]
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VIRTUAL_KEY(pub u16);

#[cfg(not(target_os = "windows"))]
pub const VK_BACK: VIRTUAL_KEY = VIRTUAL_KEY(0x08);
#[cfg(not(target_os = "windows"))]
pub const VK_TAB: VIRTUAL_KEY = VIRTUAL_KEY(0x09);
#[cfg(not(target_os = "windows"))]
pub const VK_RETURN: VIRTUAL_KEY = VIRTUAL_KEY(0x0D);
#[cfg(not(target_os = "windows"))]
pub const VK_ESCAPE: VIRTUAL_KEY = VIRTUAL_KEY(0x1B);
#[cfg(not(target_os = "windows"))]
pub const VK_SPACE: VIRTUAL_KEY = VIRTUAL_KEY(0x20);
#[cfg(not(target_os = "windows"))]
pub const VK_PRIOR: VIRTUAL_KEY = VIRTUAL_KEY(0x21);
#[cfg(not(target_os = "windows"))]
pub const VK_NEXT: VIRTUAL_KEY = VIRTUAL_KEY(0x22);
#[cfg(not(target_os = "windows"))]
pub const VK_END: VIRTUAL_KEY = VIRTUAL_KEY(0x23);
#[cfg(not(target_os = "windows"))]
pub const VK_HOME: VIRTUAL_KEY = VIRTUAL_KEY(0x24);
#[cfg(not(target_os = "windows"))]
pub const VK_LEFT: VIRTUAL_KEY = VIRTUAL_KEY(0x25);
#[cfg(not(target_os = "windows"))]
pub const VK_UP: VIRTUAL_KEY = VIRTUAL_KEY(0x26);
#[cfg(not(target_os = "windows"))]
pub const VK_RIGHT: VIRTUAL_KEY = VIRTUAL_KEY(0x27);
#[cfg(not(target_os = "windows"))]
pub const VK_DOWN: VIRTUAL_KEY = VIRTUAL_KEY(0x28);
#[cfg(not(target_os = "windows"))]
pub const VK_INSERT: VIRTUAL_KEY = VIRTUAL_KEY(0x2D);
#[cfg(not(target_os = "windows"))]
pub const VK_DELETE: VIRTUAL_KEY = VIRTUAL_KEY(0x2E);
#[cfg(not(target_os = "windows"))]
pub const VK_LWIN: VIRTUAL_KEY = VIRTUAL_KEY(0x5B);
#[cfg(not(target_os = "windows"))]
pub const VK_RWIN: VIRTUAL_KEY = VIRTUAL_KEY(0x5C);
#[cfg(not(target_os = "windows"))]
pub const VK_LSHIFT: VIRTUAL_KEY = VIRTUAL_KEY(0xA0);
#[cfg(not(target_os = "windows"))]
pub const VK_RSHIFT: VIRTUAL_KEY = VIRTUAL_KEY(0xA1);
#[cfg(not(target_os = "windows"))]
pub const VK_LCONTROL: VIRTUAL_KEY = VIRTUAL_KEY(0xA2);
#[cfg(not(target_os = "windows"))]
pub const VK_RCONTROL: VIRTUAL_KEY = VIRTUAL_KEY(0xA3);
#[cfg(not(target_os = "windows"))]
pub const VK_LMENU: VIRTUAL_KEY = VIRTUAL_KEY(0xA4);
#[cfg(not(target_os = "windows"))]
pub const VK_RMENU: VIRTUAL_KEY = VIRTUAL_KEY(0xA5);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_1: VIRTUAL_KEY = VIRTUAL_KEY(0xBA);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_PLUS: VIRTUAL_KEY = VIRTUAL_KEY(0xBB);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_COMMA: VIRTUAL_KEY = VIRTUAL_KEY(0xBC);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_MINUS: VIRTUAL_KEY = VIRTUAL_KEY(0xBD);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_PERIOD: VIRTUAL_KEY = VIRTUAL_KEY(0xBE);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_2: VIRTUAL_KEY = VIRTUAL_KEY(0xBF);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_3: VIRTUAL_KEY = VIRTUAL_KEY(0xC0);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_4: VIRTUAL_KEY = VIRTUAL_KEY(0xDB);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_5: VIRTUAL_KEY = VIRTUAL_KEY(0xDC);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_6: VIRTUAL_KEY = VIRTUAL_KEY(0xDD);
#[cfg(not(target_os = "windows"))]
pub const VK_OEM_7: VIRTUAL_KEY = VIRTUAL_KEY(0xDE);
#[cfg(not(target_os = "windows"))]
pub const VK_F1: VIRTUAL_KEY = VIRTUAL_KEY(0x70);

#[cfg(target_os = "windows")]
const LLMHF_INJECTED: u32 = 0x0000_0001;
#[cfg(target_os = "windows")]
const LLMHF_LOWER_IL_INJECTED: u32 = 0x0000_0002;
#[cfg(target_os = "windows")]
const LLKHF_UP: u32 = 0x80;
#[cfg(target_os = "windows")]
const LLKHF_INJECTED_KBD: u32 = 0x10;
#[cfg(target_os = "windows")]
const LLKHF_LOWER_IL_INJECTED_KBD: u32 = 0x02;
pub const EXTRA_INFO: usize = 0x4D49_484B;
pub const VK_MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xFC);
const TAP_QUEUE_CAP: usize = 32;
const EDGE_CHANNEL_CAP: usize = 256;
const HOLD_THRESHOLD: Duration = Duration::from_millis(400);
#[cfg(target_os = "windows")]
const HOTKEY_PAUSE: i32 = 1;
#[cfg(target_os = "windows")]
const HOTKEY_SCROLL: i32 = 2;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    XButton1 = 3,
    XButton2 = 4,
    WheelUp = 5,
    WheelDown = 6,
}

impl MouseButton {
    pub const COUNT: usize = 7;

    #[inline(always)]
    pub fn is_primary(self) -> bool {
        matches!(self, MouseButton::Left | MouseButton::Right)
    }

    #[inline(always)]
    pub fn is_wheel(self) -> bool {
        matches!(self, MouseButton::WheelUp | MouseButton::WheelDown)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::XButton1 => "xbutton1",
            MouseButton::XButton2 => "xbutton2",
            MouseButton::WheelUp => "wheelup",
            MouseButton::WheelDown => "wheeldown",
        }
    }

    pub fn from_str_fast(s: &str) -> Option<Self> {
        match s {
            "left" => Some(MouseButton::Left),
            "right" => Some(MouseButton::Right),
            "middle" => Some(MouseButton::Middle),
            "xbutton1" => Some(MouseButton::XButton1),
            "xbutton2" => Some(MouseButton::XButton2),
            "wheelup" => Some(MouseButton::WheelUp),
            "wheeldown" => Some(MouseButton::WheelDown),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerMode {
    Hold,
    Click,
    Toggle,
    Dual,
}

impl TriggerMode {
    pub fn from_str_fast(s: &str) -> Self {
        match s {
            "click" => TriggerMode::Click,
            "toggle" => TriggerMode::Toggle,
            "dual" => TriggerMode::Dual,
            _ => TriggerMode::Hold,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeySpec {
    pub vk: VIRTUAL_KEY,
    pub extended: bool,
}

impl std::hash::Hash for KeySpec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.vk.0.hash(state);
        self.extended.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledAction {
    pub mapping_id: String,
    pub mode: TriggerMode,
    pub specs: Vec<KeySpec>,
    pub tap_specs: Vec<KeySpec>,
    pub hold_specs: Vec<KeySpec>,
}

#[derive(Clone, Default, Debug)]
pub struct CompiledMappings {
    pub slots: [Option<Arc<CompiledAction>>; MouseButton::COUNT],
    pub generation: u64,
}

impl CompiledMappings {
    #[inline(always)]
    pub fn get(&self, btn: MouseButton) -> Option<&Arc<CompiledAction>> {
        self.slots[btn as usize].as_ref()
    }
}

#[allow(dead_code)]
pub fn compile_mappings(mappings: &[Mapping]) -> CompiledMappings {
    compile_mappings_with_gen(mappings, 0)
}

fn specs_from_names(names: &[String]) -> Vec<KeySpec> {
    let mut specs = Vec::new();
    for name in normalize_key_chord(names) {
        // Skip unknown tokens instead of dropping the whole chord; token
        // contents are deliberately not logged (privacy rule).
        match key_spec(&name) {
            Some(spec) => {
                if !specs.contains(&spec) { specs.push(spec); }
            }
            None => log::warn!("skipped an unsupported key token in a chord"),
        }
    }
    specs
}

pub fn compile_mappings_with_gen(mappings: &[Mapping], generation: u64) -> CompiledMappings {
    let mut slots: [Option<Arc<CompiledAction>>; MouseButton::COUNT] = Default::default();
    for m in mappings {
        if let Some(btn) = MouseButton::from_str_fast(&m.button) {
            if btn.is_primary() {
                continue;
            }
            let idx = btn as usize;
            if slots[idx].is_some() {
                continue;
            }

            let tap_names = if m.tap_keys.is_empty() {
                Vec::new()
            } else {
                m.tap_keys.clone()
            };
            let hold_names = if m.hold_keys.is_empty() {
                Vec::new()
            } else {
                m.hold_keys.clone()
            };

            let mut mode = TriggerMode::from_str_fast(&m.mode);
            if btn.is_wheel() {
                mode = TriggerMode::Click;
            }

            let (mode, tap_specs, hold_specs, specs) = if btn.is_wheel() {
                let names = if !tap_names.is_empty() {
                    tap_names
                } else {
                    m.keys.clone()
                };
                let specs = specs_from_names(&names);
                (TriggerMode::Click, specs.clone(), Vec::new(), specs)
            } else if mode == TriggerMode::Toggle {
                let specs = specs_from_names(&m.keys);
                (TriggerMode::Toggle, Vec::new(), Vec::new(), specs)
            } else {
                let mut tap = specs_from_names(&tap_names);
                let mut hold = specs_from_names(&hold_names);
                if tap.is_empty() && hold.is_empty() {
                    let legacy = specs_from_names(&m.keys);
                    if mode == TriggerMode::Hold {
                        hold = legacy;
                    } else {
                        tap = legacy;
                    }
                }
                if !tap.is_empty() && !hold.is_empty() {
                    (TriggerMode::Dual, tap.clone(), hold.clone(), tap)
                } else if !hold.is_empty() {
                    (TriggerMode::Hold, Vec::new(), hold.clone(), hold)
                } else {
                    (TriggerMode::Click, tap.clone(), Vec::new(), tap)
                }
            };

            if specs.is_empty() && tap_specs.is_empty() && hold_specs.is_empty() {
                continue;
            }
            slots[idx] = Some(Arc::new(CompiledAction {
                mapping_id: m.id.clone(),
                mode,
                specs,
                tap_specs,
                hold_specs,
            }));
        }
    }
    CompiledMappings { slots, generation }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub id: String,
    pub button: String,
    pub mode: String,
    pub keys: Vec<String>,
    #[serde(default)]
    pub tap_keys: Vec<String>,
    #[serde(default)]
    pub hold_keys: Vec<String>,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub mappings: Vec<Mapping>,
    // Forward compatibility: unknown top-level keys written by newer
    // versions round-trip instead of being dropped on the next save.
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_schema_version() -> u32 {
    1
}

fn default_theme() -> String {
    "light".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: "light".into(),
            autostart: false,
            paused: false,
            mappings: Vec::new(),
            extra: Default::default(),
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
    pub is_portable: bool,
    pub config_dir: String,
    pub emergency_hotkeys: u8,
    pub active_bindings: Vec<RuntimeBindingState>,
    pub recovery_notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SendReport {
    pub timestamp: u64,
    pub expected: u32,
    pub inserted: u32,
    pub win32_error: u32,
    pub is_uipi_blocked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeBindingState {
    pub mapping_id: String,
    pub button: String,
    pub mode: TriggerMode,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetReason {
    UserPause,
    EmergencyStop,
    ConfigChanged,
    Shutdown,
    SessionChanged,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Down,
    Up,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InjectedRecord {
    pub spec: KeySpec,
    pub action: KeyAction,
    pub is_mask: bool,
}

pub trait InputInjector: Send + 'static {
    #[allow(dead_code)]
    fn send_key(&mut self, spec: &KeySpec, down: bool) -> Result<(), SendReport> {
        self.send_keys(std::slice::from_ref(spec), down)
    }
    fn send_keys(&mut self, specs: &[KeySpec], down: bool) -> Result<(), SendReport>;
    fn send_mask(&mut self) -> Result<(), SendReport>;
    fn is_physical_down(&self, vk: VIRTUAL_KEY) -> bool;
    fn is_button_physically_down(&self, btn: MouseButton) -> bool;
    fn relinquish_key(&mut self, _spec: &KeySpec) {}
}

#[cfg(target_os = "windows")]
pub struct Win32Injector;

#[cfg(target_os = "windows")]
impl InputInjector for Win32Injector {
    fn send_keys(&mut self, specs: &[KeySpec], down: bool) -> Result<(), SendReport> {
        if specs.is_empty() {
            return Ok(());
        }
        let inputs: Vec<INPUT> = specs.iter().map(|s| make_input(s, down)).collect();
        let result = execute_send_inputs(&inputs);
        let inserted = match &result {
            Ok(n) => (*n as usize).min(specs.len()),
            Err(report) => (report.inserted as usize).min(specs.len()),
        };
        {
            let mut held = injected_held().lock();
            for s in &specs[..inserted] {
                if down {
                    held.insert(*s);
                } else {
                    held.remove(s);
                }
            }
        }
        result.map(|_| ())
    }

    fn send_mask(&mut self) -> Result<(), SendReport> {
        let inputs = [
            make_raw_input(VK_MASK_KEY, false, true),
            make_raw_input(VK_MASK_KEY, false, false),
        ];
        execute_send_inputs(&inputs).map(|_| ())
    }

    fn is_physical_down(&self, vk: VIRTUAL_KEY) -> bool {
        physical_down_set().read().contains(&(vk.0 as u32))
    }

    fn is_button_physically_down(&self, _btn: MouseButton) -> bool {
        // Verified on Windows (mi-probe): a button down swallowed by our
        // WH_MOUSE_LL hook never updates the async key state, so
        // GetAsyncKeyState reports it as released while the user still holds
        // it. Polling here would synthesize phantom releases; answer "held"
        // and let real events plus session resets clear state instead.
        true
    }
}

#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct FakeInjector {
    pub records: Arc<parking_lot::Mutex<Vec<InjectedRecord>>>,
    pub physical_held: Arc<parking_lot::Mutex<HashSet<u16>>>,
    pub batches: Arc<parking_lot::Mutex<Vec<usize>>>,
    pub buttons_up: Arc<parking_lot::Mutex<HashSet<MouseButton>>>,
    pub fail_after: Arc<parking_lot::Mutex<Option<usize>>>,
    pub fail_ups: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl FakeInjector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_physical_down(&self, vk: VIRTUAL_KEY, down: bool) {
        let mut held = self.physical_held.lock();
        if down {
            held.insert(vk.0);
        } else {
            held.remove(&vk.0);
        }
    }

    pub fn set_button_up(&self, btn: MouseButton, up: bool) {
        let mut ups = self.buttons_up.lock();
        if up {
            ups.insert(btn);
        } else {
            ups.remove(&btn);
        }
    }

    pub fn events(&self) -> Vec<InjectedRecord> {
        self.records.lock().clone()
    }

    pub fn clear(&self) {
        self.records.lock().clear();
        self.batches.lock().clear();
    }

    fn fake_report(expected: usize, inserted: usize) -> SendReport {
        SendReport {
            timestamp: 0,
            expected: expected as u32,
            inserted: inserted as u32,
            win32_error: 5,
            is_uipi_blocked: false,
        }
    }
}

impl InputInjector for FakeInjector {
    fn send_keys(&mut self, specs: &[KeySpec], down: bool) -> Result<(), SendReport> {
        if specs.is_empty() {
            return Ok(());
        }
        if !down && self.fail_ups.load(Ordering::Relaxed) {
            return Err(Self::fake_report(specs.len(), 0));
        }
        self.batches.lock().push(specs.len());
        let limit = if down {
            self.fail_after.lock().map(|n| n.min(specs.len()))
        } else {
            None
        };
        let effective = limit.unwrap_or(specs.len());
        let mut recs = self.records.lock();
        for spec in &specs[..effective] {
            recs.push(InjectedRecord {
                spec: *spec,
                action: if down { KeyAction::Down } else { KeyAction::Up },
                is_mask: false,
            });
        }
        drop(recs);
        if effective < specs.len() {
            return Err(Self::fake_report(specs.len(), effective));
        }
        Ok(())
    }

    fn send_mask(&mut self) -> Result<(), SendReport> {
        self.records.lock().push(InjectedRecord {
            spec: KeySpec {
                vk: VK_MASK_KEY,
                extended: false,
            },
            action: KeyAction::Down,
            is_mask: true,
        });
        self.records.lock().push(InjectedRecord {
            spec: KeySpec {
                vk: VK_MASK_KEY,
                extended: false,
            },
            action: KeyAction::Up,
            is_mask: true,
        });
        Ok(())
    }

    fn is_physical_down(&self, vk: VIRTUAL_KEY) -> bool {
        self.physical_held.lock().contains(&vk.0)
    }

    fn is_button_physically_down(&self, btn: MouseButton) -> bool {
        // Default is "held": the watchdog must stay inert in tests that do
        // not model physical button state explicitly.
        !self.buttons_up.lock().contains(&btn)
    }
}

#[derive(Clone, Debug)]
struct ActiveTap {
    mapping_id: String,
    specs: Vec<KeySpec>,
    due: Instant,
}

#[derive(Clone, Debug)]
struct QueuedTap {
    mapping_id: String,
    specs: Vec<KeySpec>,
}

#[derive(Clone, Debug)]
struct DualPending {
    mapping_id: String,
    tap_specs: Vec<KeySpec>,
    hold_specs: Vec<KeySpec>,
    due: Instant,
}

pub struct InputStateMachine<I: InputInjector> {
    pub injector: I,
    pub key_refs: HashMap<KeySpec, u32>,
    active_holds: HashMap<MouseButton, (String, Vec<KeySpec>)>,
    active_toggles: HashMap<MouseButton, (String, Vec<KeySpec>)>,
    tap_in_flight: HashMap<MouseButton, ActiveTap>,
    tap_queue: HashMap<MouseButton, VecDeque<QueuedTap>>,
    pending_dual: HashMap<MouseButton, DualPending>,
    pending_release: Vec<KeySpec>,
    last_toggle: HashMap<MouseButton, Instant>,
    current_generation: u64,
    paused: bool,
    tap_dwell: Duration,
    hold_threshold: Duration,
    on_state_change: Option<Box<dyn Fn(RuntimeBindingState) + Send + 'static>>,
    on_send_error: Option<Box<dyn Fn(SendReport) + Send + 'static>>,
}

impl<I: InputInjector> InputStateMachine<I> {
    pub fn new(injector: I) -> Self {
        Self {
            injector,
            key_refs: HashMap::new(),
            active_holds: HashMap::new(),
            active_toggles: HashMap::new(),
            tap_in_flight: HashMap::new(),
            tap_queue: HashMap::new(),
            pending_dual: HashMap::new(),
            pending_release: Vec::new(),
            last_toggle: HashMap::new(),
            current_generation: 0,
            paused: false,
            tap_dwell: Duration::from_millis(30),
            hold_threshold: HOLD_THRESHOLD,
            on_state_change: None,
            on_send_error: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_dwell(mut self, dwell: Duration) -> Self {
        self.tap_dwell = dwell;
        self
    }

    #[allow(dead_code)]
    pub fn with_hold_threshold(mut self, threshold: Duration) -> Self {
        self.hold_threshold = threshold;
        self
    }

    pub fn set_callbacks(
        &mut self,
        on_state_change: Option<Box<dyn Fn(RuntimeBindingState) + Send + 'static>>,
        on_send_error: Option<Box<dyn Fn(SendReport) + Send + 'static>>,
    ) {
        self.on_state_change = on_state_change;
        self.on_send_error = on_send_error;
    }

    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    #[allow(dead_code)]
    pub fn set_paused_raw(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.reset_all(ResetReason::UserPause);
        }
    }

    pub fn emergency_stop(&mut self) {
        self.paused = true;
        self.reset_all(ResetReason::EmergencyStop);
    }

    pub fn update_config(&mut self, generation: u64) {
        self.current_generation = generation;
        self.reset_all(ResetReason::ConfigChanged);
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.tap_in_flight
            .values()
            .map(|t| t.due)
            .chain(self.pending_dual.values().map(|t| t.due))
            .min()
    }

    fn emit_state_change(
        &self,
        mapping_id: String,
        button: MouseButton,
        mode: TriggerMode,
        active: bool,
    ) {
        if let Some(cb) = &self.on_state_change {
            cb(RuntimeBindingState {
                mapping_id,
                button: button.as_str().to_string(),
                mode,
                active,
            });
        }
    }

    pub fn acquire_specs(&mut self, specs: &[KeySpec]) {
        let mut to_press: Vec<KeySpec> = Vec::new();
        for s in specs {
            let count = self.key_refs.entry(*s).or_insert(0);
            *count += 1;
            if *count == 1 {
                to_press.push(*s);
            }
        }
        if !to_press.is_empty() {
            if let Err(report) = self.injector.send_keys(&to_press, true) {
                // Roll back refcount for keys that were never inserted so a
                // later release does not key-up something never pressed.
                let inserted = (report.inserted as usize).min(to_press.len());
                for s in &to_press[inserted..] {
                    if let Some(count) = self.key_refs.get_mut(s) {
                        *count = count.saturating_sub(1);
                    }
                    if self.key_refs.get(s).copied() == Some(0) {
                        self.key_refs.remove(s);
                    }
                }
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }
    }

    pub fn release_specs(&mut self, specs: &[KeySpec]) {
        let mut to_release: Vec<KeySpec> = Vec::new();
        let mut had_alt_or_win = false;
        for s in specs.iter().rev() {
            if let Some(count) = self.key_refs.get_mut(s) {
                if *count > 0 {
                    *count -= 1;
                }
                if *count == 0 {
                    // Sample physical state once; querying twice between the
                    // refcount check and the send decision raced physical input.
                    let physical = self.injector.is_physical_down(s.vk);
                    if physical {
                        self.injector.relinquish_key(s);
                    } else {
                        to_release.push(*s);
                        if is_alt_or_win(s.vk) {
                            had_alt_or_win = true;
                        }
                    }
                    self.key_refs.remove(s);
                }
            }
        }
        if !to_release.is_empty() {
            if let Err(report) = self.injector.send_keys(&to_release, false) {
                let inserted = (report.inserted as usize).min(to_release.len());
                self.pending_release.extend_from_slice(&to_release[inserted..]);
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }
        if had_alt_or_win {
            if let Err(report) = self.injector.send_mask() {
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }
    }

    fn fire_click(
        &mut self,
        mapping_id: String,
        button: MouseButton,
        specs: Vec<KeySpec>,
        at: Instant,
    ) {
        if self.tap_in_flight.contains_key(&button) {
            let q = self.tap_queue.entry(button).or_default();
            if q.len() < TAP_QUEUE_CAP {
                q.push_back(QueuedTap {
                    mapping_id,
                    specs,
                });
            }
            return;
        }
        self.acquire_specs(&specs);
        self.tap_in_flight.insert(
            button,
            ActiveTap {
                mapping_id: mapping_id.clone(),
                specs,
                due: at + self.tap_dwell,
            },
        );
        self.emit_state_change(mapping_id, button, TriggerMode::Click, true);
    }

    pub fn tick(&mut self, now: Instant) {
        // Retry key-ups that failed earlier; SendInput can transiently refuse
        // under UIPI and the keys would otherwise stay held forever.
        if !self.pending_release.is_empty() {
            let pending = std::mem::take(&mut self.pending_release);
            if let Err(report) = self.injector.send_keys(&pending, false) {
                let inserted = (report.inserted as usize).min(pending.len());
                self.pending_release.extend_from_slice(&pending[inserted..]);
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }

        let mut due_dual = Vec::new();
        for (btn, pending) in &self.pending_dual {
            if pending.due <= now {
                due_dual.push(*btn);
            }
        }
        for btn in due_dual {
            if let Some(pending) = self.pending_dual.remove(&btn) {
                self.acquire_specs(&pending.hold_specs);
                self.active_holds.insert(
                    btn,
                    (pending.mapping_id.clone(), pending.hold_specs),
                );
                self.emit_state_change(pending.mapping_id, btn, TriggerMode::Hold, true);
            }
        }

        let mut expired_buttons = Vec::new();
        for (btn, tap) in &self.tap_in_flight {
            if tap.due <= now {
                expired_buttons.push(*btn);
            }
        }

        for btn in expired_buttons {
            if let Some(tap) = self.tap_in_flight.remove(&btn) {
                self.release_specs(&tap.specs);
                self.emit_state_change(tap.mapping_id, btn, TriggerMode::Click, false);

                // P2-eng-28: drop empty queues so tap_queue does not grow
                // one dead entry per button ever tapped.
                let next = self
                    .tap_queue
                    .get_mut(&btn)
                    .and_then(|q| q.pop_front());
                if let Some(next) = next {
                    self.acquire_specs(&next.specs);
                    self.tap_in_flight.insert(
                        btn,
                        ActiveTap {
                            mapping_id: next.mapping_id.clone(),
                            specs: next.specs.clone(),
                            due: now + self.tap_dwell,
                        },
                    );
                    self.emit_state_change(next.mapping_id, btn, TriggerMode::Click, true);
                }
                if self.tap_queue.get(&btn).is_some_and(|q| q.is_empty()) {
                    self.tap_queue.remove(&btn);
                }
            }
        }
    }

    /// Physical-button watchdog: a swallowed down whose release never reached
    /// the hook (focus steal, RDP, device glitch) would hold the mapped keys
    /// forever. Poll real state and synthesize the missing up.
    pub fn tick_watchdog(&mut self, now: Instant) -> Vec<MouseButton> {
        let candidates: Vec<MouseButton> = self
            .active_holds
            .keys()
            .chain(self.pending_dual.keys())
            .copied()
            .collect();
        let mut released = Vec::new();
        for btn in candidates {
            if !self.injector.is_button_physically_down(btn) {
                self.handle_mouse_edge(btn, false, None, self.current_generation, now, now);
                released.push(btn);
            }
        }
        released
    }

    fn needs_watchdog_poll(&self) -> bool {
        !self.active_holds.is_empty()
            || !self.pending_dual.is_empty()
            || !self.pending_release.is_empty()
    }

    pub fn handle_mouse_edge(
        &mut self,
        button: MouseButton,
        down: bool,
        action: Option<Arc<CompiledAction>>,
        generation: u64,
        now: Instant,
        at: Instant,
    ) {
        self.tick(now);

        if self.paused {
            if !down {
                self.pending_dual.remove(&button);
                if let Some((mapping_id, specs)) = self.active_holds.remove(&button) {
                    self.release_specs(&specs);
                    self.emit_state_change(mapping_id, button, TriggerMode::Hold, false);
                }
            }
            return;
        }

        if down {
            if generation < self.current_generation {
                return;
            }
            let Some(action) = action else {
                return;
            };
            match action.mode {
                TriggerMode::Hold => {
                    if self.active_holds.contains_key(&button) {
                        return;
                    }
                    self.acquire_specs(&action.specs);
                    self.active_holds
                        .insert(button, (action.mapping_id.clone(), action.specs.clone()));
                    self.emit_state_change(
                        action.mapping_id.clone(),
                        button,
                        TriggerMode::Hold,
                        true,
                    );
                }
                TriggerMode::Click => {
                    self.fire_click(
                        action.mapping_id.clone(),
                        button,
                        action.specs.clone(),
                        at,
                    );
                }
                TriggerMode::Dual => {
                    if self.pending_dual.contains_key(&button)
                        || self.active_holds.contains_key(&button)
                    {
                        return;
                    }
                    self.pending_dual.insert(
                        button,
                        DualPending {
                            mapping_id: action.mapping_id.clone(),
                            tap_specs: action.tap_specs.clone(),
                            hold_specs: action.hold_specs.clone(),
                            due: at + self.hold_threshold,
                        },
                    );
                }
                TriggerMode::Toggle => {
                    // Debounce: a bouncing switch or a double-fed edge must not
                    // flip a latched toggle back off within a few ms.
                    if self
                        .last_toggle
                        .get(&button)
                        .is_some_and(|t| at.saturating_duration_since(*t) < Duration::from_millis(60))
                    {
                        return;
                    }
                    self.last_toggle.insert(button, at);
                    if let Some((mapping_id, specs)) = self.active_toggles.remove(&button) {
                        self.release_specs(&specs);
                        self.emit_state_change(mapping_id, button, TriggerMode::Toggle, false);
                    } else {
                        self.acquire_specs(&action.specs);
                        self.active_toggles
                            .insert(button, (action.mapping_id.clone(), action.specs.clone()));
                        self.emit_state_change(
                            action.mapping_id.clone(),
                            button,
                            TriggerMode::Toggle,
                            true,
                        );
                    }
                }
            }
        } else {
            if let Some(pending) = self.pending_dual.remove(&button) {
                self.fire_click(pending.mapping_id, button, pending.tap_specs, at);
                return;
            }
            if let Some((mapping_id, specs)) = self.active_holds.remove(&button) {
                self.release_specs(&specs);
                self.emit_state_change(mapping_id, button, TriggerMode::Hold, false);
            }
        }
    }

    pub fn reset_all(&mut self, _reason: ResetReason) {
        let holds: Vec<(MouseButton, (String, Vec<KeySpec>))> = self.active_holds.drain().collect();
        for (btn, (id, specs)) in holds {
            self.release_specs(&specs);
            self.emit_state_change(id, btn, TriggerMode::Hold, false);
        }

        let toggles: Vec<(MouseButton, (String, Vec<KeySpec>))> =
            self.active_toggles.drain().collect();
        for (btn, (id, specs)) in toggles {
            self.release_specs(&specs);
            self.emit_state_change(id, btn, TriggerMode::Toggle, false);
        }

        let taps: Vec<(MouseButton, ActiveTap)> = self.tap_in_flight.drain().collect();
        for (btn, tap) in taps {
            self.release_specs(&tap.specs);
            self.emit_state_change(tap.mapping_id, btn, TriggerMode::Click, false);
        }

        self.tap_queue.clear();
        self.pending_dual.clear();

        let mut had_alt_or_win = false;
        let leftover_keys: Vec<(KeySpec, u32)> = self.key_refs.drain().collect();
        let mut to_release: Vec<KeySpec> = Vec::new();
        for (spec, count) in leftover_keys {
            if count > 0 && !self.injector.is_physical_down(spec.vk) {
                to_release.push(spec);
                if is_alt_or_win(spec.vk) {
                    had_alt_or_win = true;
                }
            }
        }
        if !to_release.is_empty() {
            if let Err(report) = self.injector.send_keys(&to_release, false) {
                let inserted = (report.inserted as usize).min(to_release.len());
                self.pending_release.extend_from_slice(&to_release[inserted..]);
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }
        if had_alt_or_win {
            if let Err(report) = self.injector.send_mask() {
                if let Some(cb) = &self.on_send_error {
                    cb(report);
                }
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct RecorderState {
    physical_held: HashSet<u32>,
    /// Tokens fed by press_record_key (the DOM-key fallback path when the
    /// low-level hook is unavailable). Tracked separately so key-up can
    /// remove them.
    token_held: HashSet<String>,
    max_chord: Vec<String>,
    chip_modifiers: HashSet<String>,
    last_emitted: Vec<String>,
}

impl RecorderState {
    pub(crate) fn reset(&mut self) {
        self.physical_held.clear();
        self.token_held.clear();
        self.max_chord.clear();
        self.chip_modifiers.clear();
        self.last_emitted.clear();
    }

    /// Same shape as on_key but keyed by token rather than hardware code —
    /// used by the DOM keydown/keyup fallback when the hook cannot see keys.
    pub(crate) fn on_token(&mut self, token: String, down: bool) -> Vec<String> {
        if !down {
            self.token_held.remove(&token);
            return Vec::new();
        }
        self.token_held.insert(token);
        let mut current: Vec<String> = self
            .physical_held
            .iter()
            .filter_map(|&code| vk_to_token(code))
            .collect();
        for t in &self.token_held {
            if !current.contains(t) {
                current.push(t.clone());
            }
        }
        for chip in &self.chip_modifiers {
            if !current.contains(chip) {
                current.push(chip.clone());
            }
        }
        let normalized = normalize_key_chord(&current);
        if normalized.len() >= self.max_chord.len() {
            self.max_chord = normalized.clone();
        }
        if self.max_chord == self.last_emitted {
            Vec::new()
        } else {
            self.last_emitted = self.max_chord.clone();
            self.max_chord.clone()
        }
    }

    pub(crate) fn on_key(&mut self, vk: u32, down: bool) -> Vec<String> {
        if down {
            self.physical_held.insert(vk);
            let mut current: Vec<String> = self
                .physical_held
                .iter()
                .filter_map(|&code| vk_to_token(code))
                .collect();
            for chip in &self.chip_modifiers {
                if !current.contains(chip) {
                    current.push(chip.clone());
                }
            }
            let normalized = normalize_key_chord(&current);
            if normalized.len() >= self.max_chord.len() {
                self.max_chord = normalized.clone();
            }
            if self.max_chord == self.last_emitted {
                Vec::new()
            } else {
                self.last_emitted = self.max_chord.clone();
                self.max_chord.clone()
            }
        } else {
            self.physical_held.remove(&vk);
            Vec::new()
        }
    }

    fn add_chip(&mut self, chip: String) -> Vec<String> {
        self.chip_modifiers.insert(chip);
        let mut current: Vec<String> = self
            .physical_held
            .iter()
            .filter_map(|&code| vk_to_token(code))
            .collect();
        for c in &self.chip_modifiers {
            if !current.contains(c) {
                current.push(c.clone());
            }
        }
        let normalized = normalize_key_chord(&current);
        if normalized.len() >= self.max_chord.len() {
            self.max_chord = normalized.clone();
        }
        if self.max_chord == self.last_emitted {
            Vec::new()
        } else {
            self.last_emitted = self.max_chord.clone();
            self.max_chord.clone()
        }
    }

    fn remove_token(&mut self, token: &str) {
        self.chip_modifiers.remove(token);
        self.token_held.remove(token);
        self.physical_held
            .retain(|vk| vk_to_token(*vk).as_deref() != Some(token));
        self.max_chord.retain(|key| key != token);
        self.last_emitted = self.max_chord.clone();
    }
}

fn modifier_weight(key: &str) -> u32 {
    match key {
        "LControl" | "RControl" | "Ctrl" | "Control" => 10,
        "LShift" | "RShift" | "Shift" => 20,
        "LAlt" | "RAlt" | "Alt" | "Option" | "LOption" | "ROption" => 30,
        "LWin" | "RWin" | "Win" | "Meta" | "LMeta" | "RMeta" | "Cmd" | "Command"
        | "LCommand" | "RCommand" => 40,
        _ => 100,
    }
}

pub fn normalize_key_chord(keys: &[String]) -> Vec<String> {
    let mut deduped: Vec<String> = Vec::new();
    for k in keys {
        if !deduped.contains(k) {
            deduped.push(k.clone());
        }
    }
    deduped.sort_by(|a, b| {
        let wa = modifier_weight(a);
        let wb = modifier_weight(b);
        if wa != wb {
            wa.cmp(&wb)
        } else {
            a.cmp(b)
        }
    });
    deduped
}

pub enum InputCmd {
    MouseEdge {
        button: MouseButton,
        down: bool,
        action: Option<Arc<CompiledAction>>,
        generation: u64,
        at: Instant,
    },
    ResetState(ResetReason),
    EmergencyStop {
        persist: bool,
    },
    UpdateMappings {
        generation: u64,
    },
    SetPaused(bool),
    ListenCaptured(String),
    Record(Vec<String>),
    RecordCancel,
}

pub(crate) struct Engine {
    pub(crate) cfg: RwLock<AppConfig>,
    pub(crate) hook_status: RwLock<String>,
    pub(crate) compiled: ArcSwap<CompiledMappings>,
    pub(crate) paused: AtomicBool,
    pub(crate) listening: AtomicBool,
    pub(crate) swallowed_buttons: AtomicU32,
    pub(crate) recording: AtomicBool,
    pub(crate) window_visible: AtomicBool,
    pub(crate) last: RwLock<Option<Pulse>>,
    pub(crate) cmd_tx: Sender<InputCmd>,
    pub(crate) edge_tx: Sender<InputCmd>,
    pub(crate) telem_tx: Sender<Pulse>,
    pub(crate) recorder: RwLock<RecorderState>,
    pub(crate) swallowed_keys: RwLock<HashSet<u32>>,
    pub(crate) active_bindings: RwLock<HashMap<String, RuntimeBindingState>>,
    pub(crate) recovery_notes: RwLock<Vec<String>>,
}

impl Engine {
    pub(crate) fn enqueue_edge(&self, edge: InputCmd) -> bool {
        if self.edge_tx.try_send(edge).is_ok() { return true; }
        // Never block a native hook or silently lose a key-up. Stop and release
        // on the priority control channel, retaining the bounded edge queue.
        if !self.paused.swap(true, Ordering::SeqCst) {
            request_emergency_stop_on(self, false);
        }
        false
    }
}

pub(crate) static ENGINE: OnceLock<Engine> = OnceLock::new();
#[allow(dead_code)]
static HOOK_TID: AtomicU32 = AtomicU32::new(0);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static SAVE_SEQ: AtomicU64 = AtomicU64::new(1);
static CFG_MUTATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static DIR_INFO: OnceLock<(PathBuf, bool)> = OnceLock::new();
static PHYSICAL_DOWN: OnceLock<RwLock<HashSet<u32>>> = OnceLock::new();
static WORKER_DONE: OnceLock<Mutex<Option<Receiver<()>>>> = OnceLock::new();
static INJECTED_HELD: OnceLock<parking_lot::Mutex<HashSet<KeySpec>>> = OnceLock::new();
static EMERGENCY_ACK: AtomicU64 = AtomicU64::new(0);
static EMERGENCY_SAVE_PENDING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static EMERGENCY_HOTKEYS: AtomicU8 = AtomicU8::new(0);
#[cfg(target_os = "windows")]
static HOOK_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
static WHEEL_ACC: AtomicI32 = AtomicI32::new(0);
static HOOK_STATUS_CB: OnceLock<Box<dyn Fn(String) + Send + Sync>> = OnceLock::new();
#[cfg(target_os = "windows")]
static XMBC_CACHE: OnceLock<parking_lot::Mutex<Option<(Instant, bool)>>> = OnceLock::new();

pub(crate) fn physical_down_set() -> &'static RwLock<HashSet<u32>> {
    PHYSICAL_DOWN.get_or_init(|| RwLock::new(HashSet::new()))
}

pub(crate) fn injected_held() -> &'static parking_lot::Mutex<HashSet<KeySpec>> {
    INJECTED_HELD.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

/// Release every key the injector still believes is held, without going
/// through the worker thread. Last-resort path for a stuck worker.
pub(crate) fn failsafe_release_all() {
    let held: Vec<KeySpec> = injected_held().lock().drain().collect();
    if held.is_empty() {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let inputs: Vec<INPUT> = held.iter().map(|s| make_input(s, false)).collect();
        if let Err(report) = execute_send_inputs(&inputs) {
            log::error!(
                "failsafe release partially failed: inserted {}/{} err={}",
                report.inserted,
                report.expected,
                report.win32_error
            );
        }
        if held.iter().any(|s| is_alt_or_win(s.vk)) {
            let mask = [
                make_raw_input(VK_MASK_KEY, false, true),
                make_raw_input(VK_MASK_KEY, false, false),
            ];
            let _ = execute_send_inputs(&mask);
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::post_key_ups(&held);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = held;
    }
}

/// A session lock/unlock, suspend/resume or device removal can sever physical
/// button state from what our hooks observed (the up is eaten by the session
/// transition). Swallowed downs never appear in GetAsyncKeyState, so these
/// notifications are the recovery signal on Windows.
#[cfg(target_os = "windows")]
pub(crate) fn session_reset() {
    if let Some(e) = ENGINE.get() {
        let _ = e
            .cmd_tx
            .send(InputCmd::ResetState(ResetReason::SessionChanged));
        e.swallowed_buttons.store(0, Ordering::Relaxed);
        log::info!("session/power/device change; released held state");
    }
}

/// Respawn the platform hook loop after a recoverable failure, e.g. the user
/// just granted Accessibility permission on macOS. No-op while a hook loop is
/// already running.
pub fn retry_hook() {
    #[cfg(target_os = "windows")]
    {
        if HOOK_TID.load(Ordering::Relaxed) == 0 {
            thread::spawn(hook_loop);
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::retry_hook();
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn session_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // Any wParam (lock, unlock, disconnect, reconnect) is a boundary
        // where held-button bookkeeping can go stale.
        WM_WTSSESSION_CHANGE => session_reset(),
        WM_POWERBROADCAST
            if matches!(
                wparam.0 as u32,
                PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND
            ) =>
        {
            session_reset();
        }
        WM_DEVICECHANGE if wparam.0 as u32 == DBT_DEVICEREMOVECOMPLETE => session_reset(),
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}

pub(crate) fn request_emergency_stop(persist: bool) {
    if let Some(e) = ENGINE.get() {
        request_emergency_stop_on(e, persist);
    }
}

pub(crate) fn request_emergency_stop_on(e: &Engine, persist: bool) {
    e.paused.store(true, Ordering::SeqCst);
    e.recording.store(false, Ordering::Relaxed);
    e.listening.store(false, Ordering::Relaxed);
    e.recorder.write().reset();
    let before = EMERGENCY_ACK.load(Ordering::SeqCst);
    let _ = e.cmd_tx.send(InputCmd::EmergencyStop { persist });
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        if EMERGENCY_ACK.load(Ordering::SeqCst) <= before {
            log::error!("worker 未在 300ms 内响应急停，已独立释放注入键");
            failsafe_release_all();
        }
    });
}

/// Serializes every config mutation; disk IO happens under this lock and
/// outside the cfg RwLock.
fn cfg_mutate_lock() -> &'static Mutex<()> {
    CFG_MUTATE_LOCK.get_or_init(|| Mutex::new(()))
}

/// Notes produced before the engine exists (directory resolution failures)
/// get drained into `load_config`'s recovery notes.
static EARLY_NOTES: OnceLock<parking_lot::Mutex<Vec<String>>> = OnceLock::new();

fn early_notes() -> &'static parking_lot::Mutex<Vec<String>> {
    EARLY_NOTES.get_or_init(|| parking_lot::Mutex::new(Vec::new()))
}

fn engine() -> &'static Engine {
    ENGINE.get().expect("engine not started")
}

pub fn set_window_visible(visible: bool) {
    if let Some(e) = ENGINE.get() {
        e.window_visible.store(visible, Ordering::Relaxed);
    }
}

fn portable_config_dir(exe_dir: &std::path::Path) -> Option<PathBuf> {
    let data = exe_dir.join("data");
    if data.join(".portable").is_file() {
        Some(data)
    } else if exe_dir.join(".portable").is_file() || exe_dir.join("portable").is_file() {
        // Existing portable installations remain readable without moving files.
        Some(exe_dir.to_path_buf())
    } else {
        None
    }
}

// Directory resolution runs once: portable markers and env vars are sampled
// at startup, never re-polled on every call.
fn resolve_dir() -> (PathBuf, bool) {
    let exe_dir = match std::env::current_exe() {
        Ok(p) => p
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
        Err(err) => {
            log::warn!("current_exe failed: {err}");
            early_notes()
                .lock()
                .push("未找到用户数据目录，配置将保存在程序目录".into());
            PathBuf::from(".")
        }
    };

    if let Some(data) = portable_config_dir(&exe_dir) {
        return (data, true);
    }

    #[cfg(target_os = "windows")]
    {
        match std::env::var_os("APPDATA") {
            Some(appdata) => (PathBuf::from(appdata).join("MouseInsight"), false),
            None => {
                log::warn!("APPDATA is not set; falling back to the program directory");
                early_notes()
                    .lock()
                    .push("未找到用户数据目录，配置将保存在程序目录".into());
                (exe_dir, false)
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Installed .app bundles are not writable; keep user data out of /Applications.
        match std::env::var_os("HOME") {
            Some(home) => (
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("MouseInsight"),
                false,
            ),
            None => {
                log::warn!("HOME is not set; falling back to the program directory");
                early_notes()
                    .lock()
                    .push("未找到用户数据目录，配置将保存在程序目录".into());
                (exe_dir, false)
            }
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        (exe_dir, false)
    }
}

pub fn is_portable_mode() -> bool {
    DIR_INFO.get_or_init(resolve_dir).1
}

pub fn config_dir() -> PathBuf {
    DIR_INFO.get_or_init(resolve_dir).0.clone()
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn config_bak_path() -> PathBuf {
    config_dir().join("config.json.bak")
}

/// Lenient config parse: a torn write or a hand edit must not lose the whole
/// file. Salvages every well-formed field, skips broken mappings, preserves
/// unknown top-level keys, and reports what it dropped via `notes`.
fn parse_config_lenient(text: &str) -> Result<(AppConfig, Vec<String>), String> {
    let text = text.trim_start_matches('\u{feff}');
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("config is not valid JSON: {e}"))?;
    let Some(obj) = value.as_object() else {
        return Err("config root is not an object".into());
    };

    let mut notes = Vec::new();
    let mut cfg = AppConfig::default();

    if let Some(v) = obj.get("schema_version") {
        // Never reject on version; saturate instead of failing.
        cfg.schema_version = v.as_u64().map(|n| n.min(u32::MAX as u64) as u32).unwrap_or(1);
    }
    if let Some(v) = obj.get("theme") {
        match v.as_str() {
            Some("light") | Some("dark") => cfg.theme = v.as_str().unwrap().to_string(),
            _ => {
                notes.push("theme 值无法识别，已重置为浅色".into());
            }
        }
    }
    cfg.autostart = obj.get("autostart").and_then(|v| v.as_bool()).unwrap_or(false);
    cfg.paused = obj.get("paused").and_then(|v| v.as_bool()).unwrap_or(false);

    match obj.get("mappings") {
        Some(serde_json::Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                match serde_json::from_value::<Mapping>(item.clone()) {
                    Ok(m) => cfg.mappings.push(m),
                    Err(_) => notes.push(format!("映射 #{i} 格式无效，已跳过")),
                }
            }
        }
        Some(_) => notes.push("mappings 字段格式无效，已忽略".into()),
        None => {}
    }

    // Chords containing keys this build cannot map still load; the offending
    // tokens are skipped at compile time (names deliberately not logged).
    for m in &cfg.mappings {
        let unsupported = [&m.keys, &m.tap_keys, &m.hold_keys]
            .iter()
            .flat_map(|chord| chord.iter())
            .any(|k| key_spec(k).is_none());
        if unsupported {
            notes.push(format!("映射「{}」含当前系统不支持的按键，已忽略该键", m.button));
        }
    }

    const KNOWN: [&str; 5] = ["schema_version", "theme", "autostart", "paused", "mappings"];
    for (k, v) in obj {
        if !KNOWN.contains(&k.as_str()) {
            cfg.extra.insert(k.clone(), v.clone());
        }
    }
    Ok((cfg, notes))
}

/// Keep at most 5 corrupted copies; names carry a unix timestamp so lexic
/// order is chronological for the next decade.
fn preserve_corrupted_config(path: &PathBuf) -> Option<String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("config.json.corrupted.{ts}");
    let dest = config_dir().join(&name);
    if let Err(err) = fs::copy(path, &dest) {
        log::warn!("failed to preserve corrupted config: {err}");
        return None;
    }
    log::warn!("preserved corrupted config at {}", dest.display());
    let mut copies: Vec<PathBuf> = fs::read_dir(config_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("config.json.corrupted."))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    copies.sort();
    while copies.len() > 5 {
        let _ = fs::remove_file(copies.remove(0));
    }
    Some(name)
}

fn load_config() -> (AppConfig, Vec<String>) {
    let dir = config_dir();
    // Sweep temp files left behind by interrupted saves.
    if let Ok(rd) = fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("config.json.") && name.ends_with(".tmp") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let mut notes = std::mem::take(&mut *early_notes().lock());

    let path = config_path();
    let mut resolved: Option<AppConfig> = None;
    if let Ok(text) = fs::read_to_string(&path) {
        match parse_config_lenient(&text) {
            Ok((cfg, parse_notes)) => {
                notes.extend(parse_notes);
                resolved = Some(cfg);
            }
            Err(err) => log::warn!("config.json 解析失败：{err}"),
        }
    }
    if resolved.is_none() {
        let bak = config_bak_path();
        if let Ok(text) = fs::read_to_string(&bak) {
            match parse_config_lenient(&text) {
                Ok((cfg, parse_notes)) => {
                    notes.extend(parse_notes);
                    notes.push("主配置文件不可用，已从 config.json.bak 恢复".into());
                    log::warn!("restored config from config.json.bak");
                    resolved = Some(cfg);
                }
                Err(err) => log::warn!("config.json.bak 解析失败：{err}"),
            }
        }
    }
    let cfg = match resolved {
        Some(cfg) => cfg,
        None => {
            if path.exists() {
                if let Some(name) = preserve_corrupted_config(&path) {
                    notes.push(format!("配置文件已损坏，已重置为默认；原文件保留为 {name}"));
                }
            }
            AppConfig::default()
        }
    };

    // Writability probe: a read-only dir fails every later save.
    let _ = fs::create_dir_all(&dir);
    let probe = dir.join(".write-test");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
        }
        Err(_) => notes.push(format!(
            "配置目录不可写（{}），修改将无法保存。请把程序移到可写目录或以便携模式运行",
            dir.display()
        )),
    }

    (cfg, notes)
}

// Callers hold CFG_MUTATE_LOCK; save_config itself does not lock.
fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;

    // Always stamp the current schema version on write.
    let mut out = cfg.clone();
    out.schema_version = 2;
    let json = serde_json::to_string_pretty(&out)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;

    let seq = SAVE_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_path = dir.join(format!("config.json.{pid}.{seq}.tmp"));
    let file_path = config_path();
    let bak_path = config_bak_path();

    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| format!("Failed to create tmp config: {e}"))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write tmp config: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync tmp config to disk: {e}"))?;
    }

    if file_path.exists() {
        // A hostile or leftover symlink at the .bak path would redirect the
        // copy target; delete it instead of copying through it.
        if let Ok(meta) = fs::symlink_metadata(&bak_path) {
            if meta.file_type().is_symlink() {
                let _ = fs::remove_file(&bak_path);
            }
        }
        if let Err(err) = fs::copy(&file_path, &bak_path) {
            log::warn!("config backup copy failed: {err}");
        }
    }

    if let Err(err) = replace_file_atomic(&tmp_path, &file_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    let _ = fs::remove_file(&tmp_path);
    Ok(())
}

fn replace_file_atomic(from: &PathBuf, to: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        let src: Vec<u16> = from.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let dst: Vec<u16> = to.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let ok = unsafe {
            MoveFileExW(
                PCWSTR(src.as_ptr()),
                PCWSTR(dst.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok.is_ok() {
            return Ok(());
        }
        let err = unsafe { GetLastError() };
        return Err(match err.0 {
            5 => "配置文件被占用或无权限（错误 5），请关闭正在打开它的编辑器/安全软件后重试".into(),
            32 => "配置文件正被其他程序占用（错误 32）".into(),
            _ => format!("MoveFileExW replace failed: {}", err.0),
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(from, to).map_err(|e| format!("rename failed: {e}"))?;
        // fsync the directory so the rename itself is durable.
        if let Some(dir) = to.parent() {
            if let Err(err) = fs::File::open(dir).and_then(|d| d.sync_all()) {
                log::warn!("config dir fsync failed: {err}");
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn xmbc_scan() -> Option<bool> {
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return None,
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
        Some(found)
    }
}

pub fn xmbc_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let cache = XMBC_CACHE.get_or_init(|| parking_lot::Mutex::new(None));
        let mut guard = cache.lock();
        if let Some((t, v)) = *guard {
            if t.elapsed() < Duration::from_secs(10) {
                return v;
            }
        }
        match xmbc_scan() {
            Some(found) => {
                *guard = Some((Instant::now(), found));
                found
            }
            None => {
                log::warn!("进程枚举失败，沿用上次 XMBC 检测结果");
                guard.map(|(_, v)| v).unwrap_or(false)
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn hook_status() -> String {
    ENGINE
        .get()
        .map(|e| e.hook_status.read().clone())
        .unwrap_or_else(|| "starting".into())
}

// Menu reads must not enumerate processes or resolve filesystem paths.
pub fn menu_mappings() -> Vec<Mapping> {
    ENGINE
        .get()
        .map(|e| e.cfg.read().mappings.clone())
        .unwrap_or_default()
}

pub fn menu_summary() -> (bool, usize, String) {
    let Some(e) = ENGINE.get() else {
        return (false, 0, "starting".into());
    };
    (
        e.paused.load(Ordering::SeqCst),
        // Count compilable slots, not raw rows: an all-empty mapping shows
        // no menu entries and must not count as active.
        compile_mappings(&e.cfg.read().mappings)
            .slots
            .iter()
            .filter(|s| s.is_some())
            .count(),
        e.hook_status.read().clone(),
    )
}

pub fn set_hook_status(status: impl Into<String>) {
    let status = status.into();
    if let Some(e) = ENGINE.get() {
        *e.hook_status.write() = status.clone();
    }
    if let Some(cb) = HOOK_STATUS_CB.get() {
        cb(status);
    }
    crate::native_menu::refresh();
}

/// Toggle from the state the menu was showing when the user clicked, so a
/// stale label can never flip an emergency pause back to active.
pub fn toggle_paused_from(shown_paused: bool) -> Result<(), String> {
    let target = !shown_paused;
    if engine().paused.load(Ordering::SeqCst) == target {
        return Ok(());
    }
    set_paused(target)
}

pub fn snapshot() -> Snapshot {
    let e = engine();
    Snapshot {
        config: e.cfg.read().clone(),
        xmbc_running: xmbc_running(),
        last: e.last.read().clone(),
        listening: e.listening.load(Ordering::Relaxed),
        is_portable: is_portable_mode(),
        config_dir: config_dir().display().to_string(),
        emergency_hotkeys: emergency_hotkeys_state(),
        active_bindings: e.active_bindings.read().values().cloned().collect(),
        recovery_notes: e.recovery_notes.read().clone(),
    }
}

/// The frontend acknowledged the recovery notes; stop reporting them.
pub fn clear_recovery_notes() {
    engine().recovery_notes.write().clear();
}

fn emergency_hotkeys_state() -> u8 {
    #[cfg(target_os = "windows")]
    {
        EMERGENCY_HOTKEYS.load(Ordering::Relaxed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

fn validate_mappings(mappings: &[Mapping]) -> Result<(), String> {
    if mappings.len() > 5 { return Err("最多支持 5 个鼠标按键映射".into()); }
    let mut buttons = HashSet::new();
    let mut ids = HashSet::new();
    for mapping in mappings {
        if mapping.id.is_empty()
            || mapping.id.len() > 64
            || !mapping
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err("映射编号无效".into());
        }
        if mapping.label.chars().count() > 64 {
            return Err("标签过长".into());
        }
        if !["hold", "click", "toggle", "dual"].contains(&mapping.mode.as_str()) {
            return Err("触发方式无效".into());
        }
        let button = MouseButton::from_str_fast(&mapping.button).ok_or("无法识别鼠标按键")?;
        if button.is_primary() || !buttons.insert(button) || !ids.insert(&mapping.id) {
            return Err("鼠标按键或映射编号重复，或尝试映射主键".into());
        }
        for chord in [&mapping.keys, &mapping.tap_keys, &mapping.hold_keys] {
            if chord.len() > 16 || chord.iter().any(|key| key_spec(key).is_none()) {
                return Err("组合键包含当前系统不支持的按键，或超过 16 键".into());
            }
        }
    }
    Ok(())
}

pub fn set_mappings(mappings: Vec<Mapping>) -> Result<(), String> {
    validate_mappings(&mappings)?;
    let e = engine();
    let _g = cfg_mutate_lock()
        .lock()
        .map_err(|_| "config save lock poisoned".to_string())?;
    let mut next = e.cfg.read().clone();
    next.mappings = mappings;
    save_config(&next)?;
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    e.compiled.store(Arc::new(compile_mappings_with_gen(&next.mappings, generation)));
    *e.cfg.write() = next;
    let _ = e.cmd_tx.send(InputCmd::UpdateMappings { generation });
    Ok(())
}

fn apply_quick_mapping(next: &mut AppConfig, button: &str, slot: &str, preset: &str) -> Result<(), String> {
    if !["xbutton1", "xbutton2", "middle", "wheelup", "wheeldown"].contains(&button)
        || !["tap", "hold"].contains(&slot)
        || (slot == "hold" && button.starts_with("wheel")) { return Err("不支持的按键操作".into()); }
    let modifier = if cfg!(target_os = "macos") { "LWin" } else { "LControl" };
    let keys: Vec<String> = match preset {
        "copy" => vec![modifier, "C"], "paste" => vec![modifier, "V"],
        "undo" => vec![modifier, "Z"], "enter" => vec!["Enter"], "clear" => vec![],
        _ => return Err("不支持的快捷操作".into()),
    }.into_iter().map(str::to_owned).collect();
    let index = match next.mappings.iter().position(|m| m.button == button) {
        Some(index) => index,
        None => {
            if keys.is_empty() { return Ok(()); }
            if next.mappings.len() >= 5 { return Err("最多支持 5 个鼠标按键映射".into()); }
            let mut suffix = 0;
            let id = loop {
                let candidate = format!("quick-{button}-{suffix}");
                if next.mappings.iter().all(|m| m.id != candidate) { break candidate; }
                suffix += 1;
            };
            next.mappings.push(Mapping { id, button: button.into(),
                mode: "dual".into(), keys: vec![], tap_keys: vec![], hold_keys: vec![], label: String::new() });
            next.mappings.len() - 1
        }
    };
    let m = &mut next.mappings[index];
    if m.mode == "toggle" { return Err("此按键正在使用切换保持，请先在按键工作台更改触发方式。".into()); }
    if m.tap_keys.is_empty() && m.hold_keys.is_empty() {
        if m.mode == "hold" { m.hold_keys = m.keys.clone(); } else { m.tap_keys = m.keys.clone(); }
    }
    m.mode = if button.starts_with("wheel") { "click" } else { "dual" }.into();
    if slot == "tap" { m.tap_keys = keys; } else { m.hold_keys = keys; }
    m.keys = if m.tap_keys.is_empty() { m.hold_keys.clone() } else { m.tap_keys.clone() };
    if m.keys.is_empty() && m.tap_keys.is_empty() && m.hold_keys.is_empty() {
        // Clearing the last slot removes the mapping row entirely.
        next.mappings.remove(index);
    }
    Ok(())
}

pub fn set_quick_mapping(button: &str, slot: &str, preset: &str) -> Result<(), String> {
    let e = engine();
    let _g = cfg_mutate_lock()
        .lock()
        .map_err(|_| "config save lock poisoned".to_string())?;
    let mut next = e.cfg.read().clone();
    apply_quick_mapping(&mut next, button, slot, preset)?;
    save_config(&next)?;
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    e.compiled.store(Arc::new(compile_mappings_with_gen(&next.mappings, generation)));
    *e.cfg.write() = next;
    let _ = e.cmd_tx.send(InputCmd::UpdateMappings { generation });
    Ok(())
}

pub fn set_theme(theme: String) -> Result<(), String> {
    if theme != "light" && theme != "dark" {
        return Err("主题无效".into());
    }
    let e = engine();
    let _g = cfg_mutate_lock()
        .lock()
        .map_err(|_| "config save lock poisoned".to_string())?;
    let mut next = e.cfg.read().clone();
    next.theme = theme;
    save_config(&next)?;
    *e.cfg.write() = next;
    Ok(())
}

pub fn set_paused(paused: bool) -> Result<(), String> {
    let e = engine();
    // Safety takes effect even when storage is unavailable.
    e.paused.store(paused, Ordering::SeqCst);
    let _ = e.cmd_tx.send(InputCmd::SetPaused(paused));
    let _g = cfg_mutate_lock()
        .lock()
        .map_err(|_| "config save lock poisoned".to_string())?;
    let mut next = e.cfg.read().clone();
    next.paused = paused;
    let save_result = save_config(&next);
    // Keep in-memory paused in sync with the engine even when the write fails,
    // so a later mutation doesn't persist the stale value back to disk.
    *e.cfg.write() = next;
    save_result.map_err(|err| {
        format!("暂停状态已生效，但未能写入配置文件（重启后将恢复为上次保存的状态）：{err}")
    })
}

pub fn set_autostart_flag(on: bool) -> Result<(), String> {
    let e = engine();
    let _g = cfg_mutate_lock()
        .lock()
        .map_err(|_| "config save lock poisoned".to_string())?;
    let mut next = e.cfg.read().clone();
    next.autostart = on;
    save_config(&next)?;
    *e.cfg.write() = next;
    Ok(())
}

/// Generation bumped on every arm so the frontend can tell whether the
/// session it started is still the live one.
static ARM_GEN: AtomicU64 = AtomicU64::new(0);
/// When `recording` was armed; the worker reaps sessions idle for >60s.
static RECORD_ARMED_AT: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);

pub fn disarm_listen() {
    engine().listening.store(false, Ordering::Relaxed);
}

pub fn arm_listen() -> u64 {
    engine().listening.store(true, Ordering::Relaxed);
    ARM_GEN.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn arm_record() -> u64 {
    let e = engine();
    {
        let mut rec = e.recorder.write();
        rec.reset();
        // Seed with keys already held so a chord pressed before the dialog
        // opened is still captured correctly.
        rec.physical_held = physical_down_set().read().iter().copied().collect();
    }
    e.listening.store(false, Ordering::Relaxed);
    e.recording.store(true, Ordering::Relaxed);
    *RECORD_ARMED_AT.lock() = Some(Instant::now());
    let _ = e.cmd_tx.send(InputCmd::ResetState(ResetReason::UserPause));
    ARM_GEN.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn disarm_record() {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
    e.recorder.write().reset();
}

pub fn add_record_key(key: String) {
    let e = engine();
    if !e.recording.load(Ordering::Relaxed) || key_spec(&key).is_none() { return; }
    let chord = e.recorder.write().add_chip(key);
    if !chord.is_empty() {
        let _ = e.cmd_tx.send(InputCmd::Record(chord));
    }
}

/// DOM keydown/keyup fallback for recording when the low-level hook is not
/// ready: the webview still sees the keys, so feed them as tokens.
pub fn press_record_key(key: String, down: bool) {
    let e = engine();
    if !e.recording.load(Ordering::Relaxed) || key_spec(&key).is_none() {
        return;
    }
    let chord = e.recorder.write().on_token(key, down);
    if !chord.is_empty() {
        let _ = e.cmd_tx.send(InputCmd::Record(chord));
    }
}

pub fn remove_record_key(key: String) {
    let e = engine();
    let mut rec = e.recorder.write();
    rec.remove_token(&key);
    let chord = rec.max_chord.clone();
    drop(rec);
    let _ = e.cmd_tx.send(InputCmd::Record(chord));
}

pub fn take_record_keys() -> Vec<String> {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
    let mut rec = e.recorder.write();
    let keys = rec.max_chord.clone();
    rec.reset();
    keys
}

pub fn shutdown() {
    if let Some(e) = ENGINE.get() {
        e.paused.store(true, Ordering::SeqCst);
        e.recording.store(false, Ordering::Relaxed);
        e.listening.store(false, Ordering::Relaxed);
        let _ = e.cmd_tx.send(InputCmd::ResetState(ResetReason::Shutdown));
    }
    #[cfg(target_os = "windows")]
    {
        let tid = HOOK_TID.load(Ordering::Relaxed);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::stop_hook();
    }
    if let Some(slot) = WORKER_DONE.get() {
        if let Ok(mut guard) = slot.lock() {
            if let Some(rx) = guard.take() {
                if rx.recv_timeout(Duration::from_millis(800)).is_err() {
                    log::error!("worker 未在 800ms 内退出，执行兜底释放");
                    failsafe_release_all();
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start(
    on_pulse: impl Fn(Pulse) + Send + 'static,
    on_listen: impl Fn(String) + Send + 'static,
    on_record: impl Fn(Vec<String>) + Send + 'static,
    on_record_cancel: impl Fn() + Send + 'static,
    on_emergency_pause: impl Fn(bool) + Send + 'static,
    on_send_error: impl Fn(SendReport) + Send + 'static,
    on_binding_state: impl Fn(RuntimeBindingState) + Send + 'static,
    on_fatal: impl Fn(String) + Send + 'static,
    on_hook_status: impl Fn(String) + Send + Sync + 'static,
) {
    if ENGINE.get().is_some() {
        return;
    }
    let _ = HOOK_STATUS_CB.set(Box::new(on_hook_status));

    let on_binding_state = move |state: RuntimeBindingState| {
        if let Some(e) = ENGINE.get() {
            let mut bindings = e.active_bindings.write();
            if state.active {
                bindings.insert(state.mapping_id.clone(), state.clone());
            } else {
                bindings.remove(&state.mapping_id);
            }
        }
        on_binding_state(state);
    };

    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<InputCmd>();
    let (edge_tx, edge_rx) = crossbeam_channel::bounded::<InputCmd>(EDGE_CHANNEL_CAP);
    let (telem_tx, telem_rx) = crossbeam_channel::bounded::<Pulse>(16);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    let _ = WORKER_DONE.set(Mutex::new(Some(done_rx)));

    let (cfg, recovery_notes) = load_config();
    let paused = cfg.paused;
    let gen = NEXT_GENERATION.load(Ordering::SeqCst);
    let compiled = compile_mappings_with_gen(&cfg.mappings, gen);

    if ENGINE
        .set(Engine {
            cfg: RwLock::new(cfg),
            hook_status: RwLock::new("starting".into()),
            compiled: ArcSwap::from_pointee(compiled),
            paused: AtomicBool::new(paused),
            listening: AtomicBool::new(false),
            swallowed_buttons: AtomicU32::new(0),
            recording: AtomicBool::new(false),
            window_visible: AtomicBool::new(false),
            last: RwLock::new(None),
            cmd_tx,
            edge_tx,
            telem_tx,
            recorder: RwLock::new(RecorderState::default()),
            swallowed_keys: RwLock::new(HashSet::new()),
            active_bindings: RwLock::new(HashMap::new()),
            recovery_notes: RwLock::new(recovery_notes),
        })
        .is_err()
    {
        return;
    }

    thread::Builder::new()
        .name("mi-telemetry".into())
        .spawn(move || {
            while let Ok(pulse) = telem_rx.recv() {
                on_pulse(pulse);
            }
        })
        .expect("telemetry thread");

    thread::Builder::new()
        .name("mi-worker".into())
        .spawn(move || {
            #[cfg(target_os = "windows")]
            let injector = Some(Win32Injector);
            #[cfg(target_os = "macos")]
            let injector = crate::macos::MacosInjector::new();
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            let injector = Some(FakeInjector::new());

            let Some(injector) = injector else {
                set_hook_status("macOS 事件源创建失败，映射不可用");
                log::error!("CGEventSource creation failed; engine worker not started");
                let _ = done_tx.send(());
                return;
            };

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker_loop(
                    cmd_rx,
                    edge_rx,
                    injector,
                    on_listen,
                    on_record,
                    on_record_cancel,
                    on_emergency_pause,
                    on_send_error,
                    on_binding_state,
                );
            }));
            if let Err(payload) = outcome {
                failsafe_release_all();
                set_hook_status("engine stopped");
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into());
                log::error!("engine worker panicked: {msg}");
                on_fatal(format!("映射引擎已停止：{msg}"));
            }
            let _ = done_tx.send(());
        })
        .expect("worker thread");

    #[cfg(target_os = "windows")]
    thread::Builder::new()
        .name("mi-hook".into())
        .spawn(hook_loop)
        .expect("hook thread");

    #[cfg(target_os = "macos")]
    thread::Builder::new()
        .name("mi-macos-tap".into())
        .spawn(crate::macos::hook_loop)
        .expect("macos tap thread");
}

#[allow(clippy::too_many_arguments)]
fn worker_loop<I: InputInjector>(
    ctrl_rx: Receiver<InputCmd>,
    edge_rx: Receiver<InputCmd>,
    injector: I,
    on_listen: impl Fn(String) + Send + 'static,
    on_record: impl Fn(Vec<String>) + Send + 'static,
    on_record_cancel: impl Fn() + Send + 'static,
    on_emergency_pause: impl Fn(bool) + Send + 'static,
    on_send_error: impl Fn(SendReport) + Send + 'static,
    on_binding_state: impl Fn(RuntimeBindingState) + Send + 'static,
) {
    let mut state_machine = InputStateMachine::new(injector);
    if let Some(e) = ENGINE.get() {
        state_machine.set_paused(e.paused.load(Ordering::SeqCst));
        state_machine.update_config(e.compiled.load().generation);
    }
    state_machine.set_callbacks(
        Some(Box::new(on_binding_state)),
        Some(Box::new(on_send_error)),
    );

    loop {
        let now = Instant::now();
        state_machine.tick(now);
        for btn in state_machine.tick_watchdog(now) {
            if let Some(e) = ENGINE.get() {
                e.swallowed_buttons
                    .fetch_and(!(1u32 << btn as u32), Ordering::Relaxed);
            }
        }

        // A recording session left armed (e.g. the window was killed without
        // disarming) would swallow every key forever — reap it after 60s.
        if let Some(e) = ENGINE.get() {
            if e.recording.load(Ordering::Relaxed)
                && RECORD_ARMED_AT
                    .lock()
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(60))
            {
                e.recording.store(false, Ordering::Relaxed);
                e.recorder.write().reset();
                on_record_cancel();
            }
        }

        let timeout = match state_machine.next_deadline() {
            Some(due) => {
                let now = Instant::now();
                if due <= now {
                    Duration::ZERO
                } else {
                    due - now
                }
            }
            None => Duration::MAX,
        };
        // Poll physical buttons at least once a second while anything is held
        // or owed to the OS; an armed recorder also needs the idle reaper.
        let recording = ENGINE
            .get()
            .is_some_and(|e| e.recording.load(Ordering::Relaxed));
        let timeout = if state_machine.needs_watchdog_poll() || recording {
            timeout.min(Duration::from_secs(1))
        } else {
            timeout
        };

        let mut sel = crossbeam_channel::Select::new_biased();
        let ctrl_idx = sel.recv(&ctrl_rx);
        let edge_idx = sel.recv(&edge_rx);
        let oper = if timeout == Duration::MAX {
            sel.select()
        } else {
            match sel.select_timeout(timeout) {
                Ok(oper) => oper,
                Err(_) => continue,
            }
        };

        let cmd = if oper.index() == ctrl_idx {
            match oper.recv(&ctrl_rx) {
                Ok(cmd) => cmd,
                Err(_) => {
                    state_machine.reset_all(ResetReason::Shutdown);
                    break;
                }
            }
        } else if oper.index() == edge_idx {
            match oper.recv(&edge_rx) {
                Ok(cmd) => cmd,
                Err(_) => {
                    log::error!("edge channel disconnected; engine worker exiting");
                    state_machine.reset_all(ResetReason::Shutdown);
                    break;
                }
            }
        } else {
            continue;
        };

        let now = Instant::now();
        match cmd {
            InputCmd::MouseEdge {
                button,
                down,
                action,
                generation,
                at,
            } => {
                if let Some(e) = ENGINE.get() {
                    let current = e.compiled.load().generation;
                    if current > state_machine.current_generation {
                        state_machine.update_config(current);
                    }
                    if down && (e.paused.load(Ordering::SeqCst) || e.recording.load(Ordering::Relaxed)) {
                        continue;
                    }
                }
                state_machine.handle_mouse_edge(button, down, action, generation, now, at);
            }
            InputCmd::ResetState(reason) => {
                state_machine.reset_all(reason);
                if let Some(e) = ENGINE.get() {
                    e.swallowed_buttons.store(0, Ordering::Relaxed);
                }
                if matches!(reason, ResetReason::Shutdown) {
                    break;
                }
            }
            InputCmd::EmergencyStop { persist } => {
                EMERGENCY_ACK.fetch_add(1, Ordering::SeqCst);
                if let Some(e) = ENGINE.get() {
                    e.paused.store(true, Ordering::SeqCst);
                    e.swallowed_buttons.store(0, Ordering::Relaxed);
                }
                state_machine.emergency_stop();
                on_record_cancel();
                on_emergency_pause(true);
                // Only user-triggered stops persist; watchdog/overflow stops
                // must not write paused=true or the next launch stays paused.
                if persist && !EMERGENCY_SAVE_PENDING.swap(true, Ordering::SeqCst) {
                    // Save the current config under the mutate lock, never an
                    // old snapshot.
                    thread::spawn(|| {
                        if let Some(e) = ENGINE.get() {
                            let _g = cfg_mutate_lock()
                                .lock()
                                .unwrap_or_else(|p| p.into_inner());
                            let mut next = e.cfg.read().clone();
                            next.paused = e.paused.load(Ordering::SeqCst);
                            if save_config(&next).is_ok() {
                                *e.cfg.write() = next;
                            }
                        }
                        EMERGENCY_SAVE_PENDING.store(false, Ordering::SeqCst);
                    });
                }
            }
            InputCmd::SetPaused(paused) => {
                state_machine.set_paused(paused);
                if paused {
                    if let Some(e) = ENGINE.get() {
                        e.swallowed_buttons.store(0, Ordering::Relaxed);
                    }
                }
                on_emergency_pause(paused);
            }
            InputCmd::UpdateMappings { generation } => {
                if generation > state_machine.current_generation {
                    state_machine.update_config(generation);
                }
            }
            InputCmd::ListenCaptured(btn) => {
                on_listen(btn);
            }
            InputCmd::Record(keys) => {
                on_record(keys);
            }
            InputCmd::RecordCancel => {
                on_record_cancel();
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn hook_loop() {
    unsafe {
        HOOK_TID.store(GetCurrentThreadId(), Ordering::Relaxed);
        // Both callbacks live in this executable; supply its module for global hooks.
        let module = match windows::Win32::System::LibraryLoader::GetModuleHandleW(None) {
            Ok(module) => windows::Win32::Foundation::HINSTANCE(module.0),
            Err(err) => {
                HOOK_TID.store(0, Ordering::Relaxed);
                log::error!("GetModuleHandleW failed: {err}");
                set_hook_status(&format!("Windows 监听初始化失败：{err}"));
                return;
            }
        };

        // Message-only window on this thread's queue: WTS session changes and
        // suspend/resume notifications are the recovery signal for stale held
        // state now that swallowed events are invisible to GetAsyncKeyState.
        let session_wnd = {
            let class_name = windows::core::w!("MouseInsightHookWnd");
            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(session_wndproc),
                hInstance: module,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassW(&wnd_class) == 0 {
                log::warn!("RegisterClassW failed; session notifications disabled");
                None
            } else {
                match CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    class_name,
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(module),
                    None,
                ) {
                    Ok(hwnd) => Some(hwnd),
                    Err(err) => {
                        log::warn!("hook window creation failed: {err}");
                        None
                    }
                }
            }
        };
        let mut power_notify: Option<HPOWERNOTIFY> = None;
        let mut dev_notify: Option<HDEVNOTIFY> = None;
        if let Some(hwnd) = session_wnd {
            if let Err(err) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
                log::warn!("WTSRegisterSessionNotification failed: {err}");
            }
            match RegisterSuspendResumeNotification(HANDLE(hwnd.0), DEVICE_NOTIFY_WINDOW_HANDLE) {
                Ok(h) => power_notify = Some(h),
                Err(err) => log::warn!("RegisterSuspendResumeNotification failed: {err}"),
            }
            // Directed device-interface notifications do reach message-only
            // windows (unlike the broadcast DBT_DEVICEREMOVECOMPLETE).
            let dev_filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
                dbcc_reserved: 0,
                dbcc_classguid: GUID_DEVINTERFACE_MOUSE,
                dbcc_name: [0],
            };
            match RegisterDeviceNotificationW(
                HANDLE(hwnd.0),
                &dev_filter as *const _ as *const core::ffi::c_void,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            ) {
                Ok(h) => dev_notify = Some(h),
                Err(err) => log::warn!("RegisterDeviceNotification failed: {err}"),
            }
        }

        let mut mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module), 0) {
            Ok(h) => h,
            Err(err) => {
                HOOK_TID.store(0, Ordering::Relaxed);
                log::error!("SetWindowsHookEx WH_MOUSE_LL failed: {err}");
                set_hook_status(&format!("Windows 鼠标监听启动失败：{err}。请检查安全软件拦截及其他鼠标工具。Windows 无需辅助功能授权。"));
                return;
            }
        };

        if RegisterHotKey(None, HOTKEY_PAUSE, MOD_NOREPEAT, VK_PAUSE.0 as u32).is_ok() {
            EMERGENCY_HOTKEYS.fetch_or(1, Ordering::Relaxed);
        } else {
            log::warn!("Pause 急停热键注册失败，键盘钩子内联兜底仍可用");
        }
        if RegisterHotKey(None, HOTKEY_SCROLL, MOD_NOREPEAT, VK_SCROLL.0 as u32).is_ok() {
            EMERGENCY_HOTKEYS.fetch_or(2, Ordering::Relaxed);
        } else {
            log::warn!("Scroll Lock 急停热键注册失败，键盘钩子内联兜底仍可用");
        }

        let mut kbd = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), Some(module), 0) {
            Ok(h) => h,
            Err(err) => {
                log::error!("SetWindowsHookEx WH_KEYBOARD_LL failed: {err}");
                set_hook_status(&format!("Windows 键盘监听启动失败：{err}。请检查安全软件拦截。"));
                let _ = UnhookWindowsHookEx(mouse);
                let _ = UnregisterHotKey(None, HOTKEY_PAUSE);
                let _ = UnregisterHotKey(None, HOTKEY_SCROLL);
                HOOK_TID.store(0, Ordering::Relaxed);
                return;
            }
        };

        // Seed modifiers already held before our hook went live so release
        // decisions do not send key-ups for physically held keys.
        {
            let mut held = physical_down_set().write();
            for vk in (0xA0u32..=0xA5).chain([0x5B, 0x5C]) {
                if GetAsyncKeyState(vk as i32) < 0 {
                    held.insert(vk);
                }
            }
        }

        // Heartbeat: a stripped hook looks exactly like idle. If no callback
        // ran for a full timer interval, reinstall both hooks. With a NULL
        // hwnd the return value is the timer id needed by KillTimer.
        let heartbeat = SetTimer(None, 0, 60_000, None);
        if heartbeat == 0 {
            log::warn!("SetTimer failed; hook heartbeat disabled");
        }

        set_hook_status("ready");
        let mut last_hook_events = HOOK_EVENTS.load(Ordering::Relaxed);
        let mut msg = MSG::default();
        loop {
            let status = GetMessageW(&mut msg, None, 0, 0);
            if status.0 == 0 {
                break;
            }
            if status.0 == -1 {
                set_hook_status(&format!("Windows 监听消息循环中断：{:?}，请重新启动应用。", GetLastError()));
                break;
            }

            match msg.message {
                WM_HOTKEY => {
                    request_emergency_stop(true);
                }
                WM_TIMER => {
                    let count = HOOK_EVENTS.load(Ordering::Relaxed);
                    if count == last_hook_events {
                        let _ = UnhookWindowsHookEx(kbd);
                        let _ = UnhookWindowsHookEx(mouse);
                        match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module), 0) {
                            Ok(h) => {
                                mouse = h;
                                log::debug!("hook heartbeat: reinstalled mouse hook");
                            }
                            Err(err) => {
                                log::error!("鼠标钩子重建失败：{err}");
                                set_hook_status(&format!("Windows 鼠标监听重建失败：{err}，请重新启动应用。"));
                            }
                        }
                        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), Some(module), 0) {
                            Ok(h) => {
                                kbd = h;
                                log::debug!("hook heartbeat: reinstalled keyboard hook");
                            }
                            Err(err) => {
                                log::error!("键盘钩子重建失败：{err}");
                                set_hook_status(&format!("Windows 键盘监听重建失败：{err}，请重新启动应用。"));
                            }
                        }
                    }
                    last_hook_events = count;
                }
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        let _ = KillTimer(None, heartbeat);
        let _ = UnhookWindowsHookEx(kbd);
        let _ = UnhookWindowsHookEx(mouse);
        let _ = UnregisterHotKey(None, HOTKEY_PAUSE);
        let _ = UnregisterHotKey(None, HOTKEY_SCROLL);
        if let Some(hwnd) = session_wnd {
            let _ = WTSUnRegisterSessionNotification(hwnd);
            if let Some(h) = power_notify {
                let _ = UnregisterSuspendResumeNotification(h);
            }
            if let Some(h) = dev_notify {
                let _ = UnregisterDeviceNotification(h);
            }
            let _ = DestroyWindow(hwnd);
        }
        if let Some(eng) = ENGINE.get() {
            let _ = eng.cmd_tx.send(InputCmd::ResetState(ResetReason::Shutdown));
        }
        HOOK_TID.store(0, Ordering::Relaxed);
    }
}

#[cfg(any(target_os = "windows", test))]
fn sided_windows_vk(vk: u32, scan: u32, extended: bool) -> u32 {
    match vk {
        0x12 => if extended { 0xA5 } else { 0xA4 },
        0x11 => if extended { 0xA3 } else { 0xA2 },
        0x10 => if scan == 0x36 { 0xA1 } else { 0xA0 },
        _ => vk,
    }
}

#[cfg(target_os = "windows")]
fn is_injected_key(kb: &KBDLLHOOKSTRUCT) -> bool {
    kb.flags.0 & (LLKHF_INJECTED_KBD | LLKHF_LOWER_IL_INJECTED_KBD) != 0
        || kb.dwExtraInfo == EXTRA_INFO
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn kbd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    HOOK_EVENTS.fetch_add(1, Ordering::Relaxed);
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if is_injected_key(kb) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let key = sided_windows_vk(kb.vkCode, kb.scanCode, kb.flags.0 & 1 != 0);
    let msg = wparam.0 as u32;
    let down = kb.flags.0 & LLKHF_UP == 0 && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN);

    {
        let mut held = physical_down_set().write();
        if down {
            held.insert(key);
        } else {
            held.remove(&key);
        }
    }

    let Some(eng) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    // Emergency stop keys work even while recording and are never swallowed:
    // the press must reach the system so a stuck state is still escapable.
    if down && (key == VK_PAUSE.0 as u32 || key == VK_SCROLL.0 as u32) {
        request_emergency_stop(true);
    }

    let is_key_event =
        msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN || msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let recording = eng.recording.load(Ordering::Relaxed);

    // A key-up whose down was swallowed during recording must stay swallowed,
    // otherwise the OS sees a release for a press it never received.
    if !down && is_key_event && eng.swallowed_keys.write().remove(&key) {
        if recording {
            let _ = eng.recorder.write().on_key(key, false);
        }
        return LRESULT(1);
    }

    if !recording {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if key == VK_TAB.0 as u32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if down && key == VK_ESCAPE.0 as u32 {
        eng.swallowed_keys.write().insert(key);
        eng.recording.store(false, Ordering::Relaxed);
        let _ = eng.cmd_tx.send(InputCmd::RecordCancel);
        return LRESULT(1);
    }

    if is_key_event {
        if vk_to_token(key).is_none() {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
        if down {
            eng.swallowed_keys.write().insert(key);
        }
        let chord = eng.recorder.write().on_key(key, down);
        if !chord.is_empty() {
            let _ = eng.cmd_tx.send(InputCmd::Record(chord));
        }
        // Key-ups of keys not swallowed on press pass through untouched.
        if down {
            return LRESULT(1);
        }
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn vk_to_token(vk: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::keycode_to_token(vk as u16)
    }
    #[cfg(not(target_os = "macos"))]
    {
        win_vk_to_token(vk)
    }
}

#[cfg(any(target_os = "windows", test))]
fn win_vk_to_token(vk: u32) -> Option<String> {
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
        0x60 => "Numpad0".into(),
        0x61 => "Numpad1".into(),
        0x62 => "Numpad2".into(),
        0x63 => "Numpad3".into(),
        0x64 => "Numpad4".into(),
        0x65 => "Numpad5".into(),
        0x66 => "Numpad6".into(),
        0x67 => "Numpad7".into(),
        0x68 => "Numpad8".into(),
        0x69 => "Numpad9".into(),
        0x6A => "NumpadMultiply".into(),
        0x6B => "NumpadAdd".into(),
        0x6D => "NumpadSubtract".into(),
        0x6E => "NumpadDecimal".into(),
        0x6F => "NumpadDivide".into(),
        _ => return None,
    })
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    HOOK_EVENTS.fetch_add(1, Ordering::Relaxed);
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let Some(eng) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    if ms.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0
        || ms.dwExtraInfo == EXTRA_INFO
    {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some((button, down)) = classify(wparam.0 as u32, ms.mouseData, &WHEEL_ACC) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    // Primary buttons never consume the listen session — they must keep their
    // system behavior so the user can still click; the frontend hints via the
    // telemetry pulse instead.
    let captured = down && !button.is_primary() && eng.listening.swap(false, Ordering::Relaxed);
    if captured {
        let _ = eng
            .cmd_tx
            .send(InputCmd::ListenCaptured(button.as_str().to_string()));
    }

    let paused = eng.paused.load(Ordering::Relaxed);
    let compiled = eng.compiled.load_full();
    let action_opt = compiled.get(button).cloned();
    let is_mapped = action_opt.is_some() && !paused && !captured && !eng.recording.load(Ordering::Relaxed);
    let mut queued = false;

    if is_mapped {
        queued = eng.enqueue_edge(InputCmd::MouseEdge {
            button,
            down,
            action: action_opt,
            generation: compiled.generation,
            at: Instant::now(),
        });
    } else if !down && !button.is_primary() && !button.is_wheel() {
        // Forward a release only when we actually swallowed the press;
        // otherwise it is a stray up that needs no state-machine bookkeeping.
        let bit = 1u32 << button as u32;
        if eng.swallowed_buttons.load(Ordering::Relaxed) & bit != 0 {
            let _ = eng.enqueue_edge(InputCmd::MouseEdge {
                button,
                down: false,
                action: None,
                generation: compiled.generation,
                at: Instant::now(),
            });
        }
    }

    let mut swallow = decide_swallow(button, down, is_mapped, captured, queued);
    if !button.is_primary() && !button.is_wheel() {
        let bit = 1u32 << button as u32;
        if down && swallow {
            eng.swallowed_buttons.fetch_or(bit, Ordering::Relaxed);
        } else if !down {
            swallow = eng.swallowed_buttons.fetch_and(!bit, Ordering::Relaxed) & bit != 0;
        }
    }

    let need_telemetry =
        eng.window_visible.load(Ordering::Relaxed) || eng.listening.load(Ordering::Relaxed);

    if need_telemetry {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let pulse = Pulse {
            button: button.as_str().to_string(),
            down,
            t,
            swallowed: swallow,
        };
        *eng.last.write() = Some(pulse.clone());
        let _ = eng.telem_tx.try_send(pulse);
    }

    if swallow {
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Pure swallow decision shared by the Windows and macOS hook paths. Primary
/// buttons are never swallowed; a mapped press is swallowed only once its edge
/// was actually queued for the worker (`queued` is ignored otherwise).
pub(crate) fn decide_swallow(
    button: MouseButton,
    down: bool,
    is_mapped: bool,
    captured: bool,
    queued: bool,
) -> bool {
    let _ = down;
    let mut swallow = (is_mapped || captured) && !button.is_primary();
    if is_mapped {
        swallow &= queued;
    }
    swallow
}

#[cfg(target_os = "windows")]
#[inline(always)]
fn classify(msg: u32, mouse_data: u32, acc: &AtomicI32) -> Option<(MouseButton, bool)> {
    let xhi = ((mouse_data >> 16) & 0xffff) as u16;
    match msg {
        WM_LBUTTONDOWN => Some((MouseButton::Left, true)),
        WM_LBUTTONUP => Some((MouseButton::Left, false)),
        WM_RBUTTONDOWN => Some((MouseButton::Right, true)),
        WM_RBUTTONUP => Some((MouseButton::Right, false)),
        WM_MBUTTONDOWN => Some((MouseButton::Middle, true)),
        WM_MBUTTONUP => Some((MouseButton::Middle, false)),
        WM_XBUTTONDOWN if xhi == 1 => Some((MouseButton::XButton1, true)),
        WM_XBUTTONUP if xhi == 1 => Some((MouseButton::XButton1, false)),
        WM_XBUTTONDOWN if xhi == 2 => Some((MouseButton::XButton2, true)),
        WM_XBUTTONUP if xhi == 2 => Some((MouseButton::XButton2, false)),
        WM_MOUSEWHEEL => wheel_step(xhi as i16, acc).map(|b| (b, true)),
        WM_MOUSEHWHEEL => None,
        _ => None,
    }
}

/// Accumulate sub-notch wheel deltas (precision touchpads, high-res wheels)
/// and emit one logical step per WHEEL_DELTA (120).
fn wheel_step(delta: i16, acc: &AtomicI32) -> Option<MouseButton> {
    if delta == 0 {
        return None;
    }
    let total = acc.fetch_add(delta as i32, Ordering::Relaxed) + delta as i32;
    if total >= 120 {
        acc.fetch_sub(120, Ordering::Relaxed);
        let rest = acc.load(Ordering::Relaxed);
        if rest.abs() >= 120 {
            log::debug!("wheel accumulator holds {rest} after emitting one step");
        }
        Some(MouseButton::WheelUp)
    } else if total <= -120 {
        acc.fetch_add(120, Ordering::Relaxed);
        let rest = acc.load(Ordering::Relaxed);
        if rest.abs() >= 120 {
            log::debug!("wheel accumulator holds {rest} after emitting one step");
        }
        Some(MouseButton::WheelDown)
    } else {
        None
    }
}

/// Continuous (trackpad) scroll deltas arrive as small fractional values at a
/// high rate; discrete wheel notches arrive as larger per-notch deltas. One
/// emitted step per `SCROLL_CONT_THRESHOLD` accumulated units, rate-limited so
/// a fast fling cannot flood the edge queue.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn scroll_step(
    continuous: bool,
    d1: i64,
    acc: &AtomicI64,
    now_ms: u64,
    last_ms: &AtomicU64,
) -> Option<MouseButton> {
    if d1 == 0 {
        return None;
    }
    if !continuous {
        return Some(if d1 > 0 {
            MouseButton::WheelUp
        } else {
            MouseButton::WheelDown
        });
    }
    let total = acc.fetch_add(d1, Ordering::Relaxed) + d1;
    if total.abs() < 40 {
        return None;
    }
    let last = last_ms.load(Ordering::Relaxed);
    if last != 0 && now_ms.saturating_sub(last) < 50 {
        // Suppress the burst but keep the accumulator so the step can fire
        // once the minimum interval has elapsed.
        return None;
    }
    last_ms.store(now_ms, Ordering::Relaxed);
    if total > 0 {
        acc.fetch_sub(40, Ordering::Relaxed);
        Some(MouseButton::WheelUp)
    } else {
        acc.fetch_add(40, Ordering::Relaxed);
        Some(MouseButton::WheelDown)
    }
}

fn key_spec(name: &str) -> Option<KeySpec> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::key_spec(name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        win_key_spec(name)
    }
}

#[cfg(any(target_os = "windows", test))]
fn win_key_spec(name: &str) -> Option<KeySpec> {
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
        "Numpad0" => (VIRTUAL_KEY(0x60), false),
        "Numpad1" => (VIRTUAL_KEY(0x61), false),
        "Numpad2" => (VIRTUAL_KEY(0x62), false),
        "Numpad3" => (VIRTUAL_KEY(0x63), false),
        "Numpad4" => (VIRTUAL_KEY(0x64), false),
        "Numpad5" => (VIRTUAL_KEY(0x65), false),
        "Numpad6" => (VIRTUAL_KEY(0x66), false),
        "Numpad7" => (VIRTUAL_KEY(0x67), false),
        "Numpad8" => (VIRTUAL_KEY(0x68), false),
        "Numpad9" => (VIRTUAL_KEY(0x69), false),
        "NumpadMultiply" => (VIRTUAL_KEY(0x6A), false),
        "NumpadAdd" => (VIRTUAL_KEY(0x6B), false),
        "NumpadSubtract" => (VIRTUAL_KEY(0x6D), false),
        "NumpadDecimal" => (VIRTUAL_KEY(0x6E), false),
        "NumpadDivide" => (VIRTUAL_KEY(0x6F), true),
        other if other.len() == 1 => {
            let c = other.chars().next()?.to_ascii_uppercase() as u8;
            if c.is_ascii_alphanumeric() {
                (VIRTUAL_KEY(c as u16), false)
            } else {
                return None;
            }
        }
        other if other.starts_with('F') => {
            if let Ok(num) = other[1..].parse::<u16>() {
                if (1..=24).contains(&num) {
                    (VIRTUAL_KEY(VK_F1.0 + num - 1), false)
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

#[cfg(target_os = "windows")]
#[inline(always)]
fn is_alt_or_win(vk: VIRTUAL_KEY) -> bool {
    matches!(vk, VK_LMENU | VK_RMENU | VK_MENU | VK_LWIN | VK_RWIN)
}

#[cfg(not(target_os = "windows"))]
#[inline(always)]
fn is_alt_or_win(_vk: VIRTUAL_KEY) -> bool {
    false
}

#[cfg(target_os = "windows")]
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
                dwExtraInfo: EXTRA_INFO,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn make_input(spec: &KeySpec, down: bool) -> INPUT {
    let mut input = make_raw_input(spec.vk, spec.extended, down);
    // Side-specific modifiers use their physical scan code. Generic VK_MENU
    // translation in target applications must not turn right Alt into left Alt.
    if (0xA0..=0xA5).contains(&spec.vk.0) {
        unsafe {
            input.Anonymous.ki.dwFlags |= KEYEVENTF_SCANCODE;
            input.Anonymous.ki.wVk = VIRTUAL_KEY(0);
        }
    }
    input
}

#[cfg(target_os = "windows")]
fn execute_send_inputs(inputs: &[INPUT]) -> Result<u32, SendReport> {
    if inputs.is_empty() {
        return Ok(0);
    }
    let expected = inputs.len() as u32;
    let inserted = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };

    if inserted == expected {
        Ok(inserted)
    } else {
        let err = unsafe { GetLastError() };
        let is_uipi = inserted == 0 && (err.0 == 5 || err.0 == 0);
        Err(SendReport {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            expected,
            inserted,
            win32_error: err.0,
            is_uipi_blocked: is_uipi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_action(id: &str, mode: TriggerMode, key_names: &[&str]) -> Arc<CompiledAction> {
        let specs: Vec<KeySpec> = key_names.iter().map(|k| win_key_spec(k).unwrap()).collect();
        Arc::new(CompiledAction {
            mapping_id: id.into(),
            mode,
            specs,
            tap_specs: Vec::new(),
            hold_specs: Vec::new(),
        })
    }

    #[test]
    fn test_normalize_key_chord() {
        let keys = vec![
            "K".into(),
            "LAlt".into(),
            "LControl".into(),
            "LShift".into(),
        ];
        let normalized = normalize_key_chord(&keys);
        assert_eq!(normalized, vec!["LControl", "LShift", "LAlt", "K"]);
    }

    #[test]
    fn test_mouse_button_conversion() {
        assert_eq!(
            MouseButton::from_str_fast("xbutton1"),
            Some(MouseButton::XButton1)
        );
        assert_eq!(
            MouseButton::from_str_fast("xbutton2"),
            Some(MouseButton::XButton2)
        );
        assert_eq!(
            MouseButton::from_str_fast("middle"),
            Some(MouseButton::Middle)
        );
        assert_eq!(MouseButton::XButton1.as_str(), "xbutton1");
    }

    #[test]
    fn test_compile_mappings_precompilation() {
        let mappings = vec![Mapping {
            id: "1".into(),
            button: "xbutton1".into(),
            mode: "hold".into(),
            keys: vec!["LControl".into(), "LAlt".into()],
            label: "".into(),
            tap_keys: vec![],
            hold_keys: vec![],
        }];
        let compiled = compile_mappings(&mappings);
        let action = compiled.get(MouseButton::XButton1).expect("compiled slot");
        assert_eq!(action.mode, TriggerMode::Hold);
        assert_eq!(action.specs.len(), 2);
    }

    // ==========================================
    // Hold 模式测试
    // ==========================================

    #[test]
    fn hold_down_press_once() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);

        let evs = injector.events();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[0].spec.vk, VK_LCONTROL);
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap(), 1);
    }

    #[test]
    fn hold_duplicate_down_does_not_double_press() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);

        let evs = injector.events();
        assert_eq!(
            evs.len(),
            1,
            "Duplicate down must not trigger another KeyDown"
        );
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap(), 1);
    }

    #[test]
    fn hold_up_releases_snapshot() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);
        sm.handle_mouse_edge(MouseButton::XButton1, false, None, 1, now, now);

        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert_eq!(evs[1].spec.vk, VK_LCONTROL);
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap_or(&0), 0);
    }

    #[test]
    fn hold_up_after_mapping_changed_releases_old_snapshot() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let old_act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let new_act = dummy_action("m1", TriggerMode::Click, &["Enter"]);
        let now = Instant::now();

        // 1. 按下旧配置 (Hold Ctrl)
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(old_act), 1, now, now);

        // 2. 配置发生变更，但由于按键依然物理按住，松开时传入新的 action 或 None
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(new_act), 2, now, now);

        // 验证：释放的仍然是旧配置的 Ctrl Up，而不是 Enter
        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[0].spec.vk, VK_LCONTROL);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert_eq!(evs[1].spec.vk, VK_LCONTROL);
    }

    #[test]
    fn hold_reset_releases_every_owned_key() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl", "LAlt"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);
        sm.reset_all(ResetReason::UserPause);

        let evs = injector.events();
        let non_mask_ups: Vec<_> = evs
            .iter()
            .filter(|e| !e.is_mask && e.action == KeyAction::Up)
            .collect();
        assert_eq!(non_mask_ups.len(), 2);
        assert!(sm.key_refs.is_empty() || sm.key_refs.values().all(|&v| v == 0));
    }

    // ==========================================
    // Click 模式测试
    // ==========================================

    #[test]
    fn click_down_emits_one_tap() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let act = dummy_action("m2", TriggerMode::Click, &["Enter"]);
        let t0 = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, t0, t0);
        assert_eq!(
            injector.events().len(),
            1,
            "Click down only emits KeyDown immediately"
        );
        assert_eq!(injector.events()[0].action, KeyAction::Down);

        // 未到达 deadline 前 tick
        sm.tick(t0 + Duration::from_millis(15));
        assert_eq!(injector.events().len(), 1);

        // 到达 deadline 后 tick
        sm.tick(t0 + dwell);
        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert_eq!(evs[1].spec.vk, VK_RETURN);
    }

    #[test]
    fn click_mouse_up_does_nothing() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let act = dummy_action("m2", TriggerMode::Click, &["Enter"]);
        let t0 = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            false,
            Some(act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );

        // 鼠标松开不应提前触发任何事件
        assert_eq!(injector.events().len(), 1);
        assert_eq!(injector.events()[0].action, KeyAction::Down);
    }

    #[test]
    fn click_dwell_does_not_block_other_hold_event() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let click_act = dummy_action("m2", TriggerMode::Click, &["Enter"]);
        let hold_act = dummy_action("m1", TriggerMode::Hold, &["LAlt"]);
        let t0 = Instant::now();

        // 1. Click 启动
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(click_act), 1, t0, t0);

        // 2. 5ms 后收到另一个键的 Hold
        sm.handle_mouse_edge(
            MouseButton::XButton2,
            true,
            Some(hold_act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );

        // 验证：Hold 的 LAlt 立即被发出，完全没有被 Click 的 dwell 阻塞！
        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].spec.vk, VK_RETURN);
        assert_eq!(evs[1].spec.vk, VK_LMENU);
        assert_eq!(evs[1].action, KeyAction::Down);
    }

    #[test]
    fn two_fast_clicks_produce_two_taps() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let act = dummy_action("m2", TriggerMode::Click, &["Enter"]);
        let t0 = Instant::now();

        // 快速连续两次 click down (间隔 10ms)
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            true,
            Some(act),
            1,
            t0 + Duration::from_millis(10),
            t0 + Duration::from_millis(10),
        );

        // 此时仅发出第一次 Down
        assert_eq!(injector.events().len(), 1);

        // 第 1 次 tap 到期 (t0 + 30ms)
        sm.tick(t0 + dwell);
        let evs = injector.events();
        assert_eq!(evs.len(), 3, "Should have [Down1, Up1, Down2]");
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert_eq!(evs[2].action, KeyAction::Down);

        // 第 2 次 tap 到期 (t0 + 60ms)
        sm.tick(t0 + dwell * 2);
        let evs = injector.events();
        assert_eq!(evs.len(), 4, "Should have [Down1, Up1, Down2, Up2]");
        assert_eq!(evs[3].action, KeyAction::Up);
    }

    #[test]
    fn click_shared_modifier_does_not_release_hold_modifier() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let hold_act = dummy_action("m1", TriggerMode::Hold, &["LControl", "LAlt"]);
        let click_act = dummy_action("m2", TriggerMode::Click, &["LControl", "Enter"]);
        let t0 = Instant::now();

        // 1. Hold Ctrl+Alt
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, t0, t0);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LAlt").unwrap()).unwrap(), 1);

        // 2. Click Ctrl+Enter
        sm.handle_mouse_edge(
            MouseButton::XButton2,
            true,
            Some(click_act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 2);
        assert_eq!(*sm.key_refs.get(&win_key_spec("Enter").unwrap()).unwrap(), 1);

        // 3. Click 到期释放
        sm.tick(t0 + Duration::from_millis(35));

        // 验证：Enter 释放了，但是 LControl 的 refs 依然是 1，绝不能发出 LControl Up！
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);
        let ctrl_ups: Vec<_> = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_LCONTROL && e.action == KeyAction::Up)
            .collect();
        assert!(
            ctrl_ups.is_empty(),
            "Hold's LControl must NOT be released by Click!"
        );

        // 4. Hold 松开
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            false,
            Some(hold_act),
            1,
            t0 + Duration::from_millis(50),
            t0 + Duration::from_millis(50),
        );
        assert_eq!(
            *sm.key_refs
                .get(&win_key_spec("LControl").unwrap())
                .unwrap_or(&0),
            0
        );
        let final_ctrl_ups: Vec<_> = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_LCONTROL && e.action == KeyAction::Up)
            .collect();
        assert_eq!(
            final_ctrl_ups.len(),
            1,
            "LControl released only when Hold is released"
        );
    }

    #[test]
    fn click_shared_modifier_does_not_release_toggle_modifier() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let toggle_act = dummy_action("m1", TriggerMode::Toggle, &["LControl"]);
        let click_act = dummy_action("m2", TriggerMode::Click, &["LControl", "Enter"]);
        let t0 = Instant::now();

        // 1. Toggle Ctrl 开启
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(toggle_act), 1, t0, t0);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);

        // 2. Click Ctrl+Enter
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            true,
            Some(click_act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 2);

        // 3. Click 到期释放
        sm.tick(t0 + Duration::from_millis(35));
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);
        let ctrl_ups: Vec<_> = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_LCONTROL && e.action == KeyAction::Up)
            .collect();
        assert!(
            ctrl_ups.is_empty(),
            "Toggle's LControl must NOT be released by Click!"
        );
    }

    // ==========================================
    // Toggle 模式测试
    // ==========================================

    #[test]
    fn toggle_first_down_acquires() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, now, now);

        let evs = injector.events();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[0].spec.vk, VK_LSHIFT);
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap(), 1);
    }

    #[test]
    fn toggle_up_does_nothing() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act.clone()), 1, now, now);
        sm.handle_mouse_edge(MouseButton::Middle, false, Some(act), 1, now, now);

        let evs = injector.events();
        assert_eq!(evs.len(), 1, "Toggle Up does not release key");
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap(), 1);
    }

    #[test]
    fn toggle_second_down_releases() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act.clone()), 1, now, now);
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(act),
            1,
            now,
            now + Duration::from_millis(100),
        );

        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].action, KeyAction::Down);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert_eq!(*sm.key_refs.get(&evs[0].spec).unwrap_or(&0), 0);
    }

    #[test]
    fn toggle_reset_releases() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, now, now);
        sm.reset_all(ResetReason::UserPause);

        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert!(sm.active_toggles.is_empty());
    }

    #[test]
    fn toggle_config_change_releases() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, now, now);
        sm.update_config(2);

        let evs = injector.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].action, KeyAction::Up);
        assert!(sm.active_toggles.is_empty());
    }

    #[test]
    fn toggle_runtime_status_on_off_is_emitted() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector);
        let states = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let st_clone = states.clone();
        sm.set_callbacks(
            Some(Box::new(move |s| {
                st_clone.lock().push(s);
            })),
            None,
        );

        let act = dummy_action("m_toggle", TriggerMode::Toggle, &["LShift"]);
        let now = Instant::now();

        // 第一次 Down -> active: true
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act.clone()), 1, now, now);
        // 第二次 Down -> active: false（越过 60ms 防抖）
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(act),
            1,
            now,
            now + Duration::from_millis(100),
        );

        let list = states.lock().clone();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].mapping_id, "m_toggle");
        assert!(list[0].active);
        assert_eq!(list[1].mapping_id, "m_toggle");
        assert!(!list[1].active);
    }

    // ==========================================
    // Queue & Safety 测试
    // ==========================================

    #[test]
    fn decide_swallow_table() {
        // (button, down, is_mapped, captured, queued) -> swallow
        let cases = [
            (MouseButton::Left, true, true, true, true, false),
            (MouseButton::Right, true, true, true, true, false),
            (MouseButton::XButton1, true, true, false, true, true),
            (MouseButton::XButton1, true, true, false, false, false),
            (MouseButton::XButton1, true, false, true, false, true),
            (MouseButton::WheelUp, true, false, true, false, true),
            (MouseButton::XButton1, false, false, false, false, false),
            (MouseButton::Middle, true, false, false, false, false),
        ];
        for (button, down, is_mapped, captured, queued, expected) in cases {
            assert_eq!(
                decide_swallow(button, down, is_mapped, captured, queued),
                expected,
                "button={button:?} down={down} mapped={is_mapped} captured={captured} queued={queued}"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn classify_table() {
        let acc = AtomicI32::new(0);
        let xbutton = |hi: u16| (hi as u32) << 16;
        assert_eq!(
            classify(WM_XBUTTONDOWN, xbutton(1), &acc),
            Some((MouseButton::XButton1, true))
        );
        assert_eq!(
            classify(WM_XBUTTONDOWN, xbutton(2), &acc),
            Some((MouseButton::XButton2, true))
        );
        assert_eq!(
            classify(WM_XBUTTONUP, xbutton(1), &acc),
            Some((MouseButton::XButton1, false))
        );
        assert_eq!(
            classify(WM_MBUTTONUP, 0, &acc),
            Some((MouseButton::Middle, false))
        );
        assert_eq!(classify(WM_MOUSEHWHEEL, 120u32 << 16, &acc), None);
        assert_eq!(classify(WM_LBUTTONDOWN, 0, &acc), Some((MouseButton::Left, true)));
        // A full positive notch emits exactly one WheelUp step.
        assert_eq!(
            classify(WM_MOUSEWHEEL, 120u32 << 16, &acc),
            Some((MouseButton::WheelUp, true))
        );
        // Sub-notch deltas accumulate without emitting.
        assert_eq!(classify(WM_MOUSEWHEEL, 60u32 << 16, &acc), None);
        assert_eq!(
            classify(WM_MOUSEWHEEL, 60u32 << 16, &acc),
            Some((MouseButton::WheelUp, true))
        );
    }

    #[test]
    fn emergency_always_pauses() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector);
        sm.emergency_stop();
        assert!(sm.is_paused());
    }

    #[test]
    fn emergency_always_resets_even_when_already_paused() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        // 处于某种原因已被 paused 且仍有残留
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);
        sm.set_paused_raw(true);
        injector.clear();

        // 再次触发 EmergencyStop，必须无条件执行 reset 且保持 paused
        sm.emergency_stop();
        assert!(sm.is_paused());
        let evs = injector.events();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].action, KeyAction::Up);
        assert_eq!(evs[0].spec.vk, VK_LCONTROL);
    }

    #[test]
    fn pause_resets_pending_click() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let act = dummy_action("m1", TriggerMode::Click, &["Enter"]);
        let t0 = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            true,
            Some(act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );

        // 暂停
        sm.set_paused(true);

        // 验证正在进行的 tap 立即释放，队列中的 tap 被清空
        let ups: Vec<_> = injector
            .events()
            .into_iter()
            .filter(|e| e.action == KeyAction::Up)
            .collect();
        assert_eq!(ups.len(), 1);
        assert!(sm.tap_queue.is_empty());
        assert!(sm.tap_in_flight.is_empty());
    }

    #[test]
    fn shutdown_resets_pending_click() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let act = dummy_action("m1", TriggerMode::Click, &["Enter"]);
        let t0 = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, t0, t0);
        sm.reset_all(ResetReason::Shutdown);

        let ups: Vec<_> = injector
            .events()
            .into_iter()
            .filter(|e| e.action == KeyAction::Up)
            .collect();
        assert_eq!(ups.len(), 1);
        assert!(sm.tap_in_flight.is_empty());
    }

    #[test]
    fn stale_generation_down_is_ignored_after_config_change() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        sm.update_config(2);
        // 发送携带旧 generation=1 的 down
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);

        assert!(
            injector.events().is_empty(),
            "Stale down event must be ignored"
        );
    }

    // ==========================================
    // Capability 规则测试
    // ==========================================

    #[test]
    fn compile_skips_left_right_mapping() {
        let mappings = vec![
            Mapping {
                id: "1".into(),
                button: "left".into(),
                mode: "hold".into(),
                keys: vec!["LControl".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
            Mapping {
                id: "2".into(),
                button: "right".into(),
                mode: "click".into(),
                keys: vec!["Enter".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
            Mapping {
                id: "3".into(),
                button: "middle".into(),
                mode: "hold".into(),
                keys: vec!["Space".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
        ];

        let compiled = compile_mappings(&mappings);
        assert!(compiled.get(MouseButton::Left).is_none());
        assert!(compiled.get(MouseButton::Right).is_none());
        assert!(compiled.get(MouseButton::Middle).is_some());
    }

    #[test]
    fn wheel_accepts_click_only() {
        let mappings = vec![
            Mapping {
                id: "1".into(),
                button: "wheelup".into(),
                mode: "hold".into(),
                keys: vec!["ArrowUp".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
            Mapping {
                id: "2".into(),
                button: "wheeldown".into(),
                mode: "toggle".into(),
                keys: vec!["ArrowDown".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
            Mapping {
                id: "3".into(),
                button: "wheelup".into(),
                mode: "click".into(),
                keys: vec!["PageUp".into()],
                label: "".into(),
                tap_keys: vec![],
                hold_keys: vec![],
            },
        ];

        let compiled = compile_mappings(&mappings);
        let wheel_down = compiled
            .get(MouseButton::WheelDown)
            .expect("wheeldown toggle is coerced to click");
        assert_eq!(wheel_down.mode, TriggerMode::Click);
        let wheel_up = compiled
            .get(MouseButton::WheelUp)
            .expect("wheelup hold is coerced to click");
        assert_eq!(wheel_up.mode, TriggerMode::Click);
    }

    // ==========================================
    // 重叠与并发测试 (Overlap regression cases)
    // ==========================================

    #[test]
    fn test_overlap_hold_ctrl_alt_and_click_ctrl_enter() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(30);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);

        let hold_act = dummy_action("h", TriggerMode::Hold, &["LControl", "LAlt"]);
        let click_act = dummy_action("c", TriggerMode::Click, &["LControl", "Enter"]);
        let t0 = Instant::now();

        // 1. Hold Ctrl+Alt down
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, t0, t0);
        // 2. Click Ctrl+Enter down
        sm.handle_mouse_edge(
            MouseButton::XButton2,
            true,
            Some(click_act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );

        // 3. Click 到期
        sm.tick(t0 + Duration::from_millis(35));

        // 此时 Ctrl 必须保持 held，Enter 释放
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LAlt").unwrap()).unwrap(), 1);
        assert_eq!(
            *sm.key_refs.get(&win_key_spec("Enter").unwrap()).unwrap_or(&0),
            0
        );

        // 4. Hold Ctrl+Alt up
        sm.handle_mouse_edge(
            MouseButton::XButton1,
            false,
            Some(hold_act),
            1,
            t0 + Duration::from_millis(50),
            t0 + Duration::from_millis(50),
        );
        assert_eq!(
            *sm.key_refs
                .get(&win_key_spec("LControl").unwrap())
                .unwrap_or(&0),
            0
        );
        assert_eq!(
            *sm.key_refs.get(&win_key_spec("LAlt").unwrap()).unwrap_or(&0),
            0
        );
    }

    #[test]
    fn test_overlap_hold_ctrl_alt_and_toggle_ctrl_shift() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());

        let hold_act = dummy_action("h", TriggerMode::Hold, &["LControl", "LAlt"]);
        let toggle_act = dummy_action("t", TriggerMode::Toggle, &["LControl", "LShift"]);
        let now = Instant::now();

        // 1. Hold Ctrl+Alt
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, now, now);
        // 2. Toggle Ctrl+Shift (ON)
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(toggle_act.clone()), 1, now, now);

        // 3. Hold release
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(hold_act), 1, now, now);

        // 验证：Ctrl 和 Shift 仍由 Toggle 保持！Alt 释放！
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LShift").unwrap()).unwrap(), 1);
        assert_eq!(
            *sm.key_refs.get(&win_key_spec("LAlt").unwrap()).unwrap_or(&0),
            0
        );

        // 4. Toggle second down (OFF)
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(toggle_act),
            1,
            now,
            now + Duration::from_millis(100),
        );
        assert_eq!(
            *sm.key_refs
                .get(&win_key_spec("LControl").unwrap())
                .unwrap_or(&0),
            0
        );
        assert_eq!(
            *sm.key_refs.get(&win_key_spec("LShift").unwrap()).unwrap_or(&0),
            0
        );
    }

    #[test]
    fn test_overlap_toggle_shift_and_hold_shift() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());

        let toggle_act = dummy_action("t", TriggerMode::Toggle, &["LShift"]);
        let hold_act = dummy_action("h", TriggerMode::Hold, &["LShift"]);
        let now = Instant::now();

        // 1. Toggle Shift ON
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(toggle_act.clone()), 1, now, now);
        // 2. Hold Shift down
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, now, now);
        // 3. Hold Shift up
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(hold_act), 1, now, now);

        // Shift 仍应被 Toggle 保持
        assert_eq!(*sm.key_refs.get(&win_key_spec("LShift").unwrap()).unwrap(), 1);

        // 4. Toggle Shift OFF
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(toggle_act),
            1,
            now,
            now + Duration::from_millis(100),
        );
        assert_eq!(
            *sm.key_refs.get(&win_key_spec("LShift").unwrap()).unwrap_or(&0),
            0
        );
    }

    #[test]
    fn test_overlap_hold_ctrl_and_emergency() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let hold_act = dummy_action("h", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act), 1, now, now);
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);

        sm.emergency_stop();
        assert!(sm.is_paused());
        assert_eq!(
            *sm.key_refs
                .get(&win_key_spec("LControl").unwrap())
                .unwrap_or(&0),
            0
        );
    }

    #[test]
    fn test_overlap_tap_enter_burst_while_hold_ctrl_remains_active() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(10);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);

        let hold_act = dummy_action("h", TriggerMode::Hold, &["LControl"]);
        let click_act = dummy_action("c", TriggerMode::Click, &["Enter"]);
        let mut now = Instant::now();

        // 1. Hold Ctrl
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, now, now);

        // 2. 连续 5 次快速 Click Enter
        for _ in 0..5 {
            sm.handle_mouse_edge(MouseButton::XButton2, true, Some(click_act.clone()), 1, now, now);
            now += Duration::from_millis(1);
        }

        // 依次推进时间让 5 次 tap 全部排队完成
        for _ in 0..6 {
            now += dwell;
            sm.tick(now);
        }

        // 检查整个过程中 Ctrl 始终在 held
        assert_eq!(*sm.key_refs.get(&win_key_spec("LControl").unwrap()).unwrap(), 1);

        // 验证正好产生了 5 次独立的 Enter Down 和 5 次独立的 Enter Up
        let enter_downs = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_RETURN && e.action == KeyAction::Down)
            .count();
        let enter_ups = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_RETURN && e.action == KeyAction::Up)
            .count();
        assert_eq!(enter_downs, 5);
        assert_eq!(enter_ups, 5);

        // 3. 最终释放 Hold Ctrl
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(hold_act), 1, now, now);
        assert_eq!(
            *sm.key_refs
                .get(&win_key_spec("LControl").unwrap())
                .unwrap_or(&0),
            0
        );
    }

    #[test]
    fn test_wheel_200_detents_stress() {
        let injector = FakeInjector::new();
        let dwell = Duration::from_millis(2);
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(dwell);
        let wheel_act = dummy_action("w", TriggerMode::Click, &["ArrowUp"]);
        let hold_act = dummy_action("h", TriggerMode::Hold, &["LAlt"]);
        let mut now = Instant::now();

        for i in 0..250 {
            sm.handle_mouse_edge(MouseButton::WheelUp, true, Some(wheel_act.clone()), 1, now, now);
            if i == 100 {
                sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold_act.clone()), 1, now, now);
                assert_eq!(*sm.key_refs.get(&win_key_spec("LAlt").unwrap()).unwrap(), 1);
            }
            if i == 150 {
                sm.handle_mouse_edge(MouseButton::XButton1, false, Some(hold_act.clone()), 1, now, now);
            }
        }

        let queued = sm
            .tap_queue
            .get(&MouseButton::WheelUp)
            .map(|q| q.len())
            .unwrap_or(0);
        assert!(queued <= TAP_QUEUE_CAP);
        assert!(sm.tap_in_flight.contains_key(&MouseButton::WheelUp));

        while sm.tap_in_flight.contains_key(&MouseButton::WheelUp)
            || sm
                .tap_queue
                .get(&MouseButton::WheelUp)
                .is_some_and(|q| !q.is_empty())
        {
            now += dwell;
            sm.tick(now);
        }

        let arrow_downs = injector
            .events()
            .iter()
            .filter(|e| e.spec.vk == VK_UP && e.action == KeyAction::Down)
            .count();
        let arrow_ups = injector
            .events()
            .iter()
            .filter(|e| e.spec.vk == VK_UP && e.action == KeyAction::Up)
            .count();

        assert!(arrow_downs <= TAP_QUEUE_CAP + 1);
        assert_eq!(arrow_downs, arrow_ups);
        assert!(sm.key_refs.is_empty() || sm.key_refs.values().all(|&v| v == 0));
    }

    #[test]
    fn test_release_preserves_physically_held_key() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl"]);
        let now = Instant::now();

        // 模拟物理键盘真实按住了 Ctrl
        injector.set_physical_down(VK_LCONTROL, true);

        // MouseInsight 触发 Hold Ctrl
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);

        // 释放 Hold Ctrl
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(act), 1, now, now);

        // 验证：因为物理 Ctrl 仍被真实按住，绝不发送 synthetic KeyUp 打掉用户物理按键！
        let ctrl_ups = injector
            .events()
            .into_iter()
            .filter(|e| e.spec.vk == VK_LCONTROL && e.action == KeyAction::Up)
            .count();
        assert_eq!(
            ctrl_ups, 0,
            "Synthetic KeyUp must be suppressed when key is physically held down"
        );
    }

    #[test]
    fn recorder_emits_first_physical_key() {
        // Feed keycodes that round-trip through this platform's vk_to_token.
        let a = key_spec("A").unwrap().vk.0 as u32;
        let ctrl = key_spec("LControl").unwrap().vk.0 as u32;
        let mut rec = RecorderState::default();
        assert_eq!(rec.on_key(a, true), vec!["A".to_string()]);
        assert!(rec.on_key(a, true).is_empty(), "key repeat must not re-emit");
        rec.on_key(ctrl, true);
        assert!(rec.max_chord.iter().any(|k| k == "A"));
        assert!(rec.max_chord.iter().any(|k| k == "LControl"));
    }

    #[test]
    fn hold_ctrl_alt_is_one_batch() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl", "LAlt"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);
        let batches = injector.batches.lock().clone();
        assert_eq!(batches, vec![2], "chord must be one SendInput batch");
        assert_eq!(injector.events().len(), 2);
    }

    #[test]
    fn release_sends_up_even_if_injected_keys_look_down() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LControl", "LAlt"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);
        sm.handle_mouse_edge(MouseButton::XButton1, false, None, 1, now, now);
        let ups = injector
            .events()
            .into_iter()
            .filter(|e| e.action == KeyAction::Up && !e.is_mask)
            .count();
        assert_eq!(ups, 2);
    }

    #[test]
    fn tap_queue_is_capped() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone()).with_dwell(Duration::from_millis(30));
        let act = dummy_action("m2", TriggerMode::Click, &["Enter"]);
        let now = Instant::now();
        for _ in 0..40 {
            sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);
        }
        let queued = sm
            .tap_queue
            .get(&MouseButton::XButton1)
            .map(|q| q.len())
            .unwrap_or(0);
        assert!(queued <= TAP_QUEUE_CAP);
        assert!(sm.tap_in_flight.contains_key(&MouseButton::XButton1));
    }

    #[test]
    fn replace_file_overwrites_existing() {
        let dir = std::env::temp_dir().join(format!("mi-cfg-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let from = dir.join("src.tmp");
        let to = dir.join("dst.json");
        fs::write(&to, b"old").unwrap();
        fs::write(&from, b"new").unwrap();
        replace_file_atomic(&from, &to).expect("replace");
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }

    fn dummy_dual(id: &str, tap: &[&str], hold: &[&str]) -> Arc<CompiledAction> {
        let tap_specs: Vec<KeySpec> = tap.iter().map(|k| win_key_spec(k).unwrap()).collect();
        let hold_specs: Vec<KeySpec> = hold.iter().map(|k| win_key_spec(k).unwrap()).collect();
        Arc::new(CompiledAction {
            mapping_id: id.into(),
            mode: TriggerMode::Dual,
            specs: tap_specs.clone(),
            tap_specs,
            hold_specs,
        })
    }

    #[test]
    fn dual_short_press_fires_tap_only() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone())
            .with_dwell(Duration::from_millis(10))
            .with_hold_threshold(Duration::from_millis(40));
        let act = dummy_dual("mid", &["LControl", "V"], &["LControl", "C"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, t0, t0);
        assert!(injector.events().is_empty(), "must not inject until up or threshold");
        sm.handle_mouse_edge(
            MouseButton::Middle,
            false,
            None,
            1,
            t0 + Duration::from_millis(10),
            t0 + Duration::from_millis(10),
        );
        sm.tick(t0 + Duration::from_millis(30));
        let evs = injector.events();
        let vks: Vec<_> = evs.iter().map(|e| e.spec.vk).collect();
        assert!(vks.contains(&VK_LCONTROL));
        assert!(vks.contains(&VK_RETURN) || evs.iter().any(|e| e.spec.vk.0 == b'V' as u16));
        assert!(!evs.iter().any(|e| e.spec.vk.0 == b'C' as u16));
    }

    #[test]
    fn dual_long_press_fires_hold_not_tap() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone())
            .with_dwell(Duration::from_millis(10))
            .with_hold_threshold(Duration::from_millis(20));
        let act = dummy_dual("mid", &["LControl", "V"], &["LControl", "C"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, t0, t0);
        sm.tick(t0 + Duration::from_millis(25));
        let after_hold = injector.events();
        assert!(after_hold.iter().any(|e| e.spec.vk.0 == b'C' as u16 && e.action == KeyAction::Down));
        assert!(!after_hold.iter().any(|e| e.spec.vk.0 == b'V' as u16));
        sm.handle_mouse_edge(
            MouseButton::Middle,
            false,
            None,
            1,
            t0 + Duration::from_millis(40),
            t0 + Duration::from_millis(40),
        );
        let ups = injector
            .events()
            .into_iter()
            .filter(|e| e.action == KeyAction::Up && e.spec.vk.0 == b'C' as u16)
            .count();
        assert_eq!(ups, 1);
    }

    #[test]
    fn compile_dual_from_tap_and_hold_keys() {
        let mappings = vec![Mapping {
            id: "1".into(),
            button: "middle".into(),
            mode: "dual".into(),
            keys: vec![],
            label: "".into(),
            tap_keys: vec!["LControl".into(), "V".into()],
            hold_keys: vec!["LControl".into(), "C".into()],
        }];
        let compiled = compile_mappings(&mappings);
        let action = compiled.get(MouseButton::Middle).unwrap();
        assert_eq!(action.mode, TriggerMode::Dual);
        assert_eq!(action.tap_specs.len(), 2);
        assert_eq!(action.hold_specs.len(), 2);
    }

    fn bare_mapping(mode: &str, keys: &[&str], tap: &[&str], hold: &[&str]) -> Mapping {
        Mapping {
            id: "m".into(),
            button: "xbutton1".into(),
            mode: mode.into(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
            label: "".into(),
            tap_keys: tap.iter().map(|k| k.to_string()).collect(),
            hold_keys: hold.iter().map(|k| k.to_string()).collect(),
        }
    }

    #[test]
    fn compile_hold_only_slot_becomes_hold() {
        let compiled = compile_mappings(&[bare_mapping("dual", &[], &[], &["LControl", "C"])]);
        let action = compiled.get(MouseButton::XButton1).expect("compiled slot");
        assert_eq!(action.mode, TriggerMode::Hold);
        assert_eq!(action.hold_specs.len(), 2);
        assert!(action.tap_specs.is_empty());
    }

    #[test]
    fn compile_tap_only_slot_becomes_click() {
        let compiled = compile_mappings(&[bare_mapping("dual", &[], &["LControl", "V"], &[])]);
        let action = compiled.get(MouseButton::XButton1).expect("compiled slot");
        assert_eq!(action.mode, TriggerMode::Click);
        assert_eq!(action.specs.len(), 2);
    }

    #[test]
    fn compile_toggle_uses_keys() {
        let compiled = compile_mappings(&[bare_mapping("toggle", &["Enter"], &[], &[])]);
        let action = compiled.get(MouseButton::XButton1).expect("compiled slot");
        assert_eq!(action.mode, TriggerMode::Toggle);
        assert!(!action.specs.is_empty());
    }

    #[test]
    fn dual_pending_cleared_on_pause() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone())
            .with_hold_threshold(Duration::from_millis(20));
        let act = dummy_dual("mid", &["LControl", "V"], &["LControl", "C"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, t0, t0);
        assert!(sm.pending_dual.contains_key(&MouseButton::Middle));

        sm.set_paused(true);
        assert!(sm.pending_dual.is_empty());

        // The release edge and any later tick must not inject anything.
        sm.handle_mouse_edge(
            MouseButton::Middle,
            false,
            None,
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );
        sm.tick(t0 + Duration::from_millis(60));
        assert!(injector.events().is_empty());
    }

    #[test]
    fn dual_repeated_down_during_pending_ignored() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone())
            .with_dwell(Duration::from_millis(10))
            .with_hold_threshold(Duration::from_millis(40));
        let act = dummy_dual("mid", &["LControl", "V"], &["LControl", "C"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act.clone()), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(act),
            1,
            t0 + Duration::from_millis(5),
            t0 + Duration::from_millis(5),
        );
        assert_eq!(sm.pending_dual.len(), 1);
        sm.handle_mouse_edge(
            MouseButton::Middle,
            false,
            None,
            1,
            t0 + Duration::from_millis(10),
            t0 + Duration::from_millis(10),
        );
        sm.tick(t0 + Duration::from_millis(30));
        let downs = injector
            .events()
            .into_iter()
            .filter(|e| e.action == KeyAction::Down)
            .count();
        assert_eq!(downs, 2, "exactly one tap chord (LControl+V) down");
    }

    #[test]
    fn dual_empty_tap_short_press_injects_nothing() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone())
            .with_dwell(Duration::from_millis(10))
            .with_hold_threshold(Duration::from_millis(40));
        let act = dummy_dual("mid", &[], &["LControl", "C"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::Middle,
            false,
            None,
            1,
            t0 + Duration::from_millis(10),
            t0 + Duration::from_millis(10),
        );
        sm.tick(t0 + Duration::from_millis(30));
        assert!(injector.events().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_installed_config_dir_uses_application_support() {
        let dir = super::config_dir();
        let text = dir.to_string_lossy();
        assert!(
            text.contains("Application Support"),
            "macOS config dir should be under Application Support, got {text}"
        );
        assert!(
            text.ends_with("MouseInsight"),
            "macOS config dir should end with MouseInsight, got {text}"
        );
    }
    #[test]
    fn recorder_remove_retains_released_chord_keys() {
        let rctrl = key_spec("RControl").unwrap().vk.0 as u32;
        let ralt = key_spec("RAlt").unwrap().vk.0 as u32;
        let mut recorder = RecorderState::default();
        recorder.on_key(rctrl, true);
        recorder.on_key(ralt, true);
        recorder.on_key(rctrl, false);
        recorder.on_key(ralt, false);
        recorder.remove_token("RControl");
        assert_eq!(recorder.max_chord, vec!["RAlt"]);
    }

    #[test]
    fn generic_windows_modifiers_resolve_both_sides() {
        assert_eq!(sided_windows_vk(0x12, 0x38, true), 0xA5);
        assert_eq!(sided_windows_vk(0x12, 0x38, false), 0xA4);
        assert_eq!(sided_windows_vk(0x11, 0x1D, true), 0xA3);
        assert_eq!(sided_windows_vk(0x10, 0x36, false), 0xA1);
        assert_eq!(sided_windows_vk(0xA5, 0x38, true), 0xA5);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn right_alt_injection_uses_extended_scan_code() {
        let right = win_key_spec("RAlt").unwrap();
        let left = win_key_spec("LAlt").unwrap();
        for down in [true, false] {
            let r = unsafe { make_input(&right, down).Anonymous.ki };
            let l = unsafe { make_input(&left, down).Anonymous.ki };
            assert_eq!(r.wVk.0, 0);
            assert_eq!(r.wScan, 0x38);
            assert!(r.dwFlags.contains(KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY));
            assert!(!l.dwFlags.contains(KEYEVENTF_EXTENDEDKEY));
            assert_eq!(r.dwFlags.contains(KEYEVENTF_KEYUP), !down);
        }
    }

    #[test]
    fn cleared_dual_does_not_compile_legacy_keys() {
        // A fully cleared mapping (keys + both slots empty) compiles to nothing.
        let mapping = Mapping { id: "clear".into(), button: "middle".into(), mode: "dual".into(), keys: vec![], tap_keys: vec![], hold_keys: vec![], label: String::new() };
        assert!(compile_mappings(&[mapping]).get(MouseButton::Middle).is_none());
    }

    #[test]
    fn legacy_dual_keys_compiles_to_tap() {
        // Legacy rows only carried `keys`; with empty slots they feed the tap slot.
        let mapping = Mapping { id: "legacy".into(), button: "middle".into(), mode: "dual".into(), keys: vec!["LAlt".into()], tap_keys: vec![], hold_keys: vec![], label: String::new() };
        let compiled = compile_mappings(&[mapping]);
        let compiled = compiled.get(MouseButton::Middle).expect("mapping should compile");
        assert_eq!(compiled.mode, TriggerMode::Click);
    }

    #[test]
    fn overflowing_edge_queue_pauses_once_and_schedules_release() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (edge_tx, _edge_rx) = crossbeam_channel::bounded(1);
        let (telem_tx, _telem_rx) = crossbeam_channel::bounded(1);
        let engine = Engine {
            hook_status: RwLock::new("starting".into()),
            cfg: RwLock::new(AppConfig::default()), compiled: ArcSwap::from_pointee(compile_mappings(&[])),
            paused: AtomicBool::new(false), listening: AtomicBool::new(false), swallowed_buttons: AtomicU32::new(0), recording: AtomicBool::new(false),
            window_visible: AtomicBool::new(false), last: RwLock::new(None), cmd_tx, edge_tx, telem_tx,
            recorder: RwLock::new(RecorderState::default()),
            swallowed_keys: RwLock::new(HashSet::new()),
            active_bindings: RwLock::new(HashMap::new()),
            recovery_notes: RwLock::new(Vec::new()),
        };
        let edge = || InputCmd::MouseEdge { button: MouseButton::XButton1, down: false, action: None, generation: 1, at: Instant::now() };
        assert!(engine.enqueue_edge(edge()));
        assert!(!engine.enqueue_edge(edge()));
        assert!(!engine.enqueue_edge(edge()));
        assert!(engine.paused.load(Ordering::SeqCst));
        assert!(matches!(cmd_rx.try_recv().unwrap(), InputCmd::EmergencyStop { persist: false }));
        assert!(cmd_rx.try_recv().is_err());
        let mut state = InputStateMachine::new(FakeInjector::new());
        state.handle_mouse_edge(MouseButton::XButton1, true, Some(dummy_action("held", TriggerMode::Hold, &["RAlt"])), 1, Instant::now(), Instant::now());
        state.emergency_stop();
        assert!(state.key_refs.is_empty());
        assert!(state.is_paused());
    }

    #[test]
    fn portable_layout_prefers_data_and_preserves_legacy_support() {
        let dir = std::env::temp_dir().join(format!("mi-portable-layout-{}-{}", std::process::id(), SAVE_SEQ.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(dir.join("data")).unwrap();
        assert!(portable_config_dir(&dir).is_none());
        fs::write(dir.join(".portable"), "").unwrap();
        assert_eq!(portable_config_dir(&dir), Some(dir.clone()));
        fs::write(dir.join("data/.portable"), "").unwrap();
        assert_eq!(portable_config_dir(&dir), Some(dir.join("data")));
        fs::remove_file(dir.join(".portable")).unwrap();
        assert_eq!(portable_config_dir(&dir), Some(dir.join("data")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn quick_menu_preserves_other_slot_and_rejects_invalid_operations() {
        let mut config = AppConfig::default();
        apply_quick_mapping(&mut config, "middle", "tap", "copy").unwrap();
        apply_quick_mapping(&mut config, "middle", "hold", "enter").unwrap();
        assert_eq!(config.mappings.len(), 1);
        assert_eq!(config.mappings[0].tap_keys.last().unwrap(), "C");
        assert_eq!(config.mappings[0].hold_keys, ["Enter"]);
        apply_quick_mapping(&mut config, "middle", "tap", "clear").unwrap();
        assert!(config.mappings[0].tap_keys.is_empty());
        assert_eq!(config.mappings[0].hold_keys, ["Enter"]);
        assert!(apply_quick_mapping(&mut config, "left", "tap", "copy").is_err());
        assert!(apply_quick_mapping(&mut config, "wheelup", "hold", "copy").is_err());
        config.mappings[0].mode = "toggle".into();
        assert!(apply_quick_mapping(&mut config, "middle", "tap", "copy").is_err());
        assert_eq!(config.mappings[0].mode, "toggle");
        config.mappings[0].button = "xbutton1".into();
        apply_quick_mapping(&mut config, "middle", "tap", "copy").unwrap();
        assert_ne!(config.mappings[0].id, config.mappings[1].id);
    }

    #[test]
    fn partial_insertion_rolls_back_unsent_key_refs() {
        let injector = FakeInjector::new();
        *injector.fail_after.lock() = Some(1);
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LShift", "LAlt"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act), 1, now, now);
        // Only the first key was actually inserted; the second ref is rolled
        // back so a later release never keys-up something never pressed.
        assert_eq!(sm.key_refs.get(&win_key_spec("LShift").unwrap()).copied(), Some(1));
        assert!(!sm.key_refs.contains_key(&win_key_spec("LAlt").unwrap()));
    }

    #[test]
    fn pending_release_retries_failed_key_ups() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("m1", TriggerMode::Hold, &["LShift"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);
        injector.fail_ups.store(true, Ordering::Relaxed);
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(act), 1, now, now);
        assert!(!sm.pending_release.is_empty());
        assert_eq!(
            injector.events().iter().filter(|e| e.action == KeyAction::Up).count(),
            0
        );
        injector.fail_ups.store(false, Ordering::Relaxed);
        sm.tick(Instant::now());
        assert!(sm.pending_release.is_empty());
        assert_eq!(
            injector
                .events()
                .iter()
                .filter(|e| e.action == KeyAction::Up && !e.is_mask)
                .count(),
            1
        );
    }

    #[test]
    fn watchdog_releases_hold_whose_button_up_was_lost() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let hold = dummy_action("h", TriggerMode::Hold, &["LShift"]);
        let toggle = dummy_action("t", TriggerMode::Toggle, &["LAlt"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(hold), 1, now, now);
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(toggle), 1, now, now);

        // The physical release never reached the hook; the watchdog must
        // synthesize it. Toggles are latched state, not held buttons.
        injector.set_button_up(MouseButton::XButton1, true);
        let released = sm.tick_watchdog(Instant::now());
        assert_eq!(released, vec![MouseButton::XButton1]);
        assert!(sm.active_holds.is_empty());
        assert!(injector
            .events()
            .iter()
            .any(|e| e.action == KeyAction::Up && e.spec.vk == VK_LSHIFT && !e.is_mask));
        assert_eq!(sm.key_refs.get(&win_key_spec("LAlt").unwrap()).copied(), Some(1));
    }

    #[test]
    fn toggle_debounce_ignores_rapid_second_down() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("t", TriggerMode::Toggle, &["LShift"]);
        let t0 = Instant::now();
        sm.handle_mouse_edge(MouseButton::Middle, true, Some(act.clone()), 1, t0, t0);
        sm.handle_mouse_edge(
            MouseButton::Middle,
            true,
            Some(act),
            1,
            t0 + Duration::from_millis(10),
            t0 + Duration::from_millis(10),
        );
        let downs = injector
            .events()
            .iter()
            .filter(|e| e.action == KeyAction::Down && !e.is_mask)
            .count();
        assert_eq!(downs, 1);
        assert_eq!(sm.key_refs.get(&win_key_spec("LShift").unwrap()).copied(), Some(1));
    }

    #[test]
    fn wheel_step_accumulates_partial_notches() {
        let acc = AtomicI32::new(0);
        assert!(wheel_step(0, &acc).is_none());
        assert_eq!(wheel_step(120, &acc), Some(MouseButton::WheelUp));
        assert_eq!(wheel_step(-120, &acc), Some(MouseButton::WheelDown));
        assert!(wheel_step(60, &acc).is_none());
        assert_eq!(wheel_step(60, &acc), Some(MouseButton::WheelUp));
        // A double-notch delta emits one step and banks the remainder;
        // the banked 120 does not re-fire on a zero delta.
        assert_eq!(wheel_step(240, &acc), Some(MouseButton::WheelUp));
        assert_eq!(acc.load(Ordering::Relaxed), 120);
        assert!(wheel_step(0, &acc).is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn alt_hold_release_emits_mask_pair() {
        let injector = FakeInjector::new();
        let mut sm = InputStateMachine::new(injector.clone());
        let act = dummy_action("h", TriggerMode::Hold, &["LAlt"]);
        let now = Instant::now();
        sm.handle_mouse_edge(MouseButton::XButton1, true, Some(act.clone()), 1, now, now);
        sm.handle_mouse_edge(MouseButton::XButton1, false, Some(act), 1, now, now);
        let evs = injector.events();
        assert!(evs.len() >= 2);
        assert!(evs[evs.len() - 2..].iter().all(|e| e.is_mask));
    }

    #[test]
    fn lenient_parse_accepts_bom_and_clean_config() {
        let text = "\u{feff}{\"schema_version\":1,\"theme\":\"dark\",\"autostart\":true,\"paused\":false,\"mappings\":[]}";
        let (cfg, notes) = parse_config_lenient(text).unwrap();
        assert!(notes.is_empty());
        assert_eq!(cfg.theme, "dark");
        assert!(cfg.autostart);
    }

    #[test]
    fn lenient_parse_skips_broken_mapping_rows() {
        let text = r#"{
            "schema_version": 1, "theme": "light",
            "mappings": [
                {"id":"a","button":"middle","mode":"hold","keys":["LShift"]},
                {"id":"b","button":"xbutton1","mode":"hold"}
            ]
        }"#;
        let (cfg, notes) = parse_config_lenient(text).unwrap();
        assert_eq!(cfg.mappings.len(), 1);
        assert_eq!(cfg.mappings[0].id, "a");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn lenient_parse_resets_unknown_theme() {
        let (cfg, notes) = parse_config_lenient(r#"{"theme":"neon","mappings":[]}"#).unwrap();
        assert_eq!(cfg.theme, "light");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn lenient_parse_preserves_unknown_top_level_fields() {
        let (cfg, _) = parse_config_lenient(r#"{"mappings":[],"future":{"x":1}}"#).unwrap();
        assert!(cfg.extra.contains_key("future"));
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(out.contains("\"future\""));
    }

    #[test]
    fn lenient_parse_rejects_non_object_root() {
        assert!(parse_config_lenient("[1,2,3]").is_err());
        assert!(parse_config_lenient("{ not json").is_err());
    }

    #[test]
    fn specs_from_names_skips_unsupported_tokens() {
        let specs = specs_from_names(&[
            "LControl".to_string(),
            "NotAKey".to_string(),
            "C".to_string(),
        ]);
        assert_eq!(specs.len(), 2);
    }

    #[test]
    fn quick_mapping_clear_removes_empty_mapping() {
        let mut config = AppConfig::default();
        apply_quick_mapping(&mut config, "middle", "tap", "copy").unwrap();
        apply_quick_mapping(&mut config, "middle", "hold", "enter").unwrap();
        assert_eq!(config.mappings.len(), 1);
        apply_quick_mapping(&mut config, "middle", "tap", "clear").unwrap();
        assert_eq!(config.mappings.len(), 1);
        apply_quick_mapping(&mut config, "middle", "hold", "clear").unwrap();
        assert!(config.mappings.is_empty());
    }

    fn simple_mapping(id: &str, button: &str) -> Mapping {
        Mapping {
            id: id.into(),
            button: button.into(),
            mode: "hold".into(),
            keys: vec!["LShift".into()],
            tap_keys: vec![],
            hold_keys: vec![],
            label: String::new(),
        }
    }

    #[test]
    fn validate_mappings_rejects_bad_input() {
        assert!(validate_mappings(&[simple_mapping("a", "middle")]).is_ok());

        // More than 5 mappings.
        let many: Vec<Mapping> = (0..6).map(|i| simple_mapping(&format!("m{i}"), "middle")).collect();
        assert!(validate_mappings(&many).is_err());

        // Duplicate button.
        assert!(validate_mappings(&[
            simple_mapping("a", "middle"),
            simple_mapping("b", "middle"),
        ]).is_err());

        // Primary button cannot be mapped.
        assert!(validate_mappings(&[simple_mapping("a", "left")]).is_err());

        // Chord longer than 16.
        let mut long = simple_mapping("a", "middle");
        long.keys = vec!["LShift".to_string(); 17];
        assert!(validate_mappings(&[long]).is_err());

        // Illegal id characters / empty id.
        assert!(validate_mappings(&[simple_mapping("bad id!", "middle")]).is_err());
        assert!(validate_mappings(&[simple_mapping("", "middle")]).is_err());
    }

    #[test]
    fn normalize_orders_cmd_before_letter() {
        let normalized = normalize_key_chord(&["C".to_string(), "Cmd".to_string()]);
        assert_eq!(normalized, vec!["Cmd", "C"]);
    }

    #[test]
    fn scroll_step_discrete_notch_is_one_step() {
        let acc = AtomicI64::new(0);
        let last = AtomicU64::new(0);
        assert_eq!(
            scroll_step(false, 1, &acc, 0, &last),
            Some(MouseButton::WheelUp)
        );
        assert_eq!(
            scroll_step(false, -1, &acc, 0, &last),
            Some(MouseButton::WheelDown)
        );
        assert_eq!(scroll_step(false, 0, &acc, 0, &last), None);
    }

    #[test]
    fn scroll_step_continuous_accumulates_to_threshold() {
        let acc = AtomicI64::new(0);
        let last = AtomicU64::new(0);
        assert_eq!(scroll_step(true, 15, &acc, 1000, &last), None);
        assert_eq!(scroll_step(true, 15, &acc, 1010, &last), None);
        assert_eq!(
            scroll_step(true, 15, &acc, 1020, &last),
            Some(MouseButton::WheelUp)
        );
        assert_eq!(acc.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn scroll_step_continuous_rate_limits_bursts() {
        let acc = AtomicI64::new(0);
        let last = AtomicU64::new(0);
        assert_eq!(
            scroll_step(true, 50, &acc, 1000, &last),
            Some(MouseButton::WheelUp)
        );
        // 20ms later: threshold reached again but inside the 50ms window.
        assert_eq!(scroll_step(true, 50, &acc, 1020, &last), None);
        // The pending delta survives and fires once the interval elapses.
        assert_eq!(
            scroll_step(true, 1, &acc, 1060, &last),
            Some(MouseButton::WheelUp)
        );
    }

}
