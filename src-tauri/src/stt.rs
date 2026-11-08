use std::path::Path;
use transcribe_cpp::{RunExtension, RunOptions, Session, WhisperRunOptions};

/// The models offered in Settings. Sizes are the download; RAM is what the app uses with the model loaded,
/// measured on an M-series Mac (cargo test --lib model_ram -- --ignored).
#[derive(serde::Serialize, Clone, Copy)]
pub struct Model {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size_mb: u32,
    pub ram_mb: u32,
    pub note: &'static str,
    /// School grades for the languages the app's users pick most, "-" = not supported. Judged from published
    /// FLEURS word error rates and tests on this Mac; shown on the Models page.
    pub grades: &'static [(&'static str, &'static str)],
    /// 1 (slow) to 4 (fastest), measured on an M1.
    pub speed: u8,
}

const WHISPER_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

pub const MODELS: &[Model] = &[
    Model {
        id: "parakeet-v3",
        label: "Parakeet v3",
        file: "parakeet-tdt-0.6b-v3-Q8_0.gguf",
        // Pinned revision so the file never changes under a user.
        url: "https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v3-gguf/resolve/85ac09ea12fc4b1112fa76810059364bc6adc9de/",
        size_mb: 740,
        ram_mb: 230,
        note: "Fastest and very accurate. English, German and 23 more European languages, picks the language itself. Ignores dictionary hints",
        grades: &[("en", "A-"), ("de", "B+"), ("ar", "-")],
        speed: 4,
    },
    Model {
        id: "cohere-ar",
        label: "Cohere Transcribe Arabic",
        file: "cohere-transcribe-arabic-07-2026-Q4_K_M.gguf",
        // Apache-2.0. Q4_K_M: same Arabic word error rate as Q8_0 (11.2 vs 11.1 on FLEURS) at 1.6 instead of 2.4 GB.
        url: "https://huggingface.co/handy-computer/cohere-transcribe-arabic-07-2026-gguf/resolve/11a54d6895f94cce78d0ece142b8e552e7822539/",
        size_mb: 1560,
        ram_mb: 100, // measured like the others (resident memory added); the weights are memory-mapped
        note: "Best for Arabic, including dialects. English words inside Arabic speech come out in Arabic letters. Slower, 1.6 GB",
        grades: &[("en", "B+"), ("de", "-"), ("ar", "A")],
        speed: 2,
    },
    Model { id: "base", label: "Whisper Base", file: "ggml-base-q5_1.bin", url: WHISPER_URL, size_mb: 60, ram_mb: 180, note: "Small download, for older Macs. Misses more words",
        grades: &[("en", "C+"), ("de", "D"), ("ar", "F")], speed: 4 },
    Model { id: "small", label: "Whisper Small", file: "ggml-small-q5_1.bin", url: WHISPER_URL, size_mb: 190, ram_mb: 350, note: "Good for clear English",
        grades: &[("en", "B"), ("de", "C+"), ("ar", "D")], speed: 3 },
    Model { id: "large-v3-turbo", label: "Whisper Large v3 Turbo", file: "ggml-large-v3-turbo-q5_0.bin", url: WHISPER_URL, size_mb: 574, ram_mb: 700, note: "99 languages, including Arabic. Slower, uses dictionary words as hints",
        grades: &[("en", "A-"), ("de", "A-"), ("ar", "C")], speed: 2 },
];

/// The model for a settings value; unknown values get the default (Parakeet). Benches only.
#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn every_model_is_graded_for_en_de_ar_with_a_speed() {
        for m in MODELS {
            let langs: Vec<_> = m.grades.iter().map(|g| g.0).collect();
            assert_eq!(langs, ["en", "de", "ar"], "{}", m.id);
            assert!((1..=4).contains(&m.speed), "{}", m.id);
            assert!(m.grades.iter().all(|g| g.1 == "-" || "ABCDF".contains(&g.1[..1])), "{}", m.id);
        }
    }

    #[test]
    fn arabic_first_for_models_that_cannot_detect() {
        assert_eq!(fixed_languages(vec!["en".into(), "ar".into()]), ["ar", "en"]);
        assert_eq!(fixed_languages(vec!["en".into()]), ["en"]);
    }
}

