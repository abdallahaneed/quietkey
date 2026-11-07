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
use tauri::{AppHandle, Emitter, Manager, LogicalPosition, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

struct AppState {
    db: Arc<Mutex<Connection>>,
    tx: Sender<Input>,
    tap_running: AtomicBool,
    /// The onboarding microphone test: levels only, nothing is kept.
    mic_test: Mutex<Option<audio::Recording>>,
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
fn transcript_copy(state: State<AppState>, id: String, raw: Option<bool>) -> CmdResult<()> {
    let t = db::get_transcript(&state.db.lock().unwrap(), &id).map_err(err)?;
    paste::set_clipboard_text(if raw == Some(true) { &t.raw_text } else { &t.final_text });
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
fn settings_update(app: AppHandle, state: State<AppState>, patch: serde_json::Map<String, serde_json::Value>) -> CmdResult<serde_json::Value> {
    let conn = state.db.lock().unwrap();
    // Apply OS-level settings first, so the stored value never claims something that did not happen.
    if let Some(on) = patch.get("launch_at_login").and_then(|v| v.as_bool()) {
        let al = app.autolaunch();
        if on { al.enable() } else { al.disable() }.map_err(|e| format!("Could not change start at login: {e}"))?;
    }
    if let Some(show) = patch.get("show_bar").and_then(|v| v.as_bool()) {
        if let Some(bar) = app.get_webview_window("bar") {
            let _ = if show { bar.show() } else { bar.hide() };
        }
    }
    let out = store::settings_update(&conn, &patch)?;
    if patch.contains_key("shortcuts") {
        hotkey::set_shortcut(store::push_to_talk(&out["shortcuts"])?);
    }
    let _ = app.emit("settings://changed", &out);
    // Switching to a stricter retention mode applies right away, not at the next hourly run.
    if patch.contains_key("storage_mode") {
        let _ = store::retention_purge(&conn, out["storage_mode"].as_str().unwrap_or("store"), db::now_ms());
    }
    Ok(out)
}

#[tauri::command]
fn mics_list() -> Vec<audio::Mic> {
    audio::input_devices()
}

/// Starts the microphone test. Levels arrive on "mic_test://level"; the audio is dropped on stop.
#[tauri::command]
fn mic_test_start(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let mut slot = state.mic_test.lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    let mic = store::mic_choice(&state.db.lock().unwrap());
    let rec = audio::start(mic, move |rms| {
        let _ = app.emit("mic_test://level", rms);
    })
    .map_err(|e| match e.to_string().as_str() {
        "no_mic" => "No microphone found".to_string(),
        other => format!("Microphone error: {other}"),
    })?;
    *slot = Some(rec);
    Ok(())
}

#[tauri::command]
fn mic_test_stop(state: State<AppState>) {
    if let Some(rec) = state.mic_test.lock().unwrap().take() {
        let _ = rec.stop();
    }
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
fn dictation_undo(state: State<AppState>) {
    let _ = state.tx.send(Input::Undo);
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
    microphone: &'static str,
    other_dictation_apps: Vec<String>,
}

#[tauri::command]
fn permissions_status(state: State<AppState>) -> Permissions {
    Permissions {
        accessibility: paste::accessibility_trusted(),
        input_monitoring: hotkey::input_monitoring_granted(),
        hotkey_running: state.tap_running.load(Ordering::SeqCst),
        fn_key_conflict: hotkey::fn_key_conflict(),
        microphone: audio::mic_permission(),
        other_dictation_apps: hotkey::other_dictation_apps(paste::running_app_names()),
    }
}

/// While the shortcut editor records keys, pressing them must not start a dictation.
#[tauri::command]
fn hotkey_pause(on: bool) {
    hotkey::set_paused(on);
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

/// Click-through panel, placed on the screen in use and shown.
fn place_bar(app: &AppHandle) {
    let Some(bar) = app.get_webview_window("bar") else { return };
    // Click-through until the bar shows buttons; Bar.tsx turns clicks on and off per state.
    let _ = bar.set_ignore_cursor_events(true);
    #[cfg(target_os = "macos")]
    make_panel(&bar);
    position_bar(app);
    let _ = bar.show();
}

/// Bottom centre of the screen under the mouse pointer, a little above the Dock. Called at every dictation start,
/// so with two displays the bar is where the user is working, and follows displays being plugged in or out.
pub(crate) fn position_bar(app: &AppHandle) {
    let Some(bar) = app.get_webview_window("bar") else { return };
    // Compare in points: Tauri's cursor position is in pixels of one scale, so with a Retina and a 1x display side by
    // side it matched the wrong screen (measured: pointer on the laptop put the bar on the external display).
    let at = core_graphics::event_source::CGEventSource::new(core_graphics::event_source::CGEventSourceStateID::HIDSystemState)
        .and_then(core_graphics::event::CGEvent::new)
        .map(|e| e.location())
        .ok();
    let mon = at
        .and_then(|p| {
            app.available_monitors().ok()?.into_iter().find(|m| {
                let s = m.scale_factor();
                let (x, y) = (m.position().x as f64 / s, m.position().y as f64 / s);
                let (w, h) = (m.size().width as f64 / s, m.size().height as f64 / s);
                p.x >= x && p.x < x + w && p.y >= y && p.y < y + h
            })
        })
        .or_else(|| bar.primary_monitor().ok().flatten());
    let (Some(mon), Ok(size)) = (mon, bar.outer_size()) else { return };
    // Logical points: physical pixels differ per screen when a Retina and a normal display are mixed.
    let s = mon.scale_factor();
    let (ox, oy) = (mon.position().x as f64 / s, mon.position().y as f64 / s);
    let (mw, mh) = (mon.size().width as f64 / s, mon.size().height as f64 / s);
    let bs = bar.scale_factor().unwrap_or(s);
    let (bw, bh) = (size.width as f64 / bs, size.height as f64 / bs);
    let _ = bar.set_position(LogicalPosition::new(ox + (mw - bw) / 2.0, oy + mh - bh - 88.0));
}

/// Turns the bar into a non-activating panel: clicking Stop or Undo does not take focus from the app being
/// typed into, so the paste still lands there. Same approach as Spotlight-style panels.
#[cfg(target_os = "macos")]
fn make_panel(bar: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    let Ok(ptr) = bar.ns_window() else { return };
    let win = ptr as *mut AnyObject;
    // The panel must never become key either: a key panel takes keystrokes (like Spotlight), so the paste's ⌘V
    // would land in the bar instead of the app. Clicks still arrive because the webview accepts the first mouse.
    let panel = AnyClass::get(c"QuietkeyBarPanel").or_else(|| {
        use objc2::runtime::{Bool, ClassBuilder, Sel};
        extern "C-unwind" fn no(_: &AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        let mut b = ClassBuilder::new(c"QuietkeyBarPanel", AnyClass::get(c"NSPanel")?)?;
        unsafe {
            b.add_method(objc2::sel!(canBecomeKeyWindow), no as extern "C-unwind" fn(_, _) -> _);
            b.add_method(objc2::sel!(canBecomeMainWindow), no as extern "C-unwind" fn(_, _) -> _);
        }
        Some(b.register())
    });
    let Some(panel) = panel else { return };
    unsafe {
        objc2::ffi::object_setClass(win, panel);
        let mask: usize = msg_send![win, styleMask];
        let _: () = msg_send![win, setStyleMask: mask | (1 << 7)]; // NSWindowStyleMaskNonactivatingPanel
        let _: () = msg_send![win, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![win, setFloatingPanel: true];
        #[cfg(debug_assertions)]
        {
            let cls: &AnyClass = msg_send![win, class];
            let m: usize = msg_send![win, styleMask];
            eprintln!("[panel] class={} styleMask={m:#x}", cls.name().to_string_lossy());
        }
    }
}

/// Loads and warms up a model, then makes it the one dictation uses. The old one is freed after the swap.
fn load_model(path: &std::path::Path) -> anyhow::Result<()> {
    let mut s = stt::Stt::load(path)?;
    let _ = s.transcribe(&vec![0.0; 16_000], Some("en"), "");
    *pipeline::STT.lock().unwrap() = Some(s); // waits for a transcription in progress
    pipeline::READY.store(true, Ordering::SeqCst);
    Ok(())
}

/// The model picker: each model with its download size, RAM, and whether it is on disk or in use.
#[tauri::command]
fn models_list(app: AppHandle, state: State<AppState>) -> CmdResult<Vec<serde_json::Value>> {
    let dir = app.path().app_data_dir().map_err(err)?.join("models");
    let active = stt::model(store::model_choice(&state.db.lock().unwrap()).as_deref()).id;
    Ok(stt::MODELS
        .iter()
        .map(|m| serde_json::json!({ "model": m, "downloaded": dir.join(m.file).exists(), "active": m.id == active }))
        .collect())
}

static SWITCHING: AtomicBool = AtomicBool::new(false);

/// Downloads (if needed) and loads model `id` in the background, then saves it as the choice. Progress arrives on
/// "model://progress" {id, done, total}; the end on "model://ready" {id} or "model://error" {id, message}.
#[tauri::command]
fn model_select(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    let m = *stt::MODELS.iter().find(|m| m.id == id).ok_or("Unknown model")?;
    if SWITCHING.swap(true, Ordering::SeqCst) {
        return Err("Already switching models, wait for it to finish".into());
    }
    let dir = app.path().app_data_dir().map_err(err)?.join("models");
    let db = state.db.clone();
    std::thread::spawn(move || {
        let res = stt::download(&m, &dir, |done, total| {
            let _ = app.emit("model://progress", serde_json::json!({ "id": m.id, "done": done, "total": total }));
        })
        .and_then(|path| load_model(&path))
        .and_then(|()| store::set_model(&db.lock().unwrap(), m.id).map_err(Into::into));
        SWITCHING.store(false, Ordering::SeqCst);
        let _ = match res {
            Ok(()) => app.emit("model://ready", serde_json::json!({ "id": m.id })),
            Err(e) => app.emit("model://error", serde_json::json!({ "id": m.id, "message": e.to_string() })),
        };
    });
    Ok(())
}

fn show_hub(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// The app was called Utter before 2026-11-07 (bundle id dev.abdallah.utter). Moves that data folder (history,
/// settings, the downloaded model) to the new one once, so nothing is lost and the model is not downloaded again.
fn migrate_old_data(new: &std::path::Path) {
    let Some(old) = new.parent().map(|p| p.join("dev.abdallah.utter")) else { return };
    if old.is_dir() && !new.exists() {
        if let Err(e) = std::fs::rename(&old, new) {
            eprintln!("could not move {} to {}: {e}", old.display(), new.display());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::Builder::new().macos_launcher(MacosLauncher::LaunchAgent).build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar app, no Dock icon

            let data = app.path().app_data_dir()?;
            migrate_old_data(&data);
            std::fs::create_dir_all(data.join("models"))?;
            let conn = db::open(&data.join("app.db"))?;
            let db = Arc::new(Mutex::new(conn));
            let jobs_db = db.clone();

            let (tx, rx) = mpsc::channel::<Input>();
            app.manage(AppState { db: db.clone(), tx, tap_running: AtomicBool::new(false), mic_test: Mutex::new(None) });

            // Model load + warm-up off the main thread; the first transcription is ~40% slower without it.
            let handle = app.handle().clone();
            let choice = store::model_choice(&db.lock().unwrap());
            let model = data.join("models").join(stt::model(choice.as_deref()).file);
            std::thread::Builder::new().name("pipeline".into()).spawn(move || {
                if let Err(e) = load_model(&model) {
                    let _ = handle.emit("dictation://state", pipeline::StatePayload { state: "error", message: Some(format!("{e}")) });
                }
                Pipeline { app: handle, db }.run(rx);
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
            if let Some(mask) = store::settings_get(&state.db.lock().unwrap()).ok().and_then(|s| store::push_to_talk(&s["shortcuts"]).ok()) {
                hotkey::set_shortcut(mask);
            }
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
            let settings = store::settings_get(&app.state::<AppState>().db.lock().unwrap()).unwrap_or_default();
            if settings["show_bar"] == false {
                if let Some(bar) = app.get_webview_window("bar") {
                    let _ = bar.hide();
                }
            }
            // Keep the login item in line with the setting. Release builds only: a dev build would register
            // the debug binary as a login item.
            #[cfg(not(debug_assertions))]
            {
                let al = app.autolaunch();
                let want = settings["launch_at_login"] != false;
                if al.is_enabled().unwrap_or(false) != want {
                    let _ = if want { al.enable() } else { al.disable() };
                }
            }
            // A menu bar app must not grab focus at login; only open the Hub while setup is unfinished.
            let setup_done = store::settings_get(&app.state::<AppState>().db.lock().unwrap())
                .map(|s| s["onboarding_done"] == true)
                .unwrap_or(false);
            if !setup_done && std::env::var("QUIETKEY_TEST_WAV").is_err() {
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
            models_list,
            model_select,
            transcripts_list,
            transcript_copy,
            copy_last,
            dictation_cancel,
            dictation_toggle,
            dictation_undo,
            permissions_status,
            request_input_monitoring,
            hotkey_start,
            hotkey_pause,
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
            llm_test,
            mics_list,
            mic_test_start,
            mic_test_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod migrate_tests {
    #[test]
    fn moves_old_folder_once_and_never_overwrites() {
        let root = std::env::temp_dir().join(format!("qk-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("dev.abdallah.utter"), root.join("dev.abdallah.quietkey"));
        std::fs::create_dir_all(old.join("models")).unwrap();
        std::fs::write(old.join("app.db"), "history").unwrap();
        super::migrate_old_data(&new);
        assert_eq!(std::fs::read_to_string(new.join("app.db")).unwrap(), "history");
        assert!(!old.exists());
        // a later old folder never replaces existing data
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("app.db"), "stale").unwrap();
        super::migrate_old_data(&new);
        assert_eq!(std::fs::read_to_string(new.join("app.db")).unwrap(), "history");
        let _ = std::fs::remove_dir_all(&root);
    }
}
