#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!("Mouse Insight supports Windows and macOS only");

mod engine;
#[cfg(target_os = "macos")]
mod macos;
mod native_menu;

use engine::{Mapping, Pulse, RuntimeBindingState, SendReport, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

static WINDOW_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Marks a user-initiated window close so `ExitRequested(code=None)` can
/// tell "last window closed → stay resident in tray" from a real
/// termination request (Dock Quit, system logoff) — both arrive as
/// `code=None`.
static LAST_WINDOW_CLOSE: Mutex<Option<Instant>> = Mutex::new(None);

/// `window_visible` gates hook-side telemetry generation and WINDOW_ACTIVE
/// gates IPC emission; they always move together.
fn mark_window_active(active: bool) {
    engine::set_window_visible(active);
    WINDOW_ACTIVE.store(active, Ordering::Relaxed);
}

#[tauri::command]
fn get_hook_status() -> String {
    engine::hook_status()
}

#[tauri::command]
fn get_snapshot() -> Snapshot {
    engine::snapshot()
}

#[tauri::command]
async fn save_mappings(mappings: Vec<Mapping>) -> Result<(), String> {
    // Earliest explicit cap; engine::set_mappings re-validates everything.
    if mappings.len() > 5 {
        return Err("最多支持 5 个鼠标按键映射".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || engine::set_mappings(mappings))
        .await
        .map_err(|e| e.to_string())?;
    native_menu::refresh();
    result
}

#[tauri::command]
fn clear_recovery_notes() {
    engine::clear_recovery_notes();
}

#[tauri::command]
async fn save_theme(theme: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || engine::set_theme(theme))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_paused(app: tauri::AppHandle, paused: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || sync_engine_paused_state(&app, paused))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_autostart(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || engine::set_autostart_flag(on))
        .await
        .map_err(|e| e.to_string())?
}

/// "In foreground" means the foreground window shares our root ancestor.
/// `Window::is_focused()` is unusable here: under WebView2 the keyboard focus
/// lives on the webview child HWND, so the top-level window reports false even
/// while it is visibly frontmost.
fn window_in_foreground(window: &tauri::Window) -> bool {
    if !window.is_visible().unwrap_or(false) {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetForegroundWindow, GA_ROOT};
        let Ok(hwnd) = window.hwnd() else {
            return false;
        };
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.0.is_null() {
                return false;
            }
            GetAncestor(foreground, GA_ROOT) == GetAncestor(HWND(hwnd.0 as _), GA_ROOT)
        }
    }
    #[cfg(target_os = "macos")]
    {
        window.is_focused().unwrap_or(false)
    }
}

#[tauri::command]
fn arm_listen(window: tauri::Window) -> Result<u64, String> {
    if !window_in_foreground(&window) {
        return Err("窗口不在前台，已取消".into());
    }
    Ok(engine::arm_listen())
}

#[tauri::command]
fn disarm_listen() {
    engine::disarm_listen();
}

#[tauri::command]
fn arm_record(window: tauri::Window) -> Result<u64, String> {
    if !window_in_foreground(&window) {
        return Err("窗口不在前台，已取消".into());
    }
    Ok(engine::arm_record())
}

#[tauri::command]
fn disarm_record() {
    engine::disarm_record();
}

// The record-family commands share arm_record's foreground gate: disarming is
// always allowed, but anything that can read or mutate an armed recording
// session must come from a frontmost window, not a compromised renderer.
#[tauri::command]
fn add_record_key(window: tauri::Window, key: String) {
    if !window_in_foreground(&window) {
        return;
    }
    engine::add_record_key(key);
}

#[tauri::command]
fn remove_record_key(window: tauri::Window, key: String) {
    if !window_in_foreground(&window) {
        return;
    }
    engine::remove_record_key(key);
}