#[cfg(test)]
pub fn model(id: Option<&str>) -> &'static Model {
    MODELS.iter().find(|m| Some(m.id) == id).unwrap_or(&MODELS[0])
}

/// The model to load: the saved choice if its file is on disk, else the first model on disk (Parakeet first),
/// else the default. A fresh install has a saved choice (the column default) but no file, so it gets Parakeet.
pub fn active(choice: Option<&str>, dir: &Path) -> &'static Model {
    if let Some(m) = MODELS.iter().find(|m| Some(m.id) == choice && dir.join(m.file).exists()) {
        return m;
    }
    MODELS.iter().find(|m| dir.join(m.file).exists()).unwrap_or(&MODELS[0])
}

/// Downloads `m` into `dir` (a .part file, renamed once complete). `progress(done, total)` gets byte counts.
// shortcut: checks the byte count, not a checksum; a corrupt file fails at Stt::load. Add sha256 if that happens.
pub fn download(m: &Model, dir: &Path, progress: impl Fn(u64, u64)) -> anyhow::Result<std::path::PathBuf> {
    use std::io::{Read, Write};
    let dest = dir.join(m.file);
    if dest.exists() {
        return Ok(dest);
    }
    let url = format!("{}{}", m.url, m.file);
    let mut resp = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()?
        .get(&url)
        .send()?
        .error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let part = dir.join(format!("{}.part", m.file));
    let mut out = std::fs::File::create(&part)?;
    let (mut done, mut buf, mut last) = (0u64, vec![0u8; 1 << 16], 0u64);
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        if done - last >= total / 200 + 1 {
            last = done;
            progress(done, total);
        }
    }
    out.sync_all()?;
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&part);
        anyhow::bail!("download stopped at {done} of {total} bytes");
    }
    std::fs::rename(&part, &dest)?;
    progress(done, total);
    Ok(dest)
}

/// Loaded once and kept in memory: loading the model per dictation would add ~1 s.
/// transcribe.cpp runs both families: Parakeet (.gguf) and Whisper (whisper.cpp .bin files).
pub struct Stt {
    // Reused across calls: a fresh session re-allocates GPU buffers. It keeps the model alive.
    session: Session,
    /// Whisper takes a language hint and an initial prompt; Parakeet detects the language and takes neither.
    whisper: bool,
    /// Models that cannot detect the language (Cohere) must be told one: its supported languages, best first.
    fixed: Vec<String>,
}

