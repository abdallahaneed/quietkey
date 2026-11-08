//! Dictation in two threads:
//! - the state machine turns hotkey events into recordings and never blocks, so a new fn press always records;
//! - the worker transcribes, cleans up, pastes and saves finished recordings one at a time, in order.

use crate::audio::{self, Recording};
use crate::db::{self, NewTranscript};
use crate::hotkey::HotkeyEvent;
use crate::paste::{self, PasteResult};
use crate::stt::{self as sttmod, LangId, Stt, Vad};
use crate::{cleanup, llm, store};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const MIN_HOLD: Duration = Duration::from_millis(500); // shorter taps are accidental presses, ignore them
const DOUBLE_PRESS: Duration = Duration::from_millis(400); // a second tap this soon after a short one locks hands-free
const UNDO_FOR: Duration = Duration::from_secs(6); // a cancelled recording can be brought back this long
const WARN_AT: Duration = Duration::from_secs(19 * 60);
const STOP_AT: Duration = Duration::from_secs(20 * 60);

/// Commands from the UI or the hotkey thread, merged into one channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Key(HotkeyEvent),
    Cancel,
    Toggle,
    /// Transcribe the recording that was just cancelled after all.
    Undo,
}

#[derive(Serialize, Clone)]
pub struct StatePayload {
    pub state: &'static str, // idle | recording | hands_free | processing | success | error | warning | cancelled
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Hold,
    Locked,
}

struct Active {
    rec: Recording,
    /// The app in front when recording started, in case Quietkey itself is in front at the end.
    front: paste::FrontApp,
    started: Instant,
    mode: Mode,
    warned: bool,
}

/// A finished recording waiting for the worker.
struct Job {
    samples: Vec<f32>,
    duration_ms: i64,
    front: paste::FrontApp,
    /// Stopped from a bar click: give focus back to `front` before pasting.
    refocus: bool,
}

/// The loaded model. Settings swaps it when the user picks another one; the worker holds the lock while it transcribes.
pub static STT: Mutex<Option<Stt>> = Mutex::new(None);
/// A second model for languages the main one cannot do (Arabic with Parakeet), loaded on first need and kept.
pub static ALT: Mutex<Option<Stt>> = Mutex::new(None);
/// Language identification among the user's languages; None until downloaded.
pub static LID: Mutex<Option<LangId>> = Mutex::new(None);
/// The speech models folder, set at startup.
pub static MODELS_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Speech to text on this Mac, limited to the user's languages. Parakeet picks its language itself from 25 European
/// ones (Russian included) and has no Arabic: German with Arabic names can come out in Cyrillic, Arabic comes out
/// romanised. Language detection alone misjudges short German sentences full of Arabic names as Arabic, so:
/// Parakeet first; its text stands when it is in Latin letters and reads like English or German, or when detection
/// is not sure it was Arabic. Sure Arabic goes to an Arabic model; Cyrillic goes to a model told the language.
/// Returns the text, the language, and a notice when no model on this Mac can do the language.
fn transcribe_local(samples: &[f32], cfg: &store::PipelineSettings, prompt: &str) -> anyhow::Result<(String, Option<String>, Option<String>)> {
    let allowed: Vec<String> = if cfg.candidates.is_empty() { cfg.language.iter().cloned().collect() } else { cfg.candidates.clone() };
    let mut loaded = STT.lock().unwrap();
    let stt = loaded.as_mut().ok_or_else(|| anyhow::anyhow!("no speech model yet, open Quietkey to download one"))?;
    let old_way = |stt: &mut Stt| -> anyhow::Result<(String, Option<String>, Option<String>)> {
        let (t, l) = if cfg.candidates.is_empty() { (stt.transcribe(samples, cfg.language.as_deref(), prompt)?, cfg.language.clone()) } else { stt.transcribe_among(samples, &cfg.candidates, prompt)? };
        Ok((t, l, None))
    };
    // Whisper takes a language and retries among the user's languages itself; auto-detect means no limit.
    let Some(main) = stt.model.filter(|m| !m.langs.is_empty() && !allowed.is_empty()) else { return old_way(stt) };
    let ranked = if allowed.len() == 1 { vec![(allowed[0].clone(), 1.0)] } else { LID.lock().unwrap().as_mut().map(|l| l.rank(samples, &allowed)).unwrap_or_default() };
    #[cfg(any(test, debug_assertions))]
    eprintln!("[lang] {ranked:?} main {}", main.id);
    let Some(top) = ranked.first().cloned() else { return old_way(stt) }; // detection not ready, or clip too short

    let (lang, main_text) = if main.id == "cohere-ar" {
        // Cohere must be told the language: it keeps Arabic (with English mixed in) unless detection says German.
        if sttmod::accepts(main, &top.0) || top.0 != "de" {
            let (t, l) = stt.transcribe_among(samples, &allowed, prompt)?;
            return Ok((t, l, None));
        }
        (top.0, None)
    } else {
        let text = stt.transcribe(samples, None, prompt)?;
        let best_mine = ranked.iter().find(|r| sttmod::accepts(main, &r.0)).map(|r| r.0.clone());
        let latin = sttmod::script_fits(&text, "en");
        let sure_arabic = top.0 == "ar" && top.1 >= 0.9;
        if latin && (!sure_arabic || sttmod::common_word_share(&text) >= 0.2) {
            return Ok((text, best_mine, None));
        }
        match (latin, best_mine) {
            (true, _) => ("ar".to_string(), Some(text)),
            // Cyrillic or other letters for a language Parakeet does: Parakeet guessed a wrong language.
            (false, Some(l)) => (l, Some(text)),
            (false, None) => return Ok((text, None, None)),
        }
    };

    let dir = MODELS_DIR.get().ok_or_else(|| anyhow::anyhow!("models folder not set"))?;
    let Some(m) = sttmod::fallback_for(&lang, main.id, dir) else {
        let text = match main_text { Some(t) => t, None => stt.transcribe(samples, None, prompt)? };
        let note = (lang == "ar").then(|| "For Arabic, download Cohere Transcribe Arabic on the Models page".to_string());
        return Ok((text, Some(lang), note));
    };
    drop(loaded); // the main model is free for the next dictation while the other one runs
    let mut alt = ALT.lock().unwrap();
    if alt.as_ref().and_then(|a| a.model).map(|a| a.id) != Some(m.id) {
        *alt = None; // free the old one before loading the next
        *alt = Some(Stt::load(&dir.join(m.file))?);
    }
    let text = alt.as_mut().unwrap().transcribe(samples, Some(&lang), prompt)?;
    Ok((text, Some(lang), None))
}

