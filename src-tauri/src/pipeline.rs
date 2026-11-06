//! Dictation in two threads:
//! - the state machine turns hotkey events into recordings and never blocks, so a new fn press always records;
//! - the worker transcribes, cleans up, pastes and saves finished recordings one at a time, in order.

use crate::audio::{self, Recording};
use crate::db::{self, NewTranscript};
use crate::hotkey::HotkeyEvent;
use crate::paste::{self, PasteResult};
use crate::stt::Stt;
use crate::{cleanup, llm, store};
use rusqlite::Connection;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const MIN_HOLD: Duration = Duration::from_millis(500); // shorter taps are accidental presses, ignore them
const WARN_AT: Duration = Duration::from_secs(19 * 60);
const STOP_AT: Duration = Duration::from_secs(20 * 60);

/// Commands from the UI or the hotkey thread, merged into one channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Key(HotkeyEvent),
    Cancel,
    Toggle,
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
    started: Instant,
    mode: Mode,
    warned: bool,
}

/// A finished recording waiting for the worker.
struct Job {
    samples: Vec<f32>,
    duration_ms: i64,
    front: paste::FrontApp,
}

pub struct Pipeline {
    pub app: AppHandle,
    pub db: Arc<Mutex<Connection>>,
    pub stt: Option<Stt>,
}

fn emit(app: &AppHandle, state: &'static str, message: Option<String>) {
    let _ = app.emit("dictation://state", StatePayload { state, message });
}

impl Pipeline {
    pub fn run(self, rx: Receiver<Input>) {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let ready = self.stt.is_some();
        let worker = Worker { app: self.app.clone(), db: self.db.clone(), stt: self.stt };
        if std::thread::Builder::new().name("dictation-worker".into()).spawn(move || worker.run(job_rx)).is_err() {
            emit(&self.app, "error", Some("Could not start the dictation worker".into()));
            return;
        }
        Machine { app: self.app, db: self.db, jobs, ready }.run(rx);
    }
}

struct Machine {
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    jobs: Sender<Job>,
    ready: bool,
}

