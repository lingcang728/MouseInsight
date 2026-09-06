mod engine;
#[cfg(target_os = "macos")]
mod macos;

use engine::{Mapping, Pulse, RuntimeBindingState, SendReport, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

static WINDOW_ACTIVE: AtomicBool = AtomicBool::new(false);

struct TrayState {
    pause_item: MenuItem<tauri::Wry>,
}

#[tauri::command]
fn get_snapshot() -> Snapshot {
    engine::snapshot()
}

#[tauri::command]
fn save_mappings(mappings: Vec<Mapping>) -> Result<(), String> {
    engine::set_mappings(mappings)
}

#[tauri::command]
fn save_theme(theme: String) -> Result<(), String> {
    engine::set_theme(theme)
}

#[tauri::command]
fn save_paused(app: tauri::AppHandle, paused: bool) -> Result<(), String> {
    sync_engine_paused_state(&app, paused);
    Ok(())
}

#[tauri::command]
fn save_autostart(on: bool) -> Result<(), String> {
    engine::set_autostart_flag(on)
}

#[tauri::command]
fn arm_listen() {
    engine::arm_listen();
}

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
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("explorer").arg(dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let _ = Command::new("open").arg(dir).spawn();
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    engine::shutdown();
    app.exit(0);
}

pub fn sync_engine_paused_state(app: &tauri::AppHandle, paused: bool) {
    let _ = engine::set_paused(paused);

    if let Some(tray) = app.tray_by_id("main") {
        let tip = if paused {
            "Mouse Insight · 已暂停"
        } else {
            "Mouse Insight · 已启用"
        };
        let _ = tray.set_tooltip(Some(tip));
    }

    if let Some(state) = app.try_state::<TrayState>() {
        let text = if paused {
            "恢复映射"
        } else {
            "暂停映射"
        };
        let _ = state.pause_item.set_text(text);
    }

    let _ = app.emit(
        "engine-state-changed",
        serde_json::json!({ "paused": paused }),
    );
}

fn bring_hwnd_to_front(w: &tauri::WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.set_always_on_top(true);
    let _ = w.set_always_on_top(false);

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

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .on_window_event(|_window, event| match event {
            WindowEvent::CloseRequested { .. } => {
                engine::disarm_record();
                engine::set_window_visible(false);
                WINDOW_ACTIVE.store(false, Ordering::Relaxed);
            }
            WindowEvent::Destroyed => {
                engine::disarm_record();
                engine::set_window_visible(false);
                WINDOW_ACTIVE.store(false, Ordering::Relaxed);
            }
            _ => {}
        })
        .setup(move |app| {
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
                    sync_engine_paused_state(&handle_pause, paused);
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

            let initial_snap = engine::snapshot();
            let is_paused = initial_snap.config.paused;

            let show_item = MenuItem::with_id(app, "show", "打开控制面板", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(
                app,
                "toggle_pause",
                if is_paused {
                    "恢复映射"
                } else {
                    "暂停映射"
                },
                true,
                None::<&str>,
            )?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show_item, &pause_item, &sep, &quit_item])?;
            app.manage(TrayState {
                pause_item: pause_item.clone(),
            });

            let tooltip = if is_paused {
                "Mouse Insight · 已暂停"
            } else {
                "Mouse Insight · 已启用"
            };

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip(tooltip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        show_main_window(app);
                    }
                    "toggle_pause" => {
                        let current_paused = engine::snapshot().config.paused;
                        sync_engine_paused_state(app, !current_paused);
                    }
                    "quit" => {
                        engine::shutdown();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        show_main_window(tray.app_handle());
                    }
                    _ => {}
                })
                .build(app)?;

            if !is_autostart {
                show_main_window(&app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            save_mappings,
            save_theme,
            save_paused,
            save_autostart,
            arm_listen,
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
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