/// Silero VAD, loaded at startup. None if it failed to load: the loudness check in audio.rs decides then.
pub static VAD: Mutex<Option<Vad>> = Mutex::new(None);

/// The part of the clip with speech, or None for silence.
fn speech(samples: &[f32]) -> Option<std::ops::Range<usize>> {
    if let Some(v) = VAD.lock().unwrap().as_mut() {
        match v.speech(samples) {
            Ok(r) => return r,
            Err(e) => eprintln!("{e}"),
        }
    }
    audio::has_speech(samples).then(|| 0..samples.len())
}
/// A model is loaded. Read on every key press, which must never wait for a transcription to release STT.
pub static READY: AtomicBool = AtomicBool::new(false);

pub struct Pipeline {
    pub app: AppHandle,
    pub db: Arc<Mutex<Connection>>,
}

fn emit(app: &AppHandle, state: &'static str, message: Option<String>) {
    let _ = app.emit("dictation://state", StatePayload { state, message });
}

impl Pipeline {
    pub fn run(self, rx: Receiver<Input>) {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let worker = Worker { app: self.app.clone(), db: self.db.clone() };
        if std::thread::Builder::new().name("dictation-worker".into()).spawn(move || worker.run(job_rx)).is_err() {
            emit(&self.app, "error", Some("Dictation stopped working. Quit and reopen Quietkey".into()));
            return;
        }
        Machine { app: self.app, db: self.db, jobs, last_tap: None, cancelled: None }.run(rx);
    }
}

struct Machine {
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    jobs: Sender<Job>,
    /// When the last too-short tap ended, for the double-press.
    last_tap: Option<Instant>,
    /// The last cancelled recording, kept briefly for Undo.
    cancelled: Option<(Job, Instant)>,
}