impl Stt {
    pub fn load(model: &Path) -> anyhow::Result<Self> {
        let _ = transcribe_cpp::disable_logging(); // it prints every buffer allocation to stderr
        let m = transcribe_cpp::Model::load(model).map_err(|e| anyhow::anyhow!("model load failed: {e}"))?;
        let whisper = m.arch() == "whisper";
        let caps = m.capabilities().map_err(|e| anyhow::anyhow!("{e}"))?;
        let fixed = if caps.supports_language_detect { Vec::new() } else { fixed_languages(caps.languages) };
        let session = m.session().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { session, whisper, fixed })
    }

    /// `language` None = auto-detect. `prompt` biases spelling (dictionary words). Parakeet ignores both.
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>, prompt: &str) -> anyhow::Result<String> {
        Ok(self.run(samples, language, prompt)?.0)
    }

    /// Auto-detect limited to `candidates`: if Whisper hears a language outside them, transcribe again in the
    /// first one. Returns the text and the language used. A separate detection pass would cost ~2 s every time.
    /// Parakeet cannot be told a language, so its first result stands.
    pub fn transcribe_among(&mut self, samples: &[f32], candidates: &[String], prompt: &str) -> anyhow::Result<(String, Option<String>)> {
        if !self.fixed.is_empty() {
            // No detection: the model's main language. Cohere Arabic in Arabic mode also writes the English words
            // mixed in, while English mode drops Arabic, so a user who speaks both gets Arabic mode.
            let l = self.fixed[0].clone();
            let (text, _) = self.run(samples, Some(&l), prompt)?;
            return Ok((text, Some(l)));
        }
        let (text, heard) = self.run(samples, None, prompt)?;
        if !self.whisper {
            return Ok((text, heard));
        }
        match retry_language(heard.as_deref(), candidates) {
            None => Ok((text, heard)),
            Some(l) => Ok((self.transcribe(samples, Some(l), prompt)?, Some(l.to_string()))),
        }
    }

    /// The text and the language the model reports, if any.
    fn run(&mut self, samples: &[f32], language: Option<&str>, prompt: &str) -> anyhow::Result<(String, Option<String>)> {
        let mut opts = RunOptions::default();
        let mut audio = samples.to_vec();
        if !self.fixed.is_empty() {
            let l = language.filter(|l| self.fixed.iter().any(|f| f == l)).unwrap_or(self.fixed[0].as_str());
            opts.language = Some(l.to_string());
        }
        if self.whisper {
            opts.language = language.map(str::to_string);
            if !prompt.is_empty() {
                opts.family = Some(RunExtension::Whisper(WhisperRunOptions { initial_prompt: Some(prompt.to_string()), ..Default::default() }));
            }
            // Whisper hallucinates on very short input; pad to 1 s of silence.
            if audio.len() < 16_000 {
                audio.resize(16_000 + 1600, 0.0);
            }
        }
        let r = self.session.run(&audio, &opts).map_err(|e| anyhow::anyhow!("transcribe failed: {e}"))?;
        Ok((clean_whisper_text(&r.text), r.language))
    }
}

const SILERO: &[u8] = include_bytes!("../models/silero-vad-v6.2-F32.gguf");
const SILERO_FILE: &str = "silero-vad-v6.2-F32.gguf";
/// Kept around the speech: Silero ends a segment a little early on soft final sounds ("thanks").
const PAD: usize = 3_200; // 200 ms at 16 kHz

/// Silero voice activity detection (MIT, 1.2 MB, built into the app). Rejects silence and steady noise that the
/// models would turn into made-up text, and cuts the silence before and after speech so less audio is decoded.
pub struct Vad {
    session: transcribe_cpp::VadSession,
}

impl Vad {
    /// transcribe.cpp loads from a file, so the built-in model is written into `dir` first when missing or partial.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(SILERO_FILE);
        if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(SILERO.len() as u64) {
            std::fs::create_dir_all(dir)?;
            std::fs::write(&path, SILERO)?;
        }
        let m = transcribe_cpp::Model::load(&path).map_err(|e| anyhow::anyhow!("VAD load failed: {e}"))?;
        Ok(Self { session: m.vad_session().map_err(|e| anyhow::anyhow!("{e}"))? })
    }

    /// The samples from the first speech to the last, padded; None when the clip has no speech.
    pub fn speech(&mut self, samples: &[f32]) -> anyhow::Result<Option<std::ops::Range<usize>>> {
        // 100 ms minimum instead of 250: a one-word answer like "yes" must count as speech.
        let opts = transcribe_cpp::VadOptions { min_speech_ms: 100, ..Default::default() };
        let r = self.session.run(samples, &opts).map_err(|e| anyhow::anyhow!("VAD failed: {e}"))?;
        let segs: Vec<(usize, usize)> = r.segments.iter().map(|s| (s.start_sample.max(0) as usize, s.end_sample.max(0) as usize)).collect();
        Ok(speech_span(&segs, samples.len()))
    }
}

/// First segment start to last segment end, with PAD on both sides. Pauses inside are kept: cutting them could
/// join words and would change what the models hear.
fn speech_span(segs: &[(usize, usize)], len: usize) -> Option<std::ops::Range<usize>> {
    let (first, last) = (segs.first()?.0, segs.last()?.1);
    Some(first.saturating_sub(PAD).min(len)..last.saturating_add(PAD).min(len))
}

