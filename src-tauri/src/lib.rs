mod engine;
mod native_menu;
#[cfg(target_os = "macos")]
mod macos;

use engine::{Mapping, Pulse, RuntimeBindingState, SendReport, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

static WINDOW_ACTIVE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn get_hook_status() -> String { engine::hook_status() }

#[tauri::command]
fn get_snapshot() -> Snapshot {
    engine::snapshot()
}

#[tauri::command]
async fn save_mappings(mappings: Vec<Mapping>) -> Result<(), String> {
    let result = tauri::async_runtime::spawn_blocking(move || engine::set_mappings(mappings)).await.map_err(|e| e.to_string())?;
    native_menu::refresh();
    result
}

#[tauri::command]
async fn save_theme(theme: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || engine::set_theme(theme)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_paused(app: tauri::AppHandle, paused: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || sync_engine_paused_state(&app, paused))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_autostart(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || engine::set_autostart_flag(on)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn arm_listen() {
    engine::arm_listen();
}

#[tauri::command]
fn disarm_listen() { engine::disarm_listen(); }

#[tauri::command]
fn arm_record() {
    engine::arm_record();
}

#[tauri::command]
fn disarm_record() {
    engine::disarm_record();
}

#[tauri::command]
fn add_record_key(key: String) {
    engine::add_record_key(key);
}

#[tauri::command]
fn remove_record_key(key: String) {
    engine::remove_record_key(key);
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if url != "https://github.com/lingcang728/MouseInsight/releases/latest"
        && url != "https://github.com/lingcang728/MouseInsight/releases" {
        return Err("Only Mouse Insight release pages can be opened".into());
    }
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("open url failed: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("open url failed: {e}"))?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = url;
        Err("open_url is only implemented on Windows and macOS".into())
    }
}

#[tauri::command]
fn take_record_keys() -> Vec<String> {
    engine::take_record_keys()
}

#[tauri::command]
fn xmbc_running() -> bool {
    engine::xmbc_running()
}

#[tauri::command]
fn config_dir() -> String {
    engine::config_dir().display().to_string()
}

#[tauri::command]
fn open_config_dir() -> Result<(), String> {
    let dir = engine::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create config directory failed: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("explorer").arg(dir).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let status = Command::new("/usr/bin/open").arg(dir).status().map_err(|e| e.to_string())?;
        if !status.success() { return Err("无法打开配置目录".into()); }
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    engine::shutdown();
    app.exit(0);
}

pub fn sync_engine_paused_state(app: &tauri::AppHandle, paused: bool) -> Result<(), String> {
    let result = engine::set_paused(paused);
    sync_paused_ui(app, paused);
    result
}

fn sync_paused_ui(app: &tauri::AppHandle, paused: bool) {

    native_menu::refresh();

    let _ = app.emit(
        "engine-state-changed",
        serde_json::json!({ "paused": paused }),
    );
}

fn bring_hwnd_to_front(w: &tauri::WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    #[cfg(target_os = "windows")]
    {
        let _ = w.set_always_on_top(true);
        let _ = w.set_always_on_top(false);
    }

    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = w.hwnd() {
        unsafe {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
                SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
            };

            let win_hwnd = HWND(hwnd.0 as _);
            let fore_hwnd = GetForegroundWindow();
            let fore_tid = GetWindowThreadProcessId(fore_hwnd, None);
            let cur_tid = GetCurrentThreadId();

            if fore_tid != cur_tid && fore_tid != 0 {
                let _ = AttachThreadInput(cur_tid, fore_tid, true);
                let _ = ShowWindow(win_hwnd, SW_RESTORE);
                let _ = ShowWindow(win_hwnd, SW_SHOW);
                let _ = BringWindowToTop(win_hwnd);
                let _ = SetForegroundWindow(win_hwnd);
                let _ = AttachThreadInput(cur_tid, fore_tid, false);
            } else {
                let _ = ShowWindow(win_hwnd, SW_RESTORE);
                let _ = ShowWindow(win_hwnd, SW_SHOW);
                let _ = BringWindowToTop(win_hwnd);
                let _ = SetForegroundWindow(win_hwnd);
            }
        }
    }
}

fn create_main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    match WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Mouse Insight")
        .inner_size(1180.0, 760.0)
        .min_inner_size(920.0, 620.0)
        .center()
        .decorations(true)
        .visible(true)
        .build()
    {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("[MouseInsight] failed to create main window: {err}");
            None
        }
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    { let _ = app.show(); }
    if let Some(w) = app.get_webview_window("main") {
        bring_hwnd_to_front(&w);
        engine::set_window_visible(true);
        WINDOW_ACTIVE.store(true, Ordering::Relaxed);
        return;
    }

    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app_handle.get_webview_window("main") {
            bring_hwnd_to_front(&w);
            engine::set_window_visible(true);
            WINDOW_ACTIVE.store(true, Ordering::Relaxed);
            return;
        }
        if let Some(w) = create_main_window(&app_handle) {
            bring_hwnd_to_front(&w);
            engine::set_window_visible(true);
            WINDOW_ACTIVE.store(true, Ordering::Relaxed);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_autostart = std::env::args().any(|arg| arg == "--autostart");
    let quit_requested = std::env::args().any(|arg| arg == "--quit");

    let builder = tauri::Builder::default()
        .on_menu_event(native_menu::on_menu_event)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|arg| arg == "--quit") {
                engine::shutdown();
                app.exit(0);
                return;
            }
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .on_window_event(|_window, event| match event {
            WindowEvent::CloseRequested { .. } => {
                engine::disarm_record();
                engine::disarm_listen();
                engine::set_window_visible(false);
                WINDOW_ACTIVE.store(false, Ordering::Relaxed);
            }
            WindowEvent::Destroyed => {
                engine::disarm_record();
                engine::disarm_listen();
                engine::set_window_visible(false);
                WINDOW_ACTIVE.store(false, Ordering::Relaxed);
            }
            WindowEvent::Focused(false) => {
                engine::disarm_record();
                engine::disarm_listen();
                let _ = _window.emit("record-cancel", ());
            }
            _ => {}
        })
        .setup(move |app| {
            if quit_requested {
                app.handle().exit(0);
                return Ok(());
            }
            let handle = app.handle().clone();
            let handle2 = app.handle().clone();
            let handle3 = app.handle().clone();
            let handle4 = app.handle().clone();
            let handle_pause = app.handle().clone();
            let handle_err = app.handle().clone();
            let handle_state = app.handle().clone();

            engine::start(
                move |pulse: Pulse| {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle.emit("mouse-pulse", pulse);
                    }
                },
                move |button: String| {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle2.emit("listen-captured", button);
                    }
                },
                move |keys: Vec<String>| {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle3.emit("record-keys", keys);
                    }
                },
                move || {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle4.emit("record-cancel", ());
                    }
                },
                move |paused: bool| {
                    sync_paused_ui(&handle_pause, paused);
                },
                move |report: SendReport| {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle_err.emit("injection-error", report);
                    }
                },
                move |state: RuntimeBindingState| {
                    if WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        let _ = handle_state.emit("runtime-binding-changed", state);
                    }
                },
            );

            native_menu::setup(app)?;

            if !is_autostart {
                show_main_window(&app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_hook_status,
            save_mappings,
            save_theme,
            save_paused,
            save_autostart,
            arm_listen,
            disarm_listen,
            arm_record,
            disarm_record,
            add_record_key,
            remove_record_key,
            take_record_keys,
            app_version,
            open_url,
            xmbc_running,
            config_dir,
            open_config_dir,
            quit_app
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Mouse Insight");

    app.run(|_app_handle, event| {
        #[cfg(target_os = "macos")]
        if matches!(event, tauri::RunEvent::Reopen { .. }) {
            show_main_window(_app_handle);
        }
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