impl Machine {
    fn run(mut self, rx: Receiver<Input>) {
        let mut active: Option<Active> = None;
        loop {
            let input = match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(i) => Some(i),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };

            if self.cancelled.as_ref().is_some_and(|(_, at)| at.elapsed() > UNDO_FOR) {
                self.cancelled = None; // free the audio
            }
            // Session guard runs on every tick.
            if let Some(a) = active.as_mut() {
                let el = a.started.elapsed();
                if el >= STOP_AT {
                    let a = active.take().unwrap();
                    self.finish(a, false);
                    continue;
                } else if el >= WARN_AT && !a.warned {
                    a.warned = true;
                    emit(&self.app, "warning", Some("One minute left. Recording stops at 20 minutes".into()));
                }
            }

            let Some(input) = input else { continue };
            #[cfg(debug_assertions)]
            eprintln!("[input] {input:?} recording={}", active.is_some());
            active = match (active, input) {
                (None, Input::Key(HotkeyEvent::FnDown)) => {
                    // Its FnUp arrives in Locked mode and is ignored; the next press stops the recording.
                    let double = self.last_tap.take().is_some_and(|t| t.elapsed() < DOUBLE_PRESS);
                    self.start(if double { Mode::Locked } else { Mode::Hold })
                }
                (None, Input::Undo) => {
                    match self.cancelled.take() {
                        Some((job, at)) if at.elapsed() <= UNDO_FOR => {
                            emit(&self.app, "processing", None);
                            if self.jobs.send(job).is_err() {
                                emit(&self.app, "error", Some("Dictation stopped working. Quit and reopen Quietkey".into()));
                            }
                        }
                        _ => emit(&self.app, "error", Some("Too late to bring that back".into())),
                    }
                    None
                }
                (None, Input::Toggle) => self.start(Mode::Locked),
                (a, Input::Key(HotkeyEvent::PasteLast)) => {
                    self.paste_last();
                    a
                }
                (a, Input::Key(HotkeyEvent::CopyLast)) => {
                    if let Ok(Some(t)) = db::last_text(&self.db.lock().unwrap()) {
                        paste::set_clipboard_text(&t);
                    }
                    a
                }
                (None, _) => None,

                (Some(mut a), Input::Key(HotkeyEvent::FnSpace)) if a.mode == Mode::Hold => {
                    a.mode = Mode::Locked;
                    emit(&self.app, "hands_free", None);
                    Some(a)
                }
                (Some(a), Input::Key(HotkeyEvent::FnUp)) if a.mode == Mode::Hold => {
                    if a.started.elapsed() < MIN_HOLD {
                        let _ = a.rec.stop();
                        self.last_tap = Some(Instant::now());
                        emit(&self.app, "idle", None);
                    } else {
                        self.finish(a, false);
                    }
                    None
                }
                (Some(a), Input::Key(HotkeyEvent::FnCombo)) if a.mode == Mode::Hold => {
                    let _ = a.rec.stop(); // Fn+other key is a shortcut, not dictation
                    emit(&self.app, "idle", None);
                    None
                }
                // The FnUp that follows arrives with no recording active and is ignored.
                (Some(a), i @ (Input::Key(HotkeyEvent::FnDown) | Input::Toggle)) if a.mode == Mode::Locked => {
                    self.finish(a, i == Input::Toggle);
                    None
                }
                (Some(a), Input::Key(HotkeyEvent::Escape) | Input::Cancel) => {
                    let front = target(a.front);
                    let (samples, duration_ms) = a.rec.stop();
                    // Undo is always a bar click
                    self.cancelled = Some((Job { samples, duration_ms, front, refocus: true }, Instant::now()));
                    emit(&self.app, "cancelled", None);
                    None
                }
                (a, _) => a,
            };
        }
    }

    fn paste_last(&self) {
        let last = db::last_text(&self.db.lock().unwrap()).ok().flatten();
        match last {
            Some(t) => {
                if paste::paste_text(&t) != PasteResult::Pasted {
                    emit(&self.app, "error", Some("It's on your clipboard. Press ⌘V".into()));
                }
            }
            None => emit(&self.app, "error", Some("Nothing dictated yet".into())),
        }
    }

    fn start(&self, mode: Mode) -> Option<Active> {
        if !READY.load(Ordering::SeqCst) && !store::uses_groq(&self.db.lock().unwrap()) {
            emit(&self.app, "error", Some("No speech model yet. Open Quietkey to set one up".into()));
            return None;
        }
        let app = self.app.clone();
        let last = AtomicU64::new(0);
        let t0 = Instant::now();
        let on_level = move |rms: f32| {
            let now = t0.elapsed().as_millis() as u64;
            if now.saturating_sub(last.load(Ordering::Relaxed)) >= 33 {
                last.store(now, Ordering::Relaxed);
                let _ = app.emit("dictation://level", rms);
            }
        };
        let mic = store::mic_choice(&self.db.lock().unwrap());
        match audio::start(mic, on_level) {
            Ok(rec) => {
                cue(&self.db, "Tink");
                crate::position_bar(&self.app);
                let state = if mode == Mode::Hold { "recording" } else { "hands_free" };
                let note = rec.used_fallback.then(|| "Chosen microphone not found, using the default".to_string());
                emit(&self.app, state, note);
                Some(Active { rec, front: paste::frontmost_app(), started: Instant::now(), mode, warned: false })
            }
            Err(e) => {
                let msg = match e.to_string().as_str() {
                    "no_mic" => "No microphone found".to_string(),
                    "mic_timeout" => "Microphone did not respond".to_string(),
                    other => format!("Microphone error: {other}"),
                };
                emit(&self.app, "error", Some(msg));
                None
            }
        }
    }

    /// Stops the recording and hands it to the worker. Returns at once, so the next press records right away.
    fn finish(&self, a: Active, refocus: bool) {
        let front = target(a.front); // capture before anything can change focus
        emit(&self.app, "processing", None);
        let (samples, duration_ms) = a.rec.stop();
        cue(&self.db, "Pop"); // after stop, so the cue is not in the recording
        if self.jobs.send(Job { samples, duration_ms, front, refocus }).is_err() {
            emit(&self.app, "error", Some("Dictation stopped working. Quit and reopen Quietkey".into()));
        }
    }
}