/// A no-detection model's languages with Arabic first: the Arabic models also take English, but code-switched
/// Arabic and English speech is transcribed best in Arabic mode.
fn fixed_languages(mut langs: Vec<String>) -> Vec<String> {
    langs.sort_by_key(|l| l != "ar");
    langs
}

/// The language to transcribe again in, or None when what Whisper heard is one of the user's languages.
fn retry_language<'a>(heard: Option<&str>, candidates: &'a [String]) -> Option<&'a str> {
    match heard {
        Some(h) if candidates.iter().any(|c| c == h) => None,
        _ => candidates.first().map(String::as_str),
    }
}

const NON_SPEECH: &[&str] = &[
    "music", "laughs", "laughter", "silence", "applause", "inaudible", "coughs", "coughing", "sighs", "noise", "blank_audio",
];

/// Drops Whisper's non-speech tags ("[BLANK_AUDIO]", "(music)") and tidies whitespace.
/// Real parentheses the user dictated, like "(see above)", are kept.
pub fn clean_whisper_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['[', '(']) {
        let close = if rest.as_bytes()[i] == b'[' { ']' } else { ')' };
        let Some(j) = rest[i..].find(close).map(|j| i + j) else { break };
        let inner = rest[i + 1..j].trim().to_lowercase();
        out.push_str(&rest[..i]);
        if close == ')' && !NON_SPEECH.contains(&inner.as_str()) {
            out.push_str(&rest[i..=j]);
        }
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{clean_whisper_text, retry_language, speech_span, PAD};

    #[test]
    fn speech_span_pads_and_clamps() {
        assert_eq!(speech_span(&[], 16_000), None);
        assert_eq!(speech_span(&[(8_000, 12_000)], 16_000), Some(8_000 - PAD..12_000 + PAD));
        assert_eq!(speech_span(&[(1_000, 2_000), (9_000, 15_000)], 16_000), Some(0..16_000)); // clamped both ends
    }

    #[test]
    fn active_model_prefers_choice_then_disk() {
        let dir = std::env::temp_dir().join(format!("qk-active-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(super::active(None, &dir).id, "parakeet-v3"); // fresh install
        std::fs::write(dir.join("ggml-large-v3-turbo-q5_0.bin"), b"x").unwrap();
        assert_eq!(super::active(None, &dir).id, "large-v3-turbo"); // old install without a saved choice keeps its model
        assert_eq!(super::active(Some("small"), &dir).id, "large-v3-turbo"); // choice without its file
        std::fs::write(dir.join("ggml-small-q5_1.bin"), b"x").unwrap();
        assert_eq!(super::active(Some("small"), &dir).id, "small");
        assert_eq!(super::active(Some("gone"), &dir).id, "small"); // first on disk in list order
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn retries_only_outside_chosen_languages() {
        let picked = ["en".to_string(), "de".to_string()];
        assert_eq!(retry_language(Some("de"), &picked), None);
        assert_eq!(retry_language(Some("fr"), &picked), Some("en"));
        assert_eq!(retry_language(None, &picked), Some("en"));
        assert_eq!(retry_language(Some("fr"), &[]), None);
    }

    #[test]
    fn strips_tags_and_spaces() {
        assert_eq!(clean_whisper_text(" [BLANK_AUDIO]"), "");
        assert_eq!(clean_whisper_text(" Hello  world. (music) Bye"), "Hello world. Bye");
        assert_eq!(clean_whisper_text("Plain text"), "Plain text");
        assert_eq!(clean_whisper_text("Check this (see above) now"), "Check this (see above) now");
        assert_eq!(clean_whisper_text("Open ( no close"), "Open ( no close");
    }
}

#[cfg(test)]
mod bench {
    /// Run: QUIETKEY_TEST_WAV=/path/16k-f32.wav cargo test --release --lib bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn transcribe_wav_timing() {
        let wav = std::fs::read(std::env::var("QUIETKEY_TEST_WAV").unwrap()).unwrap();
        let at = wav.windows(4).position(|w| w == b"data").unwrap() + 8; // 32-bit float PCM after the data chunk header
        let samples: Vec<f32> = wav[at..].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models").join(super::model(std::env::var("QK_MODEL").ok().as_deref()).file);
        let t0 = std::time::Instant::now();
        let mut stt = super::Stt::load(&model).unwrap();
        let load = t0.elapsed();
        for run in 0..3 {
            let t1 = std::time::Instant::now();
            let text = stt.transcribe(&samples, Some("en"), "").unwrap();
            println!("run {run}: {:?} for {:.1}s audio -> {text}", t1.elapsed(), samples.len() as f32 / 16000.0);
        }
        println!("model load {load:?}");
    }
}

#[cfg(test)]
mod lang_bench {
    /// Run: QUIETKEY_FIXTURES=../scripts/e2e/fixtures cargo test --release --lib lang_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn detects_among_picked() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_FIXTURES").unwrap());
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models").join(super::model(std::env::var("QK_MODEL").ok().as_deref()).file);
        let mut stt = super::Stt::load(&model).unwrap();
        let picked = ["en".to_string(), "de".to_string()];
        for (n, want) in [("german", "de"), ("short", "en"), ("speech", "en")] {
            let mut r = hound::WavReader::open(dir.join(format!("{n}.wav"))).unwrap();
            let a: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
            let t = std::time::Instant::now();
            let (text, got) = stt.transcribe_among(&a, &picked, "").unwrap();
            println!("{n}: {got:?} in {:?}: {}", t.elapsed(), text.chars().take(50).collect::<String>());
            assert_eq!(got.as_deref(), Some(want));
        }
    }
}

#[cfg(test)]
mod mixed_bench {
    /// English with German words inside, per language mode, with and without the words in the dictionary prompt.
    /// Run: QUIETKEY_MIX_DIR=/dir/with/wavs cargo test --release --lib mixed_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn english_with_german_words() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_MIX_DIR").unwrap());
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models").join(super::model(std::env::var("QK_MODEL").ok().as_deref()).file);
        let mut stt = super::Stt::load(&model).unwrap();
        let picked = ["en".to_string(), "de".to_string()];
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        files.sort();
        for f in files {
            let mut r = hound::WavReader::open(&f).unwrap();
            let a: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
            println!("== {}", f.file_name().unwrap().to_string_lossy());
            println!("  en only     : {}", stt.transcribe(&a, Some("en"), "").unwrap());
            println!("  auto        : {}", stt.transcribe(&a, None, "").unwrap());
            println!("  en+de picked: {}", stt.transcribe_among(&a, &picked, "").unwrap().0);
            let dict = "Kinderwagen, Hauptbahnhof, Brötchen, Geschäftsführer";
            println!("  en + dict   : {}", stt.transcribe(&a, Some("en"), dict).unwrap());
        }
    }
}

