mod engine;

use engine::{Mapping, Pulse, Snapshot};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

#[tauri::command]
fn get_snapshot() -> Snapshot {
    engine::snapshot()
}

#[tauri::command]
fn save_mappings(mappings: Vec<Mapping>) {
    engine::set_mappings(mappings);
}

#[tauri::command]
fn save_theme(theme: String) {
    engine::set_theme(theme);
}

#[tauri::command]
fn save_paused(paused: bool) {
    engine::set_paused(paused);
}

#[tauri::command]
fn save_autostart(on: bool) {
    engine::set_autostart_flag(on);
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
fn quit_app(app: tauri::AppHandle) {
    engine::shutdown();
    app.exit(0);
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = w.hwnd() {
            unsafe {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
                    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                };
                use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};

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
    } else {
        // Fallback: Recreate window if somehow closed
        let _ = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("Mouse Insight")
            .inner_size(1180.0, 760.0)
            .min_inner_size(920.0, 620.0)
            .center()
            .decorations(true)
            .build();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                engine::disarm_record();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) => {
                engine::disarm_record();
            }
            _ => {}
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let handle2 = app.handle().clone();
            let handle3 = app.handle().clone();
            let handle4 = app.handle().clone();
            engine::start(
                move |pulse: Pulse| {
                    let _ = handle.emit("mouse-pulse", pulse);
                },
                move |button: String| {
                    let _ = handle2.emit("listen-captured", button);
                },
                move |keys: Vec<String>| {
                    let _ = handle3.emit("record-keys", keys);
                },
                move || {
                    let _ = handle4.emit("record-cancel", ());
                },
            );

            let show = MenuItem::with_id(app, "show", "打开", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("Mouse Insight")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        show_main_window(app);
                    }
                    "quit" => {
                        engine::shutdown();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                        | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            show_main_window(tray.app_handle());
                        }
                        _ => {}
                    }
                })
                .build(app)?;

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
            take_record_keys,
            xmbc_running,
            config_dir,
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