#[tauri::command]
fn press_record_key(window: tauri::Window, key: String, down: bool) {
    if !window_in_foreground(&window) {
        return;
    }
    engine::press_record_key(key, down);
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // SECURITY: exact-match whitelist only. Never relax to starts_with(): cmd /C start re-parses & and | as command separators.
    if url != "https://github.com/lingcang728/MouseInsight/releases/latest"
        && url != "https://github.com/lingcang728/MouseInsight/releases"
        && url != "https://developer.microsoft.com/microsoft-edge/webview2/"
    {
        return Err("Only Mouse Insight release pages can be opened".into());
    }
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let cmd = std::env::var_os("SystemRoot")
            .map(|root| std::path::PathBuf::from(root).join("System32\\cmd.exe"))
            .unwrap_or_else(|| "cmd".into());
        Command::new(cmd)
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
fn take_record_keys(window: tauri::Window) -> Vec<String> {
    if !window_in_foreground(&window) {
        return Vec::new();
    }
    engine::take_record_keys()
}

#[tauri::command]
fn xmbc_running() -> bool {
    engine::xmbc_running()
}

fn open_dir(dir: std::path::PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("create directory failed: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let explorer = std::env::var_os("SystemRoot")
            .map(|root| std::path::PathBuf::from(root).join("explorer.exe"))
            .unwrap_or_else(|| "explorer".into());
        Command::new(explorer)
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // Do not wait: `open` can block while Finder shows a dialog.
        Command::new("/usr/bin/open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Re-arm the input hook after a recoverable failure — e.g. right after the
/// user grants Accessibility permission on macOS.
#[tauri::command]
fn retry_hook() {
    engine::retry_hook();
}

#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("仅 macOS".into())
    }
}

#[tauri::command]
fn open_config_dir() -> Result<(), String> {
    open_dir(engine::config_dir())
}

#[tauri::command]
fn open_logs_dir() -> Result<(), String> {
    open_dir(engine::config_dir().join("logs"))
}

#[tauri::command]
async fn quit_app(app: tauri::AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(engine::shutdown).await;
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
        serde_json::json!({ "paused": paused, "emergency": engine::paused_emergency() }),
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

/// Top-level navigations are confined to the app's own origins; a remote page
/// has no IPC bridge but would still be a phishing surface wearing our window.
fn navigation_allowed(url: &tauri::Url) -> bool {
    let allowed = matches!(url.scheme(), "tauri" | "ipc")
        || matches!(
            url.host_str(),
            Some("tauri.localhost") | Some("ipc.localhost")
        )
        || url.as_str() == "about:blank"
        // Debug builds load the UI from the Vite dev server.
        || (cfg!(debug_assertions)
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost") | Some("127.0.0.1")));
    if !allowed {
        log::warn!("blocked webview navigation to {url}");
    }
    allowed
}

fn create_main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    // Small/high-DPI work areas (e.g. 1280x800@150% -> 853x533 logical) can be
    // smaller than the fixed minimum; Tauri does not clamp it, so size from
    // the monitor instead.
    let (mut width, mut height) = (1180.0_f64, 760.0_f64);
    let (mut min_w, mut min_h) = (920.0_f64, 620.0_f64);
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let work = monitor.work_area().size;
        let (avail_w, avail_h) = (work.width as f64 / scale, work.height as f64 / scale);
        if avail_w > 0.0 && avail_h > 0.0 {
            width = width.min(avail_w);
            height = height.min(avail_h);
            min_w = min_w.min(avail_w * 0.9);
            min_h = min_h.min(avail_h * 0.9);
        }
    }
    match WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Mouse Insight")
        .on_navigation(navigation_allowed)
        .inner_size(width, height)
        .min_inner_size(min_w, min_h)
        .center()
        .decorations(true)
        .visible(true)
        .build()
    {
        Ok(w) => Some(w),
        Err(err) => {
            log::error!("failed to create main window: {err}");
            #[cfg(target_os = "windows")]
            {
                use windows::core::w;
                use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
                unsafe {
                    let _ = MessageBoxW(
                        None,
                        w!("Mouse Insight 无法创建窗口。通常是缺少 Microsoft Edge WebView2 运行时，请安装后重试。"),
                        w!("Mouse Insight"),
                        MB_OK | MB_ICONERROR,
                    );
                }
                let _ = open_url(
                    "https://developer.microsoft.com/microsoft-edge/webview2/".to_string(),
                );
            }
            None
        }
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.show();
    }
    if let Some(w) = app.get_webview_window("main") {
        bring_hwnd_to_front(&w);
        mark_window_active(true);
        return;
    }

    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app_handle.get_webview_window("main") {
            bring_hwnd_to_front(&w);
            mark_window_active(true);
            return;
        }
        if let Some(w) = create_main_window(&app_handle) {
            bring_hwnd_to_front(&w);
            mark_window_active(true);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A parent process can inherit a WebView2 CDP/debug command line into us
    // through this variable; release builds drop it before any webview spins
    // up. scripts/verify-ui.py opts back in via MOUSE_INSIGHT_ALLOW_WEBVIEW2_ARGS.
    #[cfg(all(target_os = "windows", not(debug_assertions)))]
    if std::env::var_os("MOUSE_INSIGHT_ALLOW_WEBVIEW2_ARGS").is_none() {
        std::env::remove_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");
    }

    // Must work before the engine (and even the logger) exists.
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let text = format!("panic: {msg}\nthread: {thread}\nlocation: {location}\n");
        log::error!("{text}");
        // A panic must not leave injected keys held; the try-lock variant
        // never blocks on a ledger lock the panicking thread may hold.
        engine::failsafe_release_all_try();
        let dir = engine::config_dir().join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::write(dir.join(format!("crash-{ts}.log")), text);
        // Cap crash logs; a crash loop must not fill the disk. The unix-ts
        // names sort chronologically for the next decade.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut logs: Vec<_> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("crash-") && n.ends_with(".log")
                        })
                        .unwrap_or(false)
                })
                .collect();
            logs.sort();
            while logs.len() > 5 {
                let _ = std::fs::remove_file(logs.remove(0));
            }
        }
    }));

    let is_autostart = std::env::args().any(|arg| arg == "--autostart");
    let quit_requested = std::env::args().any(|arg| arg == "--quit");

    #[allow(unused_mut)]
    let mut log_targets = vec![tauri_plugin_log::Target::new(
        tauri_plugin_log::TargetKind::Folder {
            path: engine::config_dir().join("logs"),
            file_name: Some("mouse-insight".into()),
        },
    )];
    #[cfg(debug_assertions)]
    log_targets.push(tauri_plugin_log::Target::new(
        tauri_plugin_log::TargetKind::Stdout,
    ));

    let builder = tauri::Builder::default()
        .on_menu_event(native_menu::on_menu_event)
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets(log_targets)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .max_file_size(1_048_576)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|arg| arg == "--quit") {
                engine::shutdown();
                app.exit(0);
                return;
            }
            if argv.iter().any(|arg| arg == "--autostart") {
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
                if let Ok(mut t) = LAST_WINDOW_CLOSE.lock() {
                    *t = Some(Instant::now());
                }
                engine::disarm_record();
                engine::disarm_listen();
                mark_window_active(false);
            }
            WindowEvent::Destroyed => {
                engine::disarm_record();
                engine::disarm_listen();
                mark_window_active(false);
            }
            // Neither platform has a dedicated minimized/hidden window event;
            // a Windows minimize arrives as a zero-size resize. macOS app
            // hide is additionally caught by the pulse emit check below.
            WindowEvent::Resized(size) => {
                mark_window_active(size.width != 0 && size.height != 0);
            }
            WindowEvent::Focused(false) => {
                engine::disarm_record();
                engine::disarm_listen();
                let _ = _window.emit("record-cancel", ());
            }
            // Regaining focus implies visible: covers restore paths that emit
            // no resize (unminimize, macOS unhide).
            WindowEvent::Focused(true) => {
                mark_window_active(true);
            }
            _ => {}
        })
        .setup(move |app| {
            if quit_requested {
                app.handle().exit(0);
                return Ok(());
            }
            // Menu-bar resident tool: keep a Dock slot from being held,
            // including on --autostart launches that never open a window.
            // show_main_window still activates the app when it opens.
            #[cfg(target_os = "macos")]
            let _ = app
                .handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let handle2 = app.handle().clone();
            let handle3 = app.handle().clone();
            let handle4 = app.handle().clone();
            let handle_pause = app.handle().clone();
            let handle_err = app.handle().clone();
            let handle_state = app.handle().clone();
            let handle_fatal = app.handle().clone();
            let handle_hook = app.handle().clone();

            engine::start(
                move |pulse: Pulse| {
                    if !WINDOW_ACTIVE.load(Ordering::Relaxed) {
                        return;
                    }
                    // A hidden window (macOS Cmd+H, or any minimize path that
                    // skipped the zero-size resize) still looks active here;
                    // discover it once so the hook stops generating telemetry
                    // and the hidden webview stops getting IPC wakeups.
                    let visible = handle
                        .get_webview_window("main")
                        .is_some_and(|w| w.is_visible().unwrap_or(false));
                    if !visible {
                        mark_window_active(false);
                        return;
                    }
                    let _ = handle.emit("mouse-pulse", pulse);
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
                // Fatal/hook-status events must reach a hidden window too:
                // the frontend re-reads them on next show.
                move |msg: String| {
                    let _ = handle_fatal.emit("engine-fatal", msg);
                    native_menu::refresh();
                },
                move |status: String| {
                    let _ = handle_hook.emit("hook-status-changed", status);
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
            press_record_key,
            app_version,
            open_url,
            xmbc_running,
            open_config_dir,
            open_logs_dir,
            retry_hook,
            open_accessibility_settings,
            clear_recovery_notes,
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
        // Dock Quit / AppleScript quit / system logoff bypass ExitRequested
        // on macOS (tauri#9198): Exit is the last reliable chance to release
        // injected keys. Idempotent — quit_app and --quit already ran it.
        if matches!(event, tauri::RunEvent::Exit) {
            engine::shutdown();
        }
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            // code=None covers both "the last window closed" (stay resident
            // in tray) and Dock Quit / system logoff. A close always emits
            // CloseRequested first, so a recent one marks the tray path;
            // anything else is a real termination — letting it through runs
            // the RunEvent::Exit arm above, which releases injected keys.
            let closing = LAST_WINDOW_CLOSE
                .lock()
                .map(|mut t| {
                    t.take()
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
                })
                .unwrap_or(false);
            if code.is_none() && closing {
                api.prevent_exit();
            }
        }
    });
}