#[cfg(test)]
mod ctx_bench {
    /// Cohere Arabic end to end: Arabic speech, and Arabic with English words, in a multi-language setup.
    /// Run: QUIETKEY_FIXTURES=<dir with arabic.wav, mixed.wav> cargo test --release --lib cohere_arabic -- --ignored --nocapture
    #[test]
    #[ignore]
    fn cohere_arabic() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_FIXTURES").unwrap());
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models").join(super::model(Some("cohere-ar")).file);
        let mut stt = super::Stt::load(&model).unwrap();
        for n in ["arabic", "mixed"] {
            let mut r = hound::WavReader::open(dir.join(format!("{n}.wav"))).unwrap();
            let a: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
            let t = std::time::Instant::now();
            let (out, lang) = stt.transcribe_among(&a, &["en".into(), "de".into()], "").unwrap();
            println!("{n:7} {:4.1}s audio {:>6} ms {lang:?} {out}", a.len() as f32 / 16000.0, t.elapsed().as_millis());
            assert!(out.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)), "no Arabic in {out}");
        }
    }

    /// Quality and speed per fixture, long clip first (reproduces the suite order). QK_MODEL picks the model.
    /// Run: QUIETKEY_FIXTURES=../scripts/e2e/fixtures cargo test --release --lib ctx_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn window_sizes() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_FIXTURES").unwrap());
        let load = |n: &str| -> Vec<f32> {
            let mut r = hound::WavReader::open(dir.join(format!("{n}.wav"))).unwrap();
            r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
        };
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models").join(super::model(std::env::var("QK_MODEL").ok().as_deref()).file);
        let mut stt = super::Stt::load(&model).unwrap();
        {
            let names: Vec<(&str, Option<&str>)> = match std::env::var("QUIETKEY_BENCH_ONLY") {
                Ok(only) => only.split(',').map(|n| (Box::leak(n.to_string().into_boxed_str()) as &str, Some("en"))).collect(),
                Err(_) => vec![("long", Some("en")), ("short", Some("en")), ("raw", Some("en")), ("german", Some("de")), ("speech", Some("en"))],
            };
            for (n, lang) in names {
                let a = load(n);
                let t = std::time::Instant::now();
                let out = stt.transcribe(&a, lang, "").unwrap();
                let shown: String = out.chars().take(70).collect();
                println!("{:7} {:5.1}s audio  {:>6.0} ms  {:>3} words  {}", n, a.len() as f32 / 16000.0, t.elapsed().as_millis(), out.split_whitespace().count(), shown);
            }
        }
    }

    /// F6 RAM figures: downloads the model if missing, loads it, transcribes 5 s, prints the resident memory added.
    /// One model per process: QK_MODEL=small cargo test --lib model_ram -- --ignored --nocapture
    #[test]
    #[ignore]
    fn model_ram() {
        let rss = || -> u64 {
            let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap() / 1024
        };
        let m = super::model(std::env::var("QK_MODEL").ok().as_deref());
        let home = std::env::var("HOME").unwrap();
        let dir = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.quietkey/models");
        let path = super::download(m, &dir, |d, t| if d == t { println!("downloaded {d} bytes") }).unwrap();
        let before = rss();
        let mut stt = super::Stt::load(&path).unwrap();
        let tone: Vec<f32> = (0..80_000).map(|i| (i as f32 * 0.05).sin() * 0.1).collect();
        let _ = stt.transcribe(&tone, Some("en"), "");
        println!("{} RAM {} MB", m.id, rss() - before);
    }
}


