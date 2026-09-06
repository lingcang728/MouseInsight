use arc_swap::ArcSwap;
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
    VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS,
    VK_PAUSE, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SCROLL, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, MSLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

const LLMHF_INJECTED: u32 = 0x0000_0001;
const LLMHF_LOWER_IL_INJECTED: u32 = 0x0000_0002;
const LLKHF_UP: u32 = 0x80;
const LLKHF_INJECTED_KBD: u32 = 0x10;
const VK_MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xFF);

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
}

impl TriggerMode {
    pub fn from_str_fast(s: &str) -> Self {
        match s {
            "click" => TriggerMode::Click,
            "toggle" => TriggerMode::Toggle,
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

#[derive(Clone, Debug)]
pub struct CompiledAction {
    pub mode: TriggerMode,
    pub specs: Vec<KeySpec>,
}

#[derive(Clone, Default, Debug)]
pub struct CompiledMappings {
    pub slots: [Option<CompiledAction>; MouseButton::COUNT],
}

impl CompiledMappings {
    #[inline(always)]
    pub fn get(&self, btn: MouseButton) -> Option<&CompiledAction> {
        self.slots[btn as usize].as_ref()
    }
}

pub fn compile_mappings(mappings: &[Mapping]) -> CompiledMappings {
    let mut slots = [None, None, None, None, None, None, None];
    for m in mappings {
        if m.keys.is_empty() {
            continue;
        }
        if let Some(btn) = MouseButton::from_str_fast(&m.button) {
            let idx = btn as usize;
            if slots[idx].is_none() {
                let specs: Vec<KeySpec> = m.keys.iter().filter_map(|k| key_spec(k)).collect();
                if !specs.is_empty() {
                    let mode = TriggerMode::from_str_fast(&m.mode);
                    slots[idx] = Some(CompiledAction {
                        mode,
                        specs,
                    });
                }
            }
        }
    }
    CompiledMappings { slots }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub id: String,
    pub button: String,
    pub mode: String,
    pub keys: Vec<String>,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub theme: String,
    pub autostart: bool,
    pub paused: bool,
    pub mappings: Vec<Mapping>,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
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
    pub is_portable: bool,
    pub config_dir: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendReport {
    pub timestamp: u64,
    pub expected: u32,
    pub inserted: u32,
    pub win32_error: u32,
    pub is_uipi_blocked: bool,
}

#[derive(Clone, Debug)]
pub struct ActiveBinding {
    pub specs: Vec<KeySpec>,
    pub mode: TriggerMode,
}

#[derive(Default)]
pub struct InputState {
    pub active_bindings: HashMap<MouseButton, ActiveBinding>,
    pub key_refs: HashMap<KeySpec, u32>,
    pub toggle_on: HashMap<MouseButton, bool>,
}

impl InputState {
    pub fn reset_all(&mut self) {
        let mut had_alt_or_win = false;
        for (spec, count) in &self.key_refs {
            if *count > 0 {
                send_key(spec, false);
                if is_alt_or_win(spec.vk) {
                    had_alt_or_win = true;
                }
            }
        }
        if had_alt_or_win {
            send_mask_key();
        }
        self.active_bindings.clear();
        self.key_refs.clear();
        self.toggle_on.clear();
    }
}

#[derive(Default)]
struct RecorderState {
    physical_held: HashSet<u32>,
    max_chord: Vec<String>,
    chip_modifiers: HashSet<String>,
}

impl RecorderState {
    fn reset(&mut self) {
        self.physical_held.clear();
        self.max_chord.clear();
        self.chip_modifiers.clear();
    }

    fn on_key(&mut self, vk: u32, down: bool) -> Vec<String> {
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
            self.max_chord.clone()
        } else {
            self.physical_held.remove(&vk);
            self.max_chord.clone()
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
        self.max_chord.clone()
    }
}

fn modifier_weight(key: &str) -> u32 {
    match key {
        "LControl" | "RControl" | "Ctrl" | "Control" => 10,
        "LShift" | "RShift" | "Shift" => 20,
        "LAlt" | "RAlt" | "Alt" => 30,
        "LWin" | "RWin" | "Win" | "Meta" => 40,
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

enum InputCmd {
    Fire { button: MouseButton, down: bool },
    ResetState,
    EmergencyPause,
    ListenCaptured(String),
    Record(Vec<String>),
    RecordCancel,
}

struct Engine {
    cfg: RwLock<AppConfig>,
    compiled: ArcSwap<CompiledMappings>,
    paused: AtomicBool,
    listening: AtomicBool,
    recording: AtomicBool,
    window_visible: AtomicBool,
    last: RwLock<Option<Pulse>>,
    last_send_error: RwLock<Option<SendReport>>,
    cmd_tx: Sender<InputCmd>,
    telem_tx: Sender<Pulse>,
    recorder: RwLock<RecorderState>,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static HOOK_TID: AtomicU32 = AtomicU32::new(0);

fn engine() -> &'static Engine {
    ENGINE.get().expect("engine not started")
}

pub fn set_window_visible(visible: bool) {
    if let Some(e) = ENGINE.get() {
        e.window_visible.store(visible, Ordering::Relaxed);
    }
}

pub fn is_portable_mode() -> bool {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    exe_dir.join(".portable").is_file()
        || exe_dir.join("portable").is_file()
        || exe_dir.join("config.json").is_file()
}

pub fn config_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    if is_portable_mode() {
        exe_dir
    } else {
        std::env::var_os("APPDATA")
            .map(|appdata| PathBuf::from(appdata).join("MouseInsight"))
            .unwrap_or(exe_dir)
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn config_bak_path() -> PathBuf {
    config_dir().join("config.json.bak")
}

fn load_config() -> AppConfig {
    let path = config_path();
    if !path.exists() {
        return AppConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
            Ok(cfg) => cfg,
            Err(err) => {
                eprintln!("[MouseInsight] config.json parse error: {err}. Attempting backup recovery...");
                let bak = config_bak_path();
                if bak.exists() {
                    if let Ok(bak_s) = fs::read_to_string(&bak) {
                        if let Ok(bak_cfg) = serde_json::from_str::<AppConfig>(&bak_s) {
                            eprintln!("[MouseInsight] Restored config from config.json.bak successfully.");
                            return bak_cfg;
                        }
                    }
                }
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let corrupt_path = config_dir().join(format!("config.json.corrupted.{ts}"));
                let _ = fs::copy(&path, &corrupt_path);
                eprintln!("[MouseInsight] Preserved corrupted config at {:?}", corrupt_path);
                AppConfig::default()
            }
        },
        Err(e) => {
            eprintln!("[MouseInsight] Failed to read config file: {e}");
            AppConfig::default()
        }
    }
}

fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;

    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;

    let pid = std::process::id();
    let tmp_path = dir.join(format!("config.json.{pid}.tmp"));
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
        let _ = fs::copy(&file_path, &bak_path);
    }

    if let Err(e) = fs::rename(&tmp_path, &file_path) {
        if file_path.exists() {
            let _ = fs::remove_file(&file_path);
        }
        if let Err(replace_err) = fs::rename(&tmp_path, &file_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!("Atomic config swap failed: {e} / {replace_err}"));
        }
    }

    Ok(())
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
        is_portable: is_portable_mode(),
        config_dir: config_dir().display().to_string(),
    }
}

pub fn set_mappings(mappings: Vec<Mapping>) -> Result<(), String> {
    let e = engine();
    let compiled = compile_mappings(&mappings);
    e.compiled.store(Arc::new(compiled));
    let cfg = {
        let mut w = e.cfg.write();
        w.mappings = mappings;
        w.clone()
    };
    let _ = e.cmd_tx.try_send(InputCmd::ResetState);
    save_config(&cfg)
}

pub fn set_theme(theme: String) -> Result<(), String> {
    let e = engine();
    let cfg = {
        let mut w = e.cfg.write();
        w.theme = theme;
        w.clone()
    };
    save_config(&cfg)
}

pub fn set_paused(paused: bool) -> Result<(), String> {
    let e = engine();
    e.paused.store(paused, Ordering::SeqCst);
    if paused {
        let _ = e.cmd_tx.try_send(InputCmd::ResetState);
    }
    let cfg = {
        let mut w = e.cfg.write();
        w.paused = paused;
        w.clone()
    };
    save_config(&cfg)
}

pub fn set_autostart_flag(on: bool) -> Result<(), String> {
    let e = engine();
    let cfg = {
        let mut w = e.cfg.write();
        w.autostart = on;
        w.clone()
    };
    save_config(&cfg)
}

pub fn arm_listen() {
    engine().listening.store(true, Ordering::Relaxed);
}

pub fn arm_record() {
    let e = engine();
    e.recorder.write().reset();
    e.recording.store(true, Ordering::Relaxed);
}

pub fn disarm_record() {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
    e.recorder.write().reset();
}

pub fn add_record_key(key: String) {
    let e = engine();
    let chord = e.recorder.write().add_chip(key);
    let _ = e.cmd_tx.try_send(InputCmd::Record(chord));
}

pub fn take_record_keys() -> Vec<String> {
    let e = engine();
    e.recording.store(false, Ordering::Relaxed);
    let keys = e.recorder.read().max_chord.clone();
    e.recorder.write().reset();
    keys
}

pub fn shutdown() {
    if let Some(e) = ENGINE.get() {
        e.paused.store(true, Ordering::SeqCst);
        e.recording.store(false, Ordering::Relaxed);
        e.listening.store(false, Ordering::Relaxed);
        let _ = e.cmd_tx.try_send(InputCmd::ResetState);
    }
    let tid = HOOK_TID.load(Ordering::Relaxed);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn start(
    on_pulse: impl Fn(Pulse) + Send + 'static,
    on_listen: impl Fn(String) + Send + 'static,
    on_record: impl Fn(Vec<String>) + Send + 'static,
    on_record_cancel: impl Fn() + Send + 'static,
    on_emergency_pause: impl Fn(bool) + Send + 'static,
    on_send_error: impl Fn(SendReport) + Send + 'static,
) {
    let (cmd_tx, cmd_rx) = bounded::<InputCmd>(64);
    let (telem_tx, telem_rx) = bounded::<Pulse>(16);

    let cfg = load_config();
    let paused = cfg.paused;
    let compiled = compile_mappings(&cfg.mappings);

    let _ = ENGINE.set(Engine {
        cfg: RwLock::new(cfg),
        compiled: ArcSwap::from_pointee(compiled),
        paused: AtomicBool::new(paused),
        listening: AtomicBool::new(false),
        recording: AtomicBool::new(false),
        window_visible: AtomicBool::new(false),
        last: RwLock::new(None),
        last_send_error: RwLock::new(None),
        cmd_tx,
        telem_tx,
        recorder: RwLock::new(RecorderState::default()),
    });

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
            worker_loop(
                cmd_rx,
                on_listen,
                on_record,
                on_record_cancel,
                on_emergency_pause,
                on_send_error,
            )
        })
        .expect("worker thread");

    thread::Builder::new()
        .name("mi-hook".into())
        .spawn(hook_loop)
        .expect("hook thread");
}

fn worker_loop(
    rx: Receiver<InputCmd>,
    on_listen: impl Fn(String),
    on_record: impl Fn(Vec<String>),
    on_record_cancel: impl Fn(),
    on_emergency_pause: impl Fn(bool),
    on_send_error: impl Fn(SendReport),
) {
    let mut state = InputState::default();

    while let Ok(cmd) = rx.recv() {
        let e = engine();
        match cmd {
            InputCmd::Fire { button, down } => {
                if e.paused.load(Ordering::Relaxed) {
                    continue;
                }
                apply_input_state(&mut state, button, down, &on_send_error);
            }
            InputCmd::ResetState => {
                state.reset_all();
            }
            InputCmd::EmergencyPause => {
                let current = e.paused.load(Ordering::SeqCst);
                let next = !current;
                e.paused.store(next, Ordering::SeqCst);
                if next {
                    state.reset_all();
                }
                {
                    let mut w = e.cfg.write();
                    w.paused = next;
                    let cfg_clone = w.clone();
                    drop(w);
                    let _ = save_config(&cfg_clone);
                }
                on_emergency_pause(next);
            }
            InputCmd::ListenCaptured(btn) => {
                on_listen(btn);
            }
            InputCmd::Record(keys) => on_record(keys),
            InputCmd::RecordCancel => on_record_cancel(),
        }
    }
}

fn apply_input_state(
    state: &mut InputState,
    button: MouseButton,
    down: bool,
    on_send_error: &impl Fn(SendReport),
) {
    let e = engine();
    let compiled_guard = e.compiled.load();
    let Some(action) = compiled_guard.get(button) else {
        if !down {
            if let Some(binding) = state.active_bindings.remove(&button) {
                if binding.mode == TriggerMode::Hold {
                    release_active_specs(state, &binding.specs, on_send_error);
                }
            }
        }
        return;
    };

    match action.mode {
        TriggerMode::Hold => {
            if down {
                if state.active_bindings.contains_key(&button) {
                    return;
                }
                let binding = ActiveBinding {
                    specs: action.specs.clone(),
                    mode: TriggerMode::Hold,
                };
                press_active_specs(state, &action.specs, on_send_error);
                state.active_bindings.insert(button, binding);
            } else if let Some(binding) = state.active_bindings.remove(&button) {
                release_active_specs(state, &binding.specs, on_send_error);
            }
        }
        TriggerMode::Toggle => {
            if !down {
                return;
            }
            let is_on = state.toggle_on.entry(button).or_insert(false);
            if *is_on {
                *is_on = false;
                if let Some(binding) = state.active_bindings.remove(&button) {
                    release_active_specs(state, &binding.specs, on_send_error);
                }
            } else {
                *is_on = true;
                let binding = ActiveBinding {
                    specs: action.specs.clone(),
                    mode: TriggerMode::Toggle,
                };
                press_active_specs(state, &action.specs, on_send_error);
                state.active_bindings.insert(button, binding);
            }
        }
        TriggerMode::Click => {
            if down {
                press_specs_once(&action.specs, on_send_error);
            }
        }
    }
}

fn press_active_specs(
    state: &mut InputState,
    specs: &[KeySpec],
    on_send_error: &impl Fn(SendReport),
) {
    let mut to_press: Vec<KeySpec> = Vec::new();
    for s in specs {
        let count = state.key_refs.entry(*s).or_insert(0);
        *count += 1;
        if *count == 1 {
            to_press.push(*s);
        }
    }
    if !to_press.is_empty() {
        let inputs: Vec<INPUT> = to_press.iter().map(|s| make_input(s, true)).collect();
        if let Err(report) = execute_send_inputs(&inputs) {
            *engine().last_send_error.write() = Some(report.clone());
            on_send_error(report);
        }
    }
}

fn release_active_specs(
    state: &mut InputState,
    specs: &[KeySpec],
    on_send_error: &impl Fn(SendReport),
) {
    let mut to_release: Vec<KeySpec> = Vec::new();
    let mut had_alt_or_win = false;

    for s in specs.iter().rev() {
        if let Some(count) = state.key_refs.get_mut(s) {
            if *count > 0 {
                *count -= 1;
                if *count == 0 {
                    to_release.push(*s);
                    if is_alt_or_win(s.vk) {
                        had_alt_or_win = true;
                    }
                }
            }
        }
    }

    if !to_release.is_empty() {
        let mut inputs: Vec<INPUT> = Vec::with_capacity(to_release.len() + 2);
        if had_alt_or_win {
            inputs.push(make_raw_input(VK_MASK_KEY, false, true));
        }
        for s in to_release.iter() {
            inputs.push(make_input(s, false));
        }
        if had_alt_or_win {
            inputs.push(make_raw_input(VK_MASK_KEY, false, false));
        }
        if let Err(report) = execute_send_inputs(&inputs) {
            *engine().last_send_error.write() = Some(report.clone());
            on_send_error(report);
        }
    }
}

fn press_specs_once(specs: &[KeySpec], on_send_error: &impl Fn(SendReport)) {
    if specs.is_empty() {
        return;
    }

    // 1. 发送所有按键的按下事件
    let down_inputs: Vec<INPUT> = specs.iter().map(|s| make_input(s, true)).collect();
    if let Err(report) = execute_send_inputs(&down_inputs) {
        *engine().last_send_error.write() = Some(report.clone());
        on_send_error(report);
        return;
    }

    // 2. 关键：提供 30ms 的硬件级按键停留时间（Dwell Time）
    // Windows 应用程序（如各类输入框、聊天软件、浏览器、游戏）的消息循环依赖 GetKeyState 校验
    // 若 0 延迟同时发送按下与松开，应用程序在处理 KeyDown 时按键状态已处于释放，会被当作毛刺忽略丢弃
    std::thread::sleep(std::time::Duration::from_millis(30));

    // 3. 释放按键
    let had_alt_or_win = specs.iter().any(|s| is_alt_or_win(s.vk));
    let mut up_inputs: Vec<INPUT> = Vec::with_capacity(specs.len() + 2);
    if had_alt_or_win {
        up_inputs.push(make_raw_input(VK_MASK_KEY, false, true));
    }
    for s in specs.iter().rev() {
        up_inputs.push(make_input(s, false));
    }
    if had_alt_or_win {
        up_inputs.push(make_raw_input(VK_MASK_KEY, false, false));
    }

    if let Err(report) = execute_send_inputs(&up_inputs) {
        *engine().last_send_error.write() = Some(report.clone());
        on_send_error(report);
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
        if let Some(eng) = ENGINE.get() {
            let _ = eng.cmd_tx.try_send(InputCmd::ResetState);
        }
    }
}

unsafe extern "system" fn kbd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if kb.flags.0 & LLKHF_INJECTED_KBD != 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let msg = wparam.0 as u32;
    let down = kb.flags.0 & LLKHF_UP == 0 && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN);

    if down && (kb.vkCode == VK_PAUSE.0 as u32 || kb.vkCode == VK_SCROLL.0 as u32) {
        if let Some(e) = ENGINE.get() {
            let _ = e.cmd_tx.try_send(InputCmd::EmergencyPause);
        }
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some(eng) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    if !eng.recording.load(Ordering::Relaxed) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if down && kb.vkCode == VK_ESCAPE.0 as u32 {
        eng.recording.store(false, Ordering::Relaxed);
        let _ = eng.cmd_tx.try_send(InputCmd::RecordCancel);
        return LRESULT(1);
    }

    let is_key_event = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN || msg == WM_KEYUP || msg == WM_SYSKEYUP;
    if is_key_event {
        let chord = eng.recorder.write().on_key(kb.vkCode, down);
        let _ = eng.cmd_tx.try_send(InputCmd::Record(chord));
    }

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

    let Some((button, down)) = classify(wparam.0 as u32, ms.mouseData) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    if down && eng.listening.swap(false, Ordering::Relaxed) {
        let _ = eng.cmd_tx.try_send(InputCmd::ListenCaptured(button.as_str().to_string()));
    }

    let paused = eng.paused.load(Ordering::Relaxed);
    let compiled_guard = eng.compiled.load();
    let action_opt = compiled_guard.get(button);
    let is_mapped = action_opt.is_some() && !paused;
    let swallow = is_mapped && !button.is_primary();

    if is_mapped {
        let _ = eng.cmd_tx.try_send(InputCmd::Fire { button, down });
        if button.is_wheel() {
            let _ = eng.cmd_tx.try_send(InputCmd::Fire {
                button,
                down: false,
            });
        }
    }

    let need_telemetry = eng.window_visible.load(Ordering::Relaxed)
        || eng.listening.load(Ordering::Relaxed);

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

#[inline(always)]
fn classify(msg: u32, mouse_data: u32) -> Option<(MouseButton, bool)> {
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
        WM_MOUSEWHEEL => {
            let delta = xhi as i16;
            if delta > 0 {
                Some((MouseButton::WheelUp, true))
            } else {
                Some((MouseButton::WheelDown, true))
            }
        }
        WM_MOUSEHWHEEL => None,
        _ => None,
    }
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

#[inline(always)]
fn is_alt_or_win(vk: VIRTUAL_KEY) -> bool {
    matches!(vk, VK_LMENU | VK_RMENU | VK_MENU | VK_LWIN | VK_RWIN)
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
    let _ = execute_send_inputs(&[input]);
}

fn send_mask_key() {
    let inputs = [
        make_raw_input(VK_MASK_KEY, false, true),
        make_raw_input(VK_MASK_KEY, false, false),
    ];
    let _ = execute_send_inputs(&inputs);
}

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
        assert_eq!(MouseButton::from_str_fast("xbutton1"), Some(MouseButton::XButton1));
        assert_eq!(MouseButton::from_str_fast("xbutton2"), Some(MouseButton::XButton2));
        assert_eq!(MouseButton::from_str_fast("middle"), Some(MouseButton::Middle));
        assert_eq!(MouseButton::XButton1.as_str(), "xbutton1");
    }

    #[test]
    fn test_compile_mappings_precompilation() {
        let mappings = vec![
            Mapping {
                id: "1".into(),
                button: "xbutton1".into(),
                mode: "hold".into(),
                keys: vec!["LControl".into(), "LAlt".into()],
                label: "".into(),
            },
        ];
        let compiled = compile_mappings(&mappings);
        let action = compiled.get(MouseButton::XButton1).expect("compiled slot");
        assert_eq!(action.mode, TriggerMode::Hold);
        assert_eq!(action.specs.len(), 2);
    }
}

