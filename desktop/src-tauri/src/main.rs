// Vid Sync desktop app: the front end in web/ in a window, with the library
// behind web/platform-desktop.js. Every command goes through
// vidsync_core::api::App::call (the same table `vidsync-cli serve` uses for
// testing); this file adds what needs the window: the folder and file dialogs,
// the file manager, web links, the library:// protocol for videos and
// pictures (with Range requests, so the player can seek), the tray (closing the
// window leaves the app checking there), starting at sign-in (--hidden: straight
// to the tray), system notices, and one copy running at a time.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use vidsync_core::api::{App, AppPaths};
use vidsync_core::util::{mime, read_file_part};

struct AppState {
    app: Arc<App>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// One of the app's commands (see vidsync_core::api), answered as JSON.
#[tauri::command]
async fn api(cmd: String, args: Value, state: State<'_, AppState>) -> Result<Value, String> {
    let app = state.app.clone();
    tauri::async_runtime::spawn_blocking(move || app.call(&cmd, args)).await.map_err(err)?
}

/// Pick a folder (the library). None if cancelled.
#[tauri::command]
async fn pick_folder(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    let picked = app.dialog().file().set_title(title.unwrap_or_else(|| "Choose a folder".into())).blocking_pick_folder();
    match picked {
        Some(p) => Ok(Some(p.into_path().map_err(err)?.display().to_string())),
        None => Ok(None),
    }
}

/// Pick a file (yt-dlp, ffmpeg, a cookies file). None if cancelled.
#[tauri::command]
async fn pick_file(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    let picked = app.dialog().file().set_title(title.unwrap_or_else(|| "Choose a file".into())).blocking_pick_file();
    match picked {
        Some(p) => Ok(Some(p.into_path().map_err(err)?.display().to_string())),
        None => Ok(None),
    }
}

/// Show a file in the file manager.
#[tauri::command]
fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(PathBuf::from(path)).map_err(err)
}

/// Open a folder in the file manager, or a video in the system's player.
#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(err)
}

/// Open a web link (a video's page on YouTube) in the default browser.
#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only web links open in the browser.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

/// Whether Vid Sync starts (in the tray) when the user signs in.
#[tauri::command]
fn autostart_get(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(err)
}

#[tauri::command]
fn autostart_set(app: AppHandle, on: bool) -> Result<bool, String> {
    let a = app.autolaunch();
    if on { a.enable() } else { a.disable() }.map_err(err)?;
    a.is_enabled().map_err(err)
}

/// Bring the window back (from the tray, or when Vid Sync is started again).
fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// The tray icon: what's happening (or the next check), Open, Check all, Quit.
/// The line at the top is refreshed every few seconds.
fn tray(app: &tauri::App, core: Arc<App>) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", core.status_line(), false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Vid Sync", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "Check all now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vid Sync", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &PredefinedMenuItem::separator(app)?, &open, &check, &PredefinedMenuItem::separator(app)?, &quit])?;
    let checker = core.clone();
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("Vid Sync")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id.as_ref() {
            "open" => show(app),
            "check" => {
                let c = checker.clone();
                std::thread::spawn(move || {
                    let _ = c.call("check_all", json!({}));
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show(t.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    let icon = b.build(app)?;
    std::thread::Builder::new().name("tray".into()).spawn(move || {
        let mut last = String::new();
        loop {
            let line = core.status_line();
            if line != last {
                let _ = status.set_text(&line);
                let _ = icon.set_tooltip(Some(format!("Vid Sync: {line}")));
                last = line;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    })?;
    Ok(())
}

/// The app's own addresses. Anything else a link leads to opens in the browser
/// instead of replacing the app.
fn in_app(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "library" | "asset" | "ipc" | "about" | "blob" | "data" => true,
        "http" | "https" => url.host_str().is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h.ends_with(".localhost")),
        _ => false,
    }
}

fn open_outside(app: &AppHandle, url: &tauri::Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        let _ = app.opener().open_url(url.to_string(), None::<&str>);
    }
}

fn main() {
    // started at sign-in: straight to the tray, no window
    let hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        // a second start shows the window of the copy that's running (one schedule, one library)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // library://localhost/<path> (http://library.localhost/<path> on Windows): files in the open library
        .register_asynchronous_uri_scheme_protocol("library", |ctx, request, responder| {
            let app = ctx.app_handle().state::<AppState>().app.clone();
            let path = request.uri().path().to_string();
            let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(String::from);
            tauri::async_runtime::spawn_blocking(move || {
                let response = match app.library_path(&path).and_then(|p| Ok(read_file_part(&p, range.as_deref())?)) {
                    Ok(part) => {
                        let mut b = tauri::http::Response::builder()
                            .status(part.status)
                            .header("Content-Type", mime(&path))
                            .header("Accept-Ranges", "bytes")
                            // a picture is shown as a picture, never run as a page
                            .header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; img-src data:; media-src 'self'")
                            .header("Access-Control-Allow-Origin", "*");
                        if let Some(cr) = part.content_range {
                            b = b.header("Content-Range", cr);
                        }
                        b.body(part.body)
                    }
                    Err(e) => tauri::http::Response::builder().status(404).body(e.to_string().into_bytes()),
                };
                responder.respond(response.unwrap_or_else(|_| tauri::http::Response::new(Vec::new())));
            });
        })
        .setup(move |app| {
            let paths = AppPaths {
                config_dir: app.path().app_config_dir()?,
                library_url: if cfg!(windows) { "http://library.localhost/".into() } else { "library://localhost/".into() },
            };
            let core = App::new(paths, std::time::Duration::from_secs(30))?;
            let notify = app.handle().clone();
            core.set_notifier(Box::new(move |title, text| {
                let _ = notify.notification().builder().title(title).body(text).show();
            }));
            app.manage(AppState { app: core.clone() });
            if let Err(e) = tray(app, core) {
                eprintln!("no tray icon: {e}");
            }
            // the window (tauri.conf.json, "create": false), made here so a link in a
            // description can't take it over: the web opens in the browser
            let cfg = app.config().app.windows.iter().find(|w| w.label == "main").cloned().ok_or("tauri.conf.json has no main window")?;
            let (to_browser, to_browser2) = (app.handle().clone(), app.handle().clone());
            tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .visible(!hidden)
                .on_navigation(move |url| {
                    if in_app(url) {
                        return true;
                    }
                    open_outside(&to_browser, url);
                    false
                })
                .on_new_window(move |url, _features| {
                    if !in_app(&url) {
                        open_outside(&to_browser2, &url);
                    }
                    tauri::webview::NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        // closing the window leaves Vid Sync in the tray, checking, unless Settings say quit
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let core = window.state::<AppState>().app.clone();
                if !core.config().quit_on_close {
                    api.prevent_close();
                    let _ = window.hide();
                    static TOLD: AtomicBool = AtomicBool::new(false);
                    if !TOLD.swap(true, Ordering::Relaxed) && !core.config().notices_off {
                        let _ = window.app_handle().notification().builder().title("Vid Sync is still running").body("It keeps checking for new videos from the tray. Settings can make closing the window quit.").show();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![api, pick_folder, pick_file, reveal, open_path, open_url, autostart_get, autostart_set])
        .run(tauri::generate_context!())
        .expect("error while running Vid Sync");
}