#[cfg(test)]
mod vad_bench {
    /// Speech kept and time per fixture. Run: QUIETKEY_FIXTURES=../scripts/e2e/fixtures cargo test --release --lib vad_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn spans() {
        let dir = std::path::PathBuf::from(std::env::var("QUIETKEY_FIXTURES").unwrap());
        let mut vad = super::Vad::load(&std::env::temp_dir().join("qk-vad")).unwrap();
        for n in ["short", "silence", "correction", "speech", "german", "varied", "long"] {
            let mut r = hound::WavReader::open(dir.join(format!("{n}.wav"))).unwrap();
            let a: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
            let t = std::time::Instant::now();
            let span = vad.speech(&a).unwrap();
            println!("{n:10} {:5.1}s  {:>5.1} ms  kept {:?}", a.len() as f32 / 16000.0, t.elapsed().as_secs_f32() * 1000.0,
                span.map(|s| format!("{:.2}..{:.2}s", s.start as f32 / 16000.0, s.end as f32 / 16000.0)));
        }
        // Fan-like noise at about -30 dBFS: loud enough for the old loudness check, no speech.
        let mut x = 1u32;
        let noise: Vec<f32> = (0..48_000).map(|_| { x ^= x << 13; x ^= x >> 17; x ^= x << 5; (x as f32 / u32::MAX as f32 - 0.5) * 0.11 }).collect();
        println!("noise      loud={} vad={:?}", crate::audio::has_speech(&noise), vad.speech(&noise).unwrap());
        let mut r = hound::WavReader::open(dir.join("speech.wav")).unwrap();
        let mut padded = vec![0.0f32; 32_000];
        padded.extend(r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0));
        padded.extend(vec![0.0f32; 32_000]);
        println!("padded     {:.1}s vad={:?}", padded.len() as f32 / 16000.0, vad.speech(&padded).unwrap().map(|s| (s.start as f32 / 16000.0, s.end as f32 / 16000.0)));
    }
}
