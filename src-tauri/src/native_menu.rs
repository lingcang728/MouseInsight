//! Native menus only: no extra window, WebView, polling timer or UI thread I/O.
use crate::{engine, show_main_window};
use std::sync::OnceLock;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Emitter};

static APP: OnceLock<AppHandle> = OnceLock::new();

struct MenuState {
    status: MenuItem<tauri::Wry>,
    pause: MenuItem<tauri::Wry>,
    error: MenuItem<tauri::Wry>,
    slots: Vec<(String, String, Submenu<tauri::Wry>)>,
}

fn status_text(paused: bool, count: usize, hook: &str) -> String {
    if hook != "ready" {
        if hook == "starting" {
            "正在启动监听…".into()
        } else {
            "监听不可用 · 打开按键工作台查看".into()
        }
    } else if paused {
        "映射已暂停".into()
    } else if count == 0 {
        "尚未配置映射".into()
    } else {
        format!("映射已启用 · {count} 个按键")
    }
}

pub fn refresh() {
    let Some(app) = APP.get() else { return };
    let handle = app.clone();
    // Read state when this executes, so queued refreshes cannot restore stale UI.
    let _ = app.run_on_main_thread(move || {
        let Some(state) = handle.try_state::<MenuState>() else {
            return;
        };
        let (paused, count, hook) = engine::menu_summary();
        let text = status_text(paused, count, &hook);
        let mappings = engine::menu_mappings();
        for (button, slot, submenu) in &state.slots {
            let mapping = mappings.iter().find(|m| &m.button == button);
            let keys = mapping.map(|m| if slot == "hold" { &m.hold_keys } else { &m.tap_keys });
            let chord = keys.filter(|keys| !keys.is_empty()).map(|keys| keys.join(" + ")).unwrap_or_else(|| "未设置".into());
            let label = if button.starts_with("wheel") { "滚动" } else if slot == "hold" { "长按" } else { "短按" };
            let _ = submenu.set_text(format!("{label} · {chord}"));
            let _ = submenu.set_enabled(!mapping.is_some_and(|m| m.mode == "toggle"));
        }
        let _ = state.status.set_text(&text);
        let _ = state.pause.set_text(if paused {
            "恢复映射"
        } else {
            "暂停映射并释放按键"
        });
        if let Some(tray) = handle.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!("Mouse Insight · {text}")));
        }
    });
}

fn report_error(app: &AppHandle, result: Result<(), String>) {
    if let Some(state) = app.try_state::<MenuState>() {
        let _ = state.error.set_text(if result.is_err() {
            "操作失败 · 请在工作台检查设置"
        } else {
            "所有配置仅保存在本机"
        });
    }
    if let Err(err) = result {
        eprintln!("[MouseInsight] menu action: {err}");
    }
}

