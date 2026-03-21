//! Ember Desktop Application
//!
//! A Tauri-based desktop application for the Ember AI agent.
//! Features:
//! - System tray with quick actions
//! - Auto-update mechanism
//! - Native notifications
//! - Global keyboard shortcuts
//! - Autostart on login

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    App, AppHandle, Emitter, Manager, State,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

/// Application state
pub struct AppState {
    /// Current model being used
    pub current_model: Mutex<String>,
    /// Server URL
    pub server_url: Mutex<String>,
    /// Whether the server is connected
    pub connected: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_model: Mutex::new("gpt-4".to_string()),
            server_url: Mutex::new("http://localhost:3000".to_string()),
            connected: Mutex::new(false),
        }
    }
}

/// Chat request from frontend
#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub model: Option<String>,
    pub stream: Option<bool>,
}

/// Chat response to frontend
#[derive(Debug, Clone, Serialize)]
pub struct ChatResponse {
    pub message: String,
    pub model: String,
    pub tokens_used: Option<u32>,
}

/// Update status
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

/// Send a chat message to the LLM
#[tauri::command]
async fn chat(request: ChatRequest, state: State<'_, AppState>) -> Result<ChatResponse, String> {
    let model = request
        .model
        .unwrap_or_else(|| state.current_model.lock().unwrap().clone());

    info!("Chat request with model: {}", model);

    // In a full implementation, this would use ember-llm
    // For now, return a placeholder
    Ok(ChatResponse {
        message: format!("Echo: {}", request.message),
        model,
        tokens_used: Some(10),
    })
}

/// Get server info
#[tauri::command]
fn get_info(state: State<'_, AppState>) -> serde_json::Value {
    let connected = *state.connected.lock().unwrap();
    let model = state.current_model.lock().unwrap().clone();
    let server_url = state.server_url.lock().unwrap().clone();

    serde_json::json!({
        "name": "Ember AI Desktop",
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "connected": connected,
        "currentModel": model,
        "serverUrl": server_url,
    })
}

/// Get available models
#[tauri::command]
fn get_models() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "id": "gpt-4", "name": "GPT-4", "provider": "OpenAI" }),
        serde_json::json!({ "id": "gpt-4-turbo", "name": "GPT-4 Turbo", "provider": "OpenAI" }),
        serde_json::json!({ "id": "gpt-3.5-turbo", "name": "GPT-3.5 Turbo", "provider": "OpenAI" }),
        serde_json::json!({ "id": "claude-3-opus", "name": "Claude 3 Opus", "provider": "Anthropic" }),
        serde_json::json!({ "id": "claude-3-sonnet", "name": "Claude 3 Sonnet", "provider": "Anthropic" }),
        serde_json::json!({ "id": "gemini-pro", "name": "Gemini Pro", "provider": "Google" }),
        serde_json::json!({ "id": "llama-3-70b", "name": "Llama 3 70B", "provider": "Ollama" }),
    ]
}

/// Set the current model
#[tauri::command]
fn set_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    *state.current_model.lock().unwrap() = model.clone();
    info!("Model changed to: {}", model);
    Ok(())
}

/// Set the server URL
#[tauri::command]
fn set_server_url(url: String, state: State<'_, AppState>) -> Result<(), String> {
    *state.server_url.lock().unwrap() = url.clone();
    info!("Server URL changed to: {}", url);
    Ok(())
}

/// Check for updates
#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<UpdateStatus, String> {
    info!("Checking for updates...");

    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                info!("Update available: {}", update.version);
                Ok(UpdateStatus {
                    available: true,
                    version: Some(update.version.clone()),
                    notes: update.body.clone(),
                })
            }
            Ok(None) => {
                info!("No updates available");
                Ok(UpdateStatus {
                    available: false,
                    version: None,
                    notes: None,
                })
            }
            Err(e) => {
                warn!("Failed to check for updates: {}", e);
                Err(format!("Failed to check for updates: {}", e))
            }
        },
        Err(e) => {
            warn!("Updater not available: {}", e);
            Err(format!("Updater not available: {}", e))
        }
    }
}

/// Install available update
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    info!("Installing update...");

    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                // Download and install
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(_) => {
                        info!("Update installed successfully");
                        // Restart the app
                        app.restart();
                    }
                    Err(e) => {
                        error!("Failed to install update: {}", e);
                        Err(format!("Failed to install update: {}", e))
                    }
                }
            }
            Ok(None) => Err("No update available".to_string()),
            Err(e) => Err(format!("Failed to check for updates: {}", e)),
        },
        Err(e) => Err(format!("Updater not available: {}", e)),
    }
}

/// Send a native notification
#[tauri::command]
fn send_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
        .map_err(|e| format!("Failed to send notification: {}", e))
}

/// Get autostart status
#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Failed to get autostart status: {}", e))
}

/// Set autostart status
#[tauri::command]
fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();

    if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    }
    .map_err(|e| format!("Failed to set autostart: {}", e))
}

