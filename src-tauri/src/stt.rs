use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

pub const MODEL_FILE: &str = "ggml-large-v3-turbo-q5_0.bin";

/// Loaded once and kept in memory: loading the model per dictation would add ~1 s.
pub struct Stt {
    _ctx: WhisperContext,
    // Reused across calls: a fresh state re-allocates GPU buffers (~0.5 s each time).
    state: WhisperState,
}

impl Stt {
    pub fn load(model: &Path) -> anyhow::Result<Self> {
        let path = model.to_str().ok_or_else(|| anyhow::anyhow!("model path is not UTF-8"))?;
        let mut cp = WhisperContextParameters::default();
        cp.flash_attn(true);
        let ctx = WhisperContext::new_with_params(path, cp).map_err(|e| anyhow::anyhow!("model load failed: {e}"))?;
        let state = ctx.create_state().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { _ctx: ctx, state })
    }

    /// `language` None = auto-detect. `prompt` biases spelling (dictionary words).
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>, prompt: &str) -> anyhow::Result<String> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(language.or(Some("auto")));
        p.set_n_threads(std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).min(8));
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_suppress_blank(true);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        // Whisper hallucinates on very short input; pad to 1 s of silence.
        let mut audio = samples.to_vec();
        if audio.len() < 16_000 {
            audio.resize(16_000 + 1600, 0.0);
        }
        p.set_audio_ctx(audio_ctx(audio.len()));
        self.state.full(p, &audio).map_err(|e| anyhow::anyhow!("transcribe failed: {e}"))?;
        let text: String = self.state.as_iter().map(|seg| seg.to_string()).collect::<Vec<_>>().join("");
        Ok(clean_whisper_text(&text))
    }
}

const NON_SPEECH: &[&str] = &[
    "music", "laughs", "laughter", "silence", "applause", "inaudible", "coughs", "coughing", "sighs", "noise", "blank_audio",
];

/// Whisper's encoder always runs a 30 s window (1500 frames). Dictation clips are short, so shrink the window to
/// the clip plus a margin: much faster, nearly the same accuracy. Clips of 28 s or more use the full window.
pub fn audio_ctx(samples: usize) -> i32 {
    let frames = (samples as f64 / 16_000.0 / 30.0 * 1500.0).ceil() as i32;
    (frames + 128).clamp(384, 1500)
}

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
    use super::{audio_ctx, clean_whisper_text};

    #[test]
    fn audio_ctx_bounds() {
        assert_eq!(audio_ctx(16_000), 384); // 1 s -> floor
        assert_eq!(audio_ctx(16_000 * 10), 500 + 128);
        assert_eq!(audio_ctx(16_000 * 60), 1500); // long clip -> full window
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
    /// Run: FLOW_TEST_WAV=/path/16k-f32.wav cargo test --release --lib bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn transcribe_wav_timing() {
        let wav = std::fs::read(std::env::var("FLOW_TEST_WAV").unwrap()).unwrap();
        let at = wav.windows(4).position(|w| w == b"data").unwrap() + 8; // 32-bit float PCM after the data chunk header
        let samples: Vec<f32> = wav[at..].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home).join("Library/Application Support/dev.abdallah.utter/models").join(super::MODEL_FILE);
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
