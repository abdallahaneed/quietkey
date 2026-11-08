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
    },
    Model { id: "base", label: "Whisper Base", file: "ggml-base-q5_1.bin", url: WHISPER_URL, size_mb: 60, ram_mb: 180, note: "Small download, for older Macs. Misses more words" },
    Model { id: "small", label: "Whisper Small", file: "ggml-small-q5_1.bin", url: WHISPER_URL, size_mb: 190, ram_mb: 350, note: "Good for clear English" },
    Model { id: "large-v3-turbo", label: "Whisper Large v3 Turbo", file: "ggml-large-v3-turbo-q5_0.bin", url: WHISPER_URL, size_mb: 574, ram_mb: 700, note: "99 languages, including Arabic. Slower, uses dictionary words as hints" },
];

/// The model for a settings value; unknown values get the default (Parakeet). Benches only.
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
}

impl Stt {
    pub fn load(model: &Path) -> anyhow::Result<Self> {
        let _ = transcribe_cpp::disable_logging(); // it prints every buffer allocation to stderr
        let m = transcribe_cpp::Model::load(model).map_err(|e| anyhow::anyhow!("model load failed: {e}"))?;
        let whisper = m.arch() == "whisper";
        let session = m.session().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { session, whisper })
    }

    /// `language` None = auto-detect. `prompt` biases spelling (dictionary words). Parakeet ignores both.
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>, prompt: &str) -> anyhow::Result<String> {
        Ok(self.run(samples, language, prompt)?.0)
    }

    /// Auto-detect limited to `candidates`: if Whisper hears a language outside them, transcribe again in the
    /// first one. Returns the text and the language used. A separate detection pass would cost ~2 s every time.
    /// Parakeet cannot be told a language, so its first result stands.
    pub fn transcribe_among(&mut self, samples: &[f32], candidates: &[String], prompt: &str) -> anyhow::Result<(String, Option<String>)> {
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
    use super::{clean_whisper_text, retry_language};

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

