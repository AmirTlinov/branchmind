#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;
use tauri::Manager;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[derive(Clone)]
struct ViewerApiConfig {
    base_url: String,
}

#[tauri::command]
fn viewer_api_get_json(
    config: tauri::State<'_, ViewerApiConfig>,
    path: String,
) -> Result<Value, String> {
    let path = path.trim();
    if !path.starts_with("/api/") {
        return Err("path: expected /api/*".to_string());
    }
    if path.contains("://") || path.contains('\n') || path.contains('\r') {
        return Err("path: invalid characters".to_string());
    }

    let url = format!("{}{}", config.base_url, path);
    match ureq::get(&url).call() {
        Ok(resp) => resp
            .into_json::<Value>()
            .map_err(|err| format!("invalid json: {err}")),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(format!(
                "HTTP {code}: {}",
                body.chars().take(400).collect::<String>()
            ))
        }
        Err(err) => Err(format!("request failed: {err}")),
    }
}

#[tauri::command]
fn viewer_api_post_json(
    config: tauri::State<'_, ViewerApiConfig>,
    path: String,
    body: String,
) -> Result<Value, String> {
    let path = path.trim();
    if !path.starts_with("/api/") {
        return Err("path: expected /api/*".to_string());
    }
    if path.contains("://") || path.contains('\n') || path.contains('\r') {
        return Err("path: invalid characters".to_string());
    }
    if body.len() > 256 * 1024 {
        return Err("body: too large".to_string());
    }

    let url = format!("{}{}", config.base_url, path);
    let req = ureq::post(&url).set("Content-Type", "application/json");
    match req.send_string(&body) {
        Ok(resp) => resp
            .into_json::<Value>()
            .map_err(|err| format!("invalid json: {err}")),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(format!(
                "HTTP {code}: {}",
                body.chars().take(400).collect::<String>()
            ))
        }
        Err(err) => Err(format!("request failed: {err}")),
    }
}

fn parse_viewer_port() -> u16 {
    const DEFAULT_VIEWER_PORT: u16 = 7331;
    let mut cli: Option<String> = None;
    let mut saw_flag = false;
    for arg in std::env::args().skip(1) {
        if saw_flag {
            cli = Some(arg);
            break;
        }
        saw_flag = arg.as_str() == "--viewer-port";
    }

    let raw = cli.or_else(|| std::env::var("BRANCHMIND_VIEWER_PORT").ok());
    let Some(raw) = raw else {
        return DEFAULT_VIEWER_PORT;
    };
    match raw.trim().parse::<u16>().ok() {
        Some(0) | None => DEFAULT_VIEWER_PORT,
        Some(value) => value,
    }
}

fn parse_start_hidden() -> bool {
    for arg in std::env::args().skip(1) {
        if arg.as_str() == "--start-hidden" {
            return true;
        }
    }

    match std::env::var("BRANCHMIND_VIEWER_TAURI_START_HIDDEN") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.hide();
}

fn show_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(true);
    if visible {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    let viewer_port = parse_viewer_port();
    let start_hidden = parse_start_hidden();
    let api_base_url = format!("http://127.0.0.1:{viewer_port}");

    let is_quitting = Arc::new(AtomicBool::new(false));

    let is_quitting_window = is_quitting.clone();
    let is_quitting_setup = is_quitting.clone();

    let app = tauri::Builder::default()
        .manage(ViewerApiConfig {
            base_url: api_base_url,
        })
        .invoke_handler(tauri::generate_handler![
            viewer_api_get_json,
            viewer_api_post_json
        ])
        .setup(move |app| {
            let mut builder = tauri::WebviewWindowBuilder::new(
                app.handle(),
                "main",
                tauri::WebviewUrl::default(),
            )
            .title("BranchMind Viewer")
            .inner_size(1280.0, 800.0);

            if start_hidden {
                builder = builder.visible(false);
            }

            builder.build()?;

            // Tray icon + menu (Linux/Windows/macOS). Note: Linux tray click events are unsupported,
            // so show/hide is always available via the menu.
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let tray_menu = Menu::with_items(app, &[&show, &hide, &sep, &quit])?;

            let is_quitting_tray = is_quitting_setup.clone();
            TrayIconBuilder::with_id("main")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "hide" => hide_main_window(app),
                    "quit" => {
                        is_quitting_tray.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    // Linux: click events are unsupported by the platform backend, so this is a
                    // best-effort convenience for Windows/macOS.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if is_quitting_window.load(Ordering::SeqCst) {
                    return;
                }
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building BranchMind Viewer (Tauri)");

    app.run(move |_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            is_quitting.store(true, Ordering::SeqCst);
        }
    });
}
