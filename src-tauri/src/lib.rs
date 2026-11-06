mod audio;
mod cleanup;
mod db;
mod hotkey;
mod llm;
mod paste;
mod pipeline;
mod store;
mod stt;

use pipeline::{Input, Pipeline};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WindowEvent};

struct AppState {
    db: Arc<Mutex<Connection>>,
    tx: Sender<Input>,
    tap_running: AtomicBool,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn transcripts_list(state: State<AppState>, before: Option<i64>, limit: Option<u32>) -> CmdResult<Vec<db::Transcript>> {
    db::list_transcripts(&state.db.lock().unwrap(), before, limit.unwrap_or(50)).map_err(err)
}

#[tauri::command]
fn transcript_copy(state: State<AppState>, id: String) -> CmdResult<()> {
    let t = db::get_transcript(&state.db.lock().unwrap(), &id).map_err(err)?;
    paste::set_clipboard_text(&t.final_text);
    Ok(())
}

#[tauri::command]
fn copy_last(state: State<AppState>) -> CmdResult<bool> {
    match db::last_text(&state.db.lock().unwrap()).map_err(err)? {
        Some(text) => {
            paste::set_clipboard_text(&text);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
fn transcript_delete(state: State<AppState>, id: String) -> CmdResult<usize> {
    store::delete_many(&state.db.lock().unwrap(), "transcripts", &[id]).map_err(err)
}

#[tauri::command]
fn data_delete(state: State<AppState>, categories: Vec<String>, confirm: String) -> CmdResult<serde_json::Value> {
    store::data_delete(&state.db.lock().unwrap(), &categories, &confirm)
}

#[tauri::command]
fn settings_get(state: State<AppState>) -> CmdResult<serde_json::Value> {
    store::settings_get(&state.db.lock().unwrap()).map_err(err)
}

#[tauri::command]
fn settings_update(state: State<AppState>, patch: serde_json::Map<String, serde_json::Value>) -> CmdResult<serde_json::Value> {
    let conn = state.db.lock().unwrap();
    let out = store::settings_update(&conn, &patch)?;
    // Switching to a stricter retention mode applies right away, not at the next hourly run.
    if patch.contains_key("storage_mode") {
        let _ = store::retention_purge(&conn, out["storage_mode"].as_str().unwrap_or("store"), db::now_ms());
    }
    Ok(out)
}

#[tauri::command]
fn dictionary_list(state: State<AppState>) -> CmdResult<Vec<store::DictEntry>> {
    store::dict_list(&state.db.lock().unwrap()).map_err(err)
}

#[tauri::command]
fn dictionary_add(state: State<AppState>, word: String, misspelling: Option<String>) -> CmdResult<String> {
    store::dict_add(&state.db.lock().unwrap(), &word, misspelling.as_deref(), "manual")
}

#[tauri::command]
fn dictionary_update(state: State<AppState>, id: String, word: String, misspelling: Option<String>) -> CmdResult<()> {
    store::dict_update(&state.db.lock().unwrap(), &id, &word, misspelling.as_deref())
}

#[tauri::command]
fn dictionary_delete(state: State<AppState>, ids: Vec<String>) -> CmdResult<usize> {
    store::delete_many(&state.db.lock().unwrap(), "dictionary_entries", &ids).map_err(err)
}

/// The UI reads the file and sends its text, so the backend never opens arbitrary paths.
#[tauri::command]
fn dictionary_import(state: State<AppState>, content: String) -> CmdResult<store::ImportResult> {
    store::dict_import(&state.db.lock().unwrap(), &content)
}

#[tauri::command]
fn snippets_list(state: State<AppState>) -> CmdResult<Vec<store::SnippetRow>> {
    store::snip_list(&state.db.lock().unwrap()).map_err(err)
}

#[tauri::command]
fn snippets_add(state: State<AppState>, trigger: String, expansion: String) -> CmdResult<String> {
    store::snip_add(&state.db.lock().unwrap(), &trigger, &expansion)
}

#[tauri::command]
fn snippets_update(state: State<AppState>, id: String, trigger: String, expansion: String) -> CmdResult<()> {
    store::snip_update(&state.db.lock().unwrap(), &id, &trigger, &expansion)
}

#[tauri::command]
fn snippets_delete(state: State<AppState>, ids: Vec<String>) -> CmdResult<usize> {
    store::delete_many(&state.db.lock().unwrap(), "snippets", &ids).map_err(err)
}

#[tauri::command]
fn styles_get(state: State<AppState>) -> CmdResult<Vec<store::StyleRow>> {
    store::styles_get(&state.db.lock().unwrap()).map_err(err)
}

#[tauri::command]
fn style_set(state: State<AppState>, category: String, tone: String) -> CmdResult<()> {
    store::style_set(&state.db.lock().unwrap(), &category, &tone)
}

#[derive(Serialize)]
struct KeyStatus {
    saved: bool,
}

#[tauri::command]
fn llm_key_status() -> KeyStatus {
    KeyStatus { saved: llm::key_get().is_some() }
}

#[tauri::command]
fn llm_key_set(key: String) -> CmdResult<()> {
    llm::key_set(&key).map_err(err)
}

#[tauri::command]
fn llm_key_delete() {
    llm::key_delete();
}

/// Sends one short sample through the configured provider. Runs off the main thread (network).
#[tauri::command]
async fn llm_test(state: State<'_, AppState>) -> CmdResult<String> {
    let cfg = store::pipeline_settings(&state.db.lock().unwrap()).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        let sample = "um so I think we should, uh, meet at 5 actually 6 tomorrow";
        let system = cleanup::llm_system_prompt("light", "formal", &[]);
        match cfg.llm_provider.as_str() {
            "groq" => llm::groq(&cfg.groq_model, &system, sample),
            "ollama" => llm::ollama(&cfg.ollama_url, &cfg.ollama_model, &system, sample),
            _ => Ok(cleanup::local_cleanup(sample)),
        }
        .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn dictation_cancel(state: State<AppState>) {
    let _ = state.tx.send(Input::Cancel);
}

#[tauri::command]
fn dictation_toggle(state: State<AppState>) {
    let _ = state.tx.send(Input::Toggle);
}

#[derive(Serialize)]
struct Permissions {
    accessibility: bool,
    input_monitoring: bool,
    hotkey_running: bool,
    fn_key_conflict: bool,
}

#[tauri::command]
fn permissions_status(state: State<AppState>) -> Permissions {
    Permissions {
        accessibility: paste::accessibility_trusted(),
        input_monitoring: hotkey::input_monitoring_granted(),
        hotkey_running: state.tap_running.load(Ordering::SeqCst),
        fn_key_conflict: hotkey::fn_key_conflict(),
    }
}

#[tauri::command]
fn request_input_monitoring() -> bool {
    hotkey::request_input_monitoring()
}

/// Starts the Fn listener if it is not running yet. Called at launch and again after the user grants permission.
#[tauri::command]
fn hotkey_start(state: State<AppState>) -> CmdResult<()> {
    start_hotkey(&state)
}

fn start_hotkey(state: &AppState) -> CmdResult<()> {
    if state.tap_running.load(Ordering::SeqCst) {
        return Ok(());
    }
    let tx = state.tx.clone();
    hotkey::spawn(move |ev| {
        let _ = tx.send(Input::Key(ev));
    })
    .map_err(err)?;
    state.tap_running.store(true, Ordering::SeqCst);
    Ok(())
}

/// Bottom centre of the primary screen, a little above the Dock.
fn place_bar(app: &AppHandle) {
    let Some(bar) = app.get_webview_window("bar") else { return };
    let _ = bar.set_ignore_cursor_events(true); // TODO: switch to a non-activating NSPanel; click-through for now so it never steals focus
    if let (Ok(Some(mon)), Ok(size)) = (bar.primary_monitor(), bar.outer_size()) {
        let m = mon.size();
        let x = (m.width as i32 - size.width as i32) / 2;
        let y = m.height as i32 - size.height as i32 - (88.0 * mon.scale_factor()) as i32;
        let _ = bar.set_position(PhysicalPosition::new(x, y));
    }
    let _ = bar.show();
}

fn show_hub(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar app, no Dock icon

            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(data.join("models"))?;
            let conn = db::open(&data.join("app.db"))?;
            let db = Arc::new(Mutex::new(conn));
            let jobs_db = db.clone();

            let (tx, rx) = mpsc::channel::<Input>();
            app.manage(AppState { db: db.clone(), tx, tap_running: AtomicBool::new(false) });

            // Model load + warm-up off the main thread; the first transcription is ~40% slower without it.
            let handle = app.handle().clone();
            let model = data.join("models").join(stt::MODEL_FILE);
            std::thread::Builder::new().name("pipeline".into()).spawn(move || {
                let stt = match stt::Stt::load(&model) {
                    Ok(mut s) => {
                        let _ = s.transcribe(&vec![0.0; 16_000], Some("en"), "");
                        Some(s)
                    }
                    Err(e) => {
                        let _ = handle.emit("dictation://state", pipeline::StatePayload { state: "error", message: Some(format!("{e}")) });
                        None
                    }
                };
                Pipeline { app: handle, db, stt }.run(rx);
            })?;

            // Retention job: at startup and every hour.
            std::thread::Builder::new().name("retention".into()).spawn(move || loop {
                if let Ok(conn) = jobs_db.lock() {
                    let mode = store::settings_get(&conn).ok().and_then(|s| s["storage_mode"].as_str().map(String::from));
                    if let Err(e) = store::retention_purge(&conn, mode.as_deref().unwrap_or("store"), db::now_ms()) {
                        eprintln!("retention purge failed: {e}");
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(3600));
            })?;

            let state = app.state::<AppState>();
            if let Err(e) = start_hotkey(&state) {
                eprintln!("hotkey not started: {e} (Input Monitoring / Accessibility missing)");
            }

            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().ok_or("no icon")?)
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_hub(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            place_bar(app.handle());
            // A menu bar app must not grab focus at login; only open the Hub while setup is unfinished.
            let setup_done = store::settings_get(&app.state::<AppState>().db.lock().unwrap())
                .map(|s| s["onboarding_done"] == true)
                .unwrap_or(false);
            if !setup_done && std::env::var("UTTER_TEST_WAV").is_err() {
                show_hub(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the Hub hides it; the app keeps running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            transcripts_list,
            transcript_copy,
            copy_last,
            dictation_cancel,
            dictation_toggle,
            permissions_status,
            request_input_monitoring,
            hotkey_start,
            transcript_delete,
            data_delete,
            settings_get,
            settings_update,
            dictionary_list,
            dictionary_add,
            dictionary_update,
            dictionary_delete,
            dictionary_import,
            snippets_list,
            snippets_add,
            snippets_update,
            snippets_delete,
            styles_get,
            style_set,
            llm_key_status,
            llm_key_set,
            llm_key_delete,
            llm_test
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