/// Start and stop cues (Settings, Sound): macOS's own sounds through afplay, so there is no audio output code.
/// A thread waits for afplay so no zombie process is left.
fn cue(db: &Mutex<Connection>, name: &'static str) {
    if store::sounds_on(&db.lock().unwrap()) {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("/usr/bin/afplay").args(["-v", "0.5", &format!("/System/Library/Sounds/{name}.aiff")]).status();
        });
    }
}

/// The folder for saved recordings, one WAV per history entry.
pub fn recordings_dir(app: &AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app.path().app_data_dir()?.join("recordings");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Deletes recordings whose history entry is gone (deleted, purged after 24 h, or history switched off).
pub fn prune_recordings(app: &AppHandle, conn: &Connection) {
    let (Ok(dir), Ok(keep)) = (recordings_dir(app), db::audio_paths(conn)) else { return };
    for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if !keep.contains(f.path().to_string_lossy().as_ref()) {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

/// The app to paste into: the one in front now, unless that is Quietkey itself (after a click on the bar).
fn target(at_start: paste::FrontApp) -> paste::FrontApp {
    let now = paste::frontmost_app();
    if now.pid == Some(std::process::id() as i32) { at_start } else { now }
}

struct Worker {
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
}

impl Worker {
    fn run(mut self, jobs: Receiver<Job>) {
        for job in jobs {
            self.handle(job);
        }
    }

    fn handle(&mut self, job: Job) {
        let Job { samples, duration_ms, front, refocus } = job;
        #[cfg(debug_assertions)]
        let tv = Instant::now();
        let Some(span) = speech(&samples) else {
            emit(&self.app, "error", Some("Didn't hear anything".into()));
            return;
        };
        #[cfg(debug_assertions)]
        eprintln!("[timing] vad {:?}, kept {:.1}s of {:.1}s", tv.elapsed(), span.len() as f32 / 16_000.0, samples.len() as f32 / 16_000.0);
        let speech = &samples[span];
        match process(&self.db, speech, front.bundle_id.as_deref()) {
            Ok(out) if out.final_text.is_empty() => emit(&self.app, "error", Some("Didn't hear anything".into())),
            Ok(out) => {
                // Only refuse when macOS clearly reports a focused element that cannot take text (e.g. Finder).
                let accepts = paste::focused_accepts_text(front.pid);
                #[cfg(debug_assertions)]
                eprintln!("[focus] {:?} pid={:?} accepts_text={accepts:?}", front.name, front.pid);
                if accepts == Some(false) {
                    self.save(&out, duration_ms, &front, "paste_failed", Some("no_text_field"), speech);
                    emit(&self.app, "error", Some("No text field here. Click one, then press ⌃⌘V".into()));
                    return;
                }
                if refocus {
                    paste::give_focus(front.pid);
                }
                let tp = Instant::now();
                let pasted = paste::paste_text(&out.final_text);
                #[cfg(debug_assertions)]
                eprintln!("[timing] paste+restore {:?} into {:?}", tp.elapsed(), front.name);
                let (status, err, msg): (&str, Option<&str>, Option<String>) = match pasted {
                    PasteResult::Pasted => ("pasted", None, out.notice.clone()),
                    PasteResult::NoPermission => (
                        "paste_failed",
                        Some("accessibility"),
                        Some("Copied. Allow Accessibility so text can be typed for you".into()),
                    ),
                    PasteResult::Failed("clipboard_write") => ("paste_failed", Some("clipboard_write"), Some("Could not use the clipboard. Press ⌃⌘V to try again".into())),
                    PasteResult::Failed(e) => ("paste_failed", Some(e), Some("It's on your clipboard. Press ⌘V".into())),
                };
                self.save(&out, duration_ms, &front, status, err, speech);
                match (status, msg) {
                    ("pasted", None) => emit(&self.app, "success", None),
                    ("pasted", Some(m)) => emit(&self.app, "warning", Some(m)), // pasted, but cleanup fell back
                    (_, m) => emit(&self.app, "error", m),
                }
            }
            Err(e) => emit(&self.app, "error", Some(format!("Couldn't transcribe that: {e}"))),
        }
    }

}

/// Whisper -> snippets -> dictionary fixes -> local rules -> optional LLM -> snippet text.
/// The database lock is never held during transcription or a network call. Also used to transcribe a saved
/// recording again from History.
pub fn process(db: &Mutex<Connection>, samples: &[f32], bundle: Option<&str>) -> anyhow::Result<Processed> {
        let (cfg, words, fixes, snippets, tone) = {
            let conn = db.lock().unwrap();
            (
                store::pipeline_settings(&conn)?,
                store::dict_words(&conn),
                store::dict_corrections(&conn),
                store::snippets_for_cleanup(&conn),
                store::tone_for_app(&conn, bundle),
            )
        };
        let t0 = Instant::now();
        let prompt = words.join(", ");
        let mut notice = None;
        let (raw, language) = if cfg.stt_engine == "groq" {
            (llm::groq_transcribe(samples, cfg.language.as_deref(), &prompt)?, cfg.language.clone())
        } else {
            // The model locks are held only while transcribing: cleanup below may call Groq or Ollama.
            let (t, l, n) = transcribe_local(samples, &cfg, &prompt)?;
            notice = n;
            (t, l)
        };
        #[cfg(debug_assertions)]
        eprintln!("[timing] transcribe {:?} for {:.1}s audio", t0.elapsed(), samples.len() as f32 / 16_000.0);
        if raw.is_empty() {
            return Ok(Processed { raw, final_text: String::new(), cleanup_level: cfg.cleanup_level, notice: None, language: language.clone(), storage_mode: cfg.storage_mode, fixes: 0 });
        }

        let (with_ph, expansions) = cleanup::insert_placeholders(&raw, &snippets);
        let corrected = cleanup::apply_corrections(&with_ph, &fixes);
        let mut text = if cfg.cleanup_level == "none" { corrected.clone() } else { cleanup::local_cleanup(&corrected) };
        let mut llm_used = false;

        if cfg.cleanup_level != "none" && cfg.llm_provider != "none" && text.split_whitespace().count() >= 4 {
            let system = cleanup::llm_system_prompt(&cfg.cleanup_level, &tone, &words);
            let res = match cfg.llm_provider.as_str() {
                "groq" => llm::groq(&cfg.groq_model, &system, &text),
                "ollama" => llm::ollama(&cfg.ollama_url, &cfg.ollama_model, &system, &text),
                _ => Err(anyhow::anyhow!("unknown provider")),
            };
            match res {
                // Placeholders must survive, and the output must look like an edit, not an answer.
                Ok(out) if (0..expansions.len()).all(|i| out.contains(&format!("⟦S{}⟧", i + 1))) && llm::plausible_edit(&text, &out) => {
                    text = out;
                    llm_used = true;
                }
                Ok(_) => notice = Some("AI cleanup changed too much, so basic cleanup was used".into()),
                Err(e) => {
                    eprintln!("llm cleanup failed: {e}");
                    notice = Some("AI cleanup didn't answer, so basic cleanup was used".into());
                }
            }
        }
        if cfg.cleanup_level != "none" && !llm_used {
            text = cleanup::local_tone(&text, &tone);
        }
        let final_text = cleanup::expand_placeholders(&text, &expansions);
        let fixes = cleanup::fixes_count(&raw, &final_text);
        Ok(Processed { raw, final_text, cleanup_level: cfg.cleanup_level, notice, language: language.clone(), storage_mode: cfg.storage_mode, fixes })
}

impl Worker {
    fn save(&self, out: &Processed, duration_ms: i64, front: &paste::FrontApp, status: &str, err: Option<&str>, audio: &[f32]) {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.db.lock().unwrap();
        let _ = db::set_last_text(&out.final_text);
        if out.storage_mode == "never" {
            // History off: count the stats, keep no text.
            if let Err(e) = store::record_usage(&conn, &day, db::word_count(&out.final_text), duration_ms, out.fixes) {
                eprintln!("usage update failed: {e}");
            }
            return;
        }
        let row = db::insert_transcript(
            &conn,
            &NewTranscript {
                raw_text: &out.raw,
                final_text: &out.final_text,
                app_bundle_id: front.bundle_id.as_deref(),
                app_name: front.name.as_deref(),
                language: out.language.as_deref(),
                duration_ms,
                cleanup_level: &out.cleanup_level,
                status,
                error: err,
                fixes: out.fixes,
            },
            &day,
        );
        match row {
            Ok(mut t) => {
                // The recording, so History can play it and transcribe it again. Failing here loses only the audio.
                let saved = recordings_dir(&self.app).and_then(|d| {
                    let path = d.join(format!("{}.wav", t.id));
                    std::fs::write(&path, audio::wav_bytes(audio)?)?;
                    db::set_audio_path(&conn, &t.id, &path.to_string_lossy())?;
                    Ok(())
                });
                match saved {
                    Ok(()) => t.audio = true,
                    Err(e) => eprintln!("save recording failed: {e}"),
                }
                let _ = self.app.emit("transcript://created", t);
            }
            Err(e) => eprintln!("save transcript failed: {e}"),
        }
    }
}

pub struct Processed {
    pub raw: String,
    pub final_text: String,
    pub cleanup_level: String,
    pub notice: Option<String>,
    pub language: Option<String>,
    pub storage_mode: String,
    pub fixes: i64,
}

#[cfg(test)]
mod routing_bench {
    /// The language routing with the real models: Parakeet main, detection among en/de/ar, Cohere for Arabic.
    /// Run: QUIETKEY_FIXTURES=<dir of 16 kHz WAVs> cargo test --release --lib routing -- --ignored --nocapture
    #[test]
    #[ignore]
    fn routing() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_FIXTURES").unwrap());
        let models = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/dev.abdallah.quietkey/models");
        let _ = super::MODELS_DIR.set(models.clone());
        *super::STT.lock().unwrap() = Some(super::Stt::load(&models.join("parakeet-tdt-0.6b-v3-Q8_0.gguf")).unwrap());
        *super::LID.lock().unwrap() = Some(super::LangId::load(&models).unwrap());
        let cfg = crate::store::PipelineSettings {
            cleanup_level: "none".into(), llm_provider: "none".into(), groq_model: String::new(), ollama_model: String::new(),
            ollama_url: String::new(), storage_mode: "store".into(), language: None,
            candidates: vec!["en".into(), "de".into(), "ar".into()], stt_engine: "local".into(),
        };
        let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        names.sort();
        for p in names {
            let mut r = hound::WavReader::open(&p).unwrap();
            let a: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
            let t = std::time::Instant::now();
            let (text, lang, note) = super::transcribe_local(&a, &cfg, "").unwrap();
            println!("{:10} {:4.1}s {:>5} ms {:?} {text} {}", p.file_stem().unwrap().to_string_lossy(), a.len() as f32 / 16000.0, t.elapsed().as_millis(), lang.unwrap_or_default(), note.unwrap_or_default());
            assert!(!text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)), "Cyrillic in {text}");
        }
    }
}