impl Machine {
    fn run(self, rx: Receiver<Input>) {
        let mut active: Option<Active> = None;
        loop {
            let input = match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(i) => Some(i),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };

            // Session guard runs on every tick.
            if let Some(a) = active.as_mut() {
                let el = a.started.elapsed();
                if el >= STOP_AT {
                    let a = active.take().unwrap();
                    self.finish(a);
                    continue;
                } else if el >= WARN_AT && !a.warned {
                    a.warned = true;
                    emit(&self.app, "warning", Some("One minute left in this session".into()));
                }
            }

            let Some(input) = input else { continue };
            #[cfg(debug_assertions)]
            eprintln!("[input] {input:?} recording={}", active.is_some());
            active = match (active, input) {
                (None, Input::Key(HotkeyEvent::FnDown)) => self.start(Mode::Hold),
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
                        emit(&self.app, "idle", None);
                    } else {
                        self.finish(a);
                    }
                    None
                }
                (Some(a), Input::Key(HotkeyEvent::FnCombo)) if a.mode == Mode::Hold => {
                    let _ = a.rec.stop(); // Fn+other key is a shortcut, not dictation
                    emit(&self.app, "idle", None);
                    None
                }
                // The FnUp that follows arrives with no recording active and is ignored.
                (Some(a), Input::Key(HotkeyEvent::FnDown) | Input::Toggle) if a.mode == Mode::Locked => {
                    self.finish(a);
                    None
                }
                (Some(a), Input::Key(HotkeyEvent::Escape) | Input::Cancel) => {
                    let _ = a.rec.stop();
                    // TODO: offer Undo to transcribe the cancelled audio after all.
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
                    emit(&self.app, "error", Some("Copied. Press ⌘V to paste".into()));
                }
            }
            None => emit(&self.app, "error", Some("Nothing dictated yet".into())),
        }
    }

    fn start(&self, mode: Mode) -> Option<Active> {
        if !self.ready {
            emit(&self.app, "error", Some("Speech model not loaded".into()));
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
        match audio::start(on_level) {
            Ok(rec) => {
                emit(&self.app, if mode == Mode::Hold { "recording" } else { "hands_free" }, None);
                Some(Active { rec, started: Instant::now(), mode, warned: false })
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
    fn finish(&self, a: Active) {
        let front = paste::frontmost_app(); // capture before anything can change focus
        emit(&self.app, "processing", None);
        let (samples, duration_ms) = a.rec.stop();
        if self.jobs.send(Job { samples, duration_ms, front }).is_err() {
            emit(&self.app, "error", Some("Dictation worker stopped".into()));
        }
    }
}

struct Worker {
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    stt: Option<Stt>,
}

impl Worker {
    fn run(mut self, jobs: Receiver<Job>) {
        for job in jobs {
            self.handle(job);
        }
    }

    fn handle(&mut self, job: Job) {
        let Job { samples, duration_ms, front } = job;
        if !audio::has_speech(&samples) {
            emit(&self.app, "error", Some("No speech heard".into()));
            return;
        }
        match self.process(&samples, front.bundle_id.as_deref()) {
            Ok(out) if out.final_text.is_empty() => emit(&self.app, "error", Some("No speech heard".into())),
            Ok(out) => {
                // Only refuse when macOS clearly reports a focused element that cannot take text (e.g. Finder).
                let accepts = paste::focused_accepts_text(front.pid);
                #[cfg(debug_assertions)]
                eprintln!("[focus] {:?} pid={:?} accepts_text={accepts:?}", front.name, front.pid);
                if accepts == Some(false) {
                    self.save(&out, duration_ms, &front, "paste_failed", Some("no_text_field"));
                    emit(&self.app, "error", Some("No text field selected. Click one, then press ⌃⌘V".into()));
                    return;
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
                    PasteResult::Failed(e) => ("paste_failed", Some(e), Some("Copied. Press ⌘V to paste".into())),
                };
                self.save(&out, duration_ms, &front, status, err);
                match (status, msg) {
                    ("pasted", None) => emit(&self.app, "success", None),
                    ("pasted", Some(m)) => emit(&self.app, "warning", Some(m)), // pasted, but cleanup fell back
                    (_, m) => emit(&self.app, "error", m),
                }
            }
            Err(e) => emit(&self.app, "error", Some(format!("Transcription failed: {e}"))),
        }
    }

    /// Whisper -> snippets -> dictionary fixes -> local rules -> optional LLM -> snippet text.
    /// The database lock is never held during transcription or a network call.
    fn process(&mut self, samples: &[f32], bundle: Option<&str>) -> anyhow::Result<Processed> {
        let (cfg, words, fixes, snippets, tone) = {
            let conn = self.db.lock().unwrap();
            (
                store::pipeline_settings(&conn)?,
                store::dict_words(&conn),
                store::dict_corrections(&conn),
                store::snippets_for_cleanup(&conn),
                store::tone_for_app(&conn, bundle),
            )
        };
        let stt = self.stt.as_mut().ok_or_else(|| anyhow::anyhow!("model not loaded"))?;
        let t0 = Instant::now();
        let raw = stt.transcribe(samples, cfg.language.as_deref(), &words.join(", "))?;
        #[cfg(debug_assertions)]
        eprintln!("[timing] transcribe {:?} for {:.1}s audio", t0.elapsed(), samples.len() as f32 / 16_000.0);
        if raw.is_empty() {
            return Ok(Processed { raw, final_text: String::new(), cleanup_level: cfg.cleanup_level, notice: None, language: cfg.language, storage_mode: cfg.storage_mode, fixes: 0 });
        }

        let (with_ph, expansions) = cleanup::insert_placeholders(&raw, &snippets);
        let corrected = cleanup::apply_corrections(&with_ph, &fixes);
        let mut text = if cfg.cleanup_level == "none" { corrected.clone() } else { cleanup::local_cleanup(&corrected) };
        let mut notice = None;

        if cfg.cleanup_level != "none" && cfg.llm_provider != "none" && text.split_whitespace().count() >= 4 {
            let system = cleanup::llm_system_prompt(&cfg.cleanup_level, &tone, &words);
            let res = match cfg.llm_provider.as_str() {
                "groq" => llm::groq(&cfg.groq_model, &system, &text),
                "ollama" => llm::ollama(&cfg.ollama_url, &cfg.ollama_model, &system, &text),
                _ => Err(anyhow::anyhow!("unknown provider")),
            };
            match res {
                // Placeholders must survive, and the output must look like an edit, not an answer.
                Ok(out) if (0..expansions.len()).all(|i| out.contains(&format!("⟦S{}⟧", i + 1))) && llm::plausible_edit(&text, &out) => text = out,
                Ok(_) => notice = Some("AI cleanup skipped: result did not match what you said".into()),
                Err(e) => {
                    eprintln!("llm cleanup failed: {e}");
                    notice = Some("AI cleanup unavailable, used basic cleanup".into());
                }
            }
        }
        let final_text = cleanup::expand_placeholders(&text, &expansions);
        let fixes = cleanup::fixes_count(&raw, &final_text);
        Ok(Processed { raw, final_text, cleanup_level: cfg.cleanup_level, notice, language: cfg.language, storage_mode: cfg.storage_mode, fixes })
    }

    fn save(&self, out: &Processed, duration_ms: i64, front: &paste::FrontApp, status: &str, err: Option<&str>) {
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
            Ok(t) => {
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