pub fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    if let Some(action) = event.id.as_ref().strip_prefix("quick:") {
        let parts: Vec<String> = action.split(':').map(str::to_owned).collect();
        if parts.len() == 3 {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = engine::set_quick_mapping(&parts[0], &parts[1], &parts[2]);
                if result.is_ok() { let _ = app.emit("mappings-changed", ()); }
                refresh();
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || report_error(&handle, result));
            });
        }
        return;
    }
    match event.id.as_ref() {
        "show" => show_main_window(app),
        "quit" => {
            let handle = app.clone();
            tauri::async_runtime::spawn_blocking(move || crate::quit_app(handle));
        }
        "toggle_pause" | "open_config" | "accessibility" | "releases" => {
            let app = app.clone();
            // Persistence and launching Finder/Explorer must never stall menu tracking.
            tauri::async_runtime::spawn_blocking(move || {
                let result = match event.id.as_ref() {
                    "toggle_pause" => engine::toggle_paused(),
                    "open_config" => crate::open_config_dir(),
                    "releases" => crate::open_url("https://github.com/lingcang728/MouseInsight/releases/latest".into()),
                    #[cfg(target_os = "macos")]
                    "accessibility" => std::process::Command::new("/usr/bin/open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                        .status().map_err(|e| e.to_string()).and_then(|s| {
                            if s.success() { Ok(()) } else { Err("无法打开辅助功能设置".into()) }
                        }),
                    _ => Ok(()),
                };
                refresh();
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || report_error(&handle, result));
            });
        }
        _ => {}
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let item = |id, text, shortcut: Option<&str>| MenuItem::with_id(app, id, text, true, shortcut);
    let sep = || PredefinedMenuItem::separator(app);
    let status = MenuItem::with_id(app, "status", "正在启动监听…", false, None::<&str>)?;
    let show = item("show", "打开按键工作台…", Some("CmdOrCtrl+Comma"))?;
    let pause = item(
        "toggle_pause",
        "暂停映射并释放按键",
        Some("CmdOrCtrl+Shift+P"),
    )?;
    let config = item("open_config", "打开配置目录…", None)?;
    let releases = item("releases", "查看最新版本…", None)?;
    let quit = item("quit", "退出 Mouse Insight", Some("CmdOrCtrl+Q"))?;
    let error = MenuItem::with_id(
        app,
        "operation_status",
        "所有配置仅保存在本机",
        false,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&status, &pause, &sep()?])?;
    let mut slots = Vec::new();
    for (button, label) in [("xbutton2", "前侧键"), ("xbutton1", "后侧键"), ("middle", "中键"), ("wheelup", "滚轮上"), ("wheeldown", "滚轮下")] {
        let button_menu = Submenu::new(app, label, true)?;
        for (slot, title) in [("tap", "短按"), ("hold", "长按")] {
            if button.starts_with("wheel") && slot == "hold" { continue; }
            let actions = Submenu::new(app, if button.starts_with("wheel") { "滚动操作" } else { title }, true)?;
            for (preset, title) in [("copy", "复制"), ("paste", "粘贴"), ("undo", "撤销"), ("enter", "回车"), ("clear", "清除绑定")] {
                actions.append(&MenuItem::with_id(app, format!("quick:{button}:{slot}:{preset}"), title, true, None::<&str>)?)?;
            }
            button_menu.append(&actions)?;
            slots.push((button.to_owned(), slot.to_owned(), actions));
        }
        menu.append(&button_menu)?;
    }
    menu.append_items(&[&sep()?, &show, &config])?;
    #[cfg(target_os = "macos")]
    {
        menu.append(&item("accessibility", "辅助功能设置…", None)?)?;
        // Keep standard editing, hiding and window shortcuts in the app menu.
        // Use our quit action so Cmd+Q releases injected keys before exiting.
        use tauri::menu::Submenu;
        let app_menu = Submenu::with_items(
            app,
            "Mouse Insight",
            true,
            &[
                &PredefinedMenuItem::about(app, Some("关于 Mouse Insight"), None)?,
                &sep()?,
                &show,
                &pause,
                &sep()?,
                &PredefinedMenuItem::services(app, Some("服务"))?,
                &sep()?,
                &PredefinedMenuItem::hide(app, Some("隐藏 Mouse Insight"))?,
                &PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?,
                &PredefinedMenuItem::show_all(app, Some("显示全部"))?,
                &sep()?,
                &quit,
            ],
        )?;
        let edit = Submenu::with_items(
            app,
            "编辑",
            true,
            &[
                &PredefinedMenuItem::undo(app, None)?,
                &PredefinedMenuItem::redo(app, None)?,
                &sep()?,
                &PredefinedMenuItem::cut(app, None)?,
                &PredefinedMenuItem::copy(app, None)?,
                &PredefinedMenuItem::paste(app, None)?,
                &PredefinedMenuItem::select_all(app, None)?,
            ],
        )?;
        let window = Submenu::with_items(
            app,
            "窗口",
            true,
            &[
                &PredefinedMenuItem::minimize(app, None)?,
                &PredefinedMenuItem::close_window(app, None)?,
            ],
        )?;
        app.set_menu(Menu::with_items(app, &[&app_menu, &edit, &window])?)?;
    }
    menu.append_items(&[&releases, &sep()?, &error, &quit])?;
    app.manage(MenuState {
        status,
        pause,
        error,
        slots,
    });
    let tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Mouse Insight")
        .show_menu_on_left_click(cfg!(target_os = "macos"));
    #[cfg(target_os = "macos")]
    let tray = tray.icon(template_icon()).icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let tray = tray
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    tray.build(app)?;
    let _ = APP.set(app.handle().clone());
    refresh();
    Ok(())
}

// A transparent 2x mouse silhouette. App icons have an opaque square background
// and cannot be used as NSImage templates (they turn into solid menu-bar blocks).
#[cfg(any(target_os = "macos", test))]
fn template_icon() -> tauri::image::Image<'static> {
    let mut rgba = vec![0; 36 * 36 * 4];
    for y in 0..36 {
        for x in 0..36 {
            let mut coverage = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0) / 2.0 - 9.0;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0) / 2.0;
                    let dy = (py - 9.0).abs() - 2.5;
                    let distance = (px * px + dy.max(0.0).powi(2)).sqrt();
                    let outline = (3.7..=5.1).contains(&distance);
                    let wheel = px.abs() < 0.65 && (4.5..=7.8).contains(&py);
                    if outline || wheel {
                        coverage += 1;
                    }
                }
            }
            rgba[(y * 36 + x) * 4 + 3] = (coverage * 255 / 16) as u8;
        }
    }
    tauri::image::Image::new_owned(rgba, 36, 36)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_hook_never_claims_mappings_are_running() {
        assert!(status_text(false, 5, "permission denied").contains("不可用"));
        assert!(status_text(true, 5, "starting").contains("启动"));
        assert_eq!(status_text(false, 0, "ready"), "尚未配置映射");
        assert_eq!(status_text(true, 3, "ready"), "映射已暂停");
        assert_eq!(status_text(false, 3, "ready"), "映射已启用 · 3 个按键");
    }
    #[test]
    fn template_has_transparent_padding_and_visible_antialiased_strokes() {
        let icon = template_icon();
        let alpha: Vec<_> = icon.rgba().chunks_exact(4).map(|p| p[3]).collect();
        assert!(alpha[..36].iter().all(|a| *a == 0));
        assert!(alpha[35 * 36..].iter().all(|a| *a == 0));
        assert!(alpha.contains(&255));
        assert!(alpha.iter().any(|a| *a > 0 && *a < 255));
        assert!(alpha.iter().filter(|a| **a > 0).count() < 36 * 36 / 2);
    }
}