/// Setup system tray with menu
fn setup_tray(app: &App) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    // Create menu items
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide Window", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;

    // Model submenu
    let model_gpt4 = MenuItem::with_id(app, "model_gpt4", "GPT-4", true, None::<&str>)?;
    let model_claude = MenuItem::with_id(app, "model_claude", "Claude 3", true, None::<&str>)?;
    let model_gemini = MenuItem::with_id(app, "model_gemini", "Gemini Pro", true, None::<&str>)?;
    let model_local = MenuItem::with_id(app, "model_local", "Local (Ollama)", true, None::<&str>)?;
    let model_submenu = Submenu::with_items(
        app,
        "Switch Model",
        true,
        &[&model_gpt4, &model_claude, &model_gemini, &model_local],
    )?;

    let separator2 = PredefinedMenuItem::separator(app)?;
    let check_updates = MenuItem::with_id(
        app,
        "check_updates",
        "Check for Updates...",
        true,
        None::<&str>,
    )?;
    let preferences = MenuItem::with_id(app, "preferences", "Preferences...", true, None::<&str>)?;
    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ember", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &separator1,
            &model_submenu,
            &separator2,
            &check_updates,
            &preferences,
            &separator3,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Ember AI - Ready")
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            info!("Tray menu event: {}", id);

            match id {
                "quit" => {
                    info!("Quitting application");
                    app.exit(0);
                }
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "hide" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
                "preferences" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        let _ = window.emit("open-settings", ());
                    }
                }
                "check_updates" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(updater) = app_handle.updater() {
                            match updater.check().await {
                                Ok(Some(update)) => {
                                    let _ = app_handle
                                        .notification()
                                        .builder()
                                        .title("Update Available")
                                        .body(format!("Version {} is available", update.version))
                                        .show();
                                }
                                Ok(None) => {
                                    let _ = app_handle
                                        .notification()
                                        .builder()
                                        .title("No Updates")
                                        .body("You're running the latest version")
                                        .show();
                                }
                                Err(e) => {
                                    error!("Update check failed: {}", e);
                                }
                            }
                        }
                    });
                }
                id if id.starts_with("model_") => {
                    let model = match id {
                        "model_gpt4" => "gpt-4",
                        "model_claude" => "claude-3-sonnet",
                        "model_gemini" => "gemini-pro",
                        "model_local" => "llama-3-70b",
                        _ => return,
                    };

                    if let Some(state) = app.try_state::<AppState>() {
                        *state.current_model.lock().unwrap() = model.to_string();
                        info!("Model switched to: {}", model);

                        let _ = app
                            .notification()
                            .builder()
                            .title("Model Changed")
                            .body(format!("Now using {}", model))
                            .show();

                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("model-changed", model);
                        }
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;

            if let TrayIconEvent::Click { button, .. } = event {
                if button == tauri::tray::MouseButton::Left {
                    if let Some(window) = tray.app_handle().get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            }
        })
        .build(app)?;

    Ok(tray)
}

/// Setup global shortcuts
fn setup_shortcuts(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    // Toggle window visibility (Cmd/Ctrl + Shift + E)
    let toggle_shortcut: Shortcut = "CommandOrControl+Shift+E".parse()?;
    let app_handle = app.handle().clone();

    app.global_shortcut()
        .on_shortcut(toggle_shortcut, move |_app, _shortcut, _event| {
            if let Some(window) = app_handle.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })?;

    // Quick chat (Cmd/Ctrl + Shift + Space)
    let chat_shortcut: Shortcut = "CommandOrControl+Shift+Space".parse()?;
    let app_handle2 = app.handle().clone();

    app.global_shortcut()
        .on_shortcut(chat_shortcut, move |_app, _shortcut, _event| {
            if let Some(window) = app_handle2.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit("focus-chat-input", ());
            }
        })?;

    info!("Global shortcuts registered");
    Ok(())
}

/// Check for updates on startup
async fn check_updates_on_startup(app: AppHandle) {
    // Wait a bit before checking
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    if let Ok(updater) = app.updater() {
        match updater.check().await {
            Ok(Some(update)) => {
                info!("Update available on startup: {}", update.version);
                let _ = app
                    .notification()
                    .builder()
                    .title("Update Available")
                    .body(format!(
                        "Ember AI {} is available. Click to update.",
                        update.version
                    ))
                    .show();
            }
            Ok(None) => {
                info!("No updates available on startup");
            }
            Err(e) => {
                warn!("Failed to check for updates on startup: {}", e);
            }
        }
    }
}

fn main() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Ember AI Desktop v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            chat,
            get_info,
            get_models,
            set_model,
            set_server_url,
            check_for_updates,
            install_update,
            send_notification,
            get_autostart_enabled,
            set_autostart_enabled,
        ])
        .setup(|app| {
            info!("Setting up application...");

            // Setup system tray
            let _tray = setup_tray(app)?;
            info!("System tray initialized");

            // Setup global shortcuts
            if let Err(e) = setup_shortcuts(app) {
                warn!("Failed to setup shortcuts: {}", e);
            }

            // Check for updates in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_updates_on_startup(app_handle).await;
            });

            // Show welcome notification
            let _ = app
                .notification()
                .builder()
                .title("Ember AI Ready")
                .body("Press Cmd+Shift+E to toggle the window")
                .show();

            info!("Application setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Prevent app from closing when window is closed, minimize to tray instead
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                #[cfg(not(target_os = "macos"))]
                {
                    window.hide().unwrap();
                    api.prevent_close();
                }
                #[cfg(target_os = "macos")]
                {
                    // On macOS, hide the app when the window is closed
                    tauri::AppHandle::hide(window.app_handle()).unwrap();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
