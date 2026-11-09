use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const WHISPER_RATE: u32 = 16_000;

/// How long the input stream stays open after a recording. Opening one takes 100 ms to seconds (Bluetooth),
/// which cut off the first words; a warm stream records the moment fn goes down. macOS shows the mic dot meanwhile.
const WARM_FOR: Duration = Duration::from_secs(20);

type Level = Box<dyn Fn(f32) + Send>;

/// What the audio callback writes into. Samples are only kept while `on_level` is set (a recording runs).
struct Shared {
    samples: Mutex<Vec<f32>>,
    on_level: Mutex<Option<Level>>,
    /// Set when the last recording on this stream ended; None while recording.
    idle_since: Mutex<Option<Instant>>,
    /// The device reported an error (unplugged): never reuse this stream.
    dead: AtomicBool,
}

/// An open input stream. cpal streams are not Send on macOS, so it lives on its own thread until `stop_tx` fires
/// or it has been idle for WARM_FOR.
struct Stream {
    mic: Option<String>,
    rate: u32,
    used_fallback: bool,
    shared: Arc<Shared>,
    stop_tx: mpsc::Sender<()>,
}

/// The one stream kept warm between recordings.
static WARM: Mutex<Option<Arc<Stream>>> = Mutex::new(None);

/// A running capture on a (possibly warm) stream.
pub struct Recording {
    stream: Arc<Stream>,
    /// The chosen microphone was not found and the system default was used instead.
    pub used_fallback: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Mic {
    pub name: String,
    pub is_default: bool,
}

fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|desc| desc.name().to_string())
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

/// Microphone permission as macOS reports it: granted | denied | undetermined (asked on first recording).
pub fn mic_permission() -> &'static str {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    let Some(cls) = AnyClass::get(c"AVCaptureDevice") else { return "unknown" };
    let audio = objc2_foundation::NSString::from_str("soun"); // AVMediaTypeAudio
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*audio] };
    match status {
        0 => "undetermined",
        1 | 2 => "denied", // restricted or denied
        3 => "granted",
        _ => "unknown",
    }
}

/// Input devices for the microphone picker. The name is what gets stored as the choice.
pub fn input_devices() -> Vec<Mic> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| device_name(&d));
    host.input_devices()
        .map(|it| {
            it.filter_map(|d| device_name(&d))
                .map(|name| Mic { is_default: Some(&name) == default.as_ref(), name })
                .collect()
        })
        .unwrap_or_default()
}

/// Starts capturing from `mic` (by name) or the default input. `on_level` gets an RMS value (0..1) per callback.
/// Reuses the warm stream when it is idle and for the same choice.
// shortcut: with "default" chosen, a warm stream keeps the old default for up to 20 s after the system default
// changes (AirPods connected); compare the resolved device name if that bites.
pub fn start(mic: Option<String>, on_level: impl Fn(f32) + Send + 'static) -> anyhow::Result<Recording> {
    // Test hook: with QUIETKEY_TEST_WAV set (dev builds only), "recording" replays that file instead of the mic,
    // so the whole hotkey -> transcribe -> paste -> save loop can be tested without speaking.
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("QUIETKEY_TEST_WAV") {
        let _ = mic;
        return from_wav(&path, on_level);
    }
    let reusable = |s: &Stream| s.mic == mic && !s.shared.dead.load(Ordering::SeqCst) && s.shared.idle_since.lock().unwrap().is_some();
    {
        let mut warm = WARM.lock().unwrap();
        match warm.as_ref() {
            Some(s) if reusable(s) => return Ok(begin(s.clone(), Box::new(on_level))),
            // Idle but for another mic, or broken: close it.
            Some(s) if s.shared.idle_since.lock().unwrap().is_some() || s.shared.dead.load(Ordering::SeqCst) => {
                let _ = warm.take().map(|s| s.stop_tx.send(()));
            }
            _ => {}
        }
    }
    let stream = Arc::new(open(mic)?); // outside the lock: opening can take seconds
    let rec = begin(stream.clone(), Box::new(on_level));
    let mut warm = WARM.lock().unwrap();
    if warm.is_none() {
        *warm = Some(stream); // otherwise the warm one is busy (mic test during dictation): this one closes on stop
    }
    Ok(rec)
}

fn begin(stream: Arc<Stream>, on_level: Level) -> Recording {
    stream.shared.samples.lock().unwrap().clear();
    *stream.shared.on_level.lock().unwrap() = Some(on_level);
    *stream.shared.idle_since.lock().unwrap() = None;
    Recording { used_fallback: stream.used_fallback, stream }
}

fn open(mic: Option<String>) -> anyhow::Result<Stream> {
    let shared = Arc::new(Shared {
        samples: Mutex::new(Vec::with_capacity(16_000 * 60)),
        on_level: Mutex::new(None),
        idle_since: Mutex::new(None),
        dead: AtomicBool::new(false),
    });
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<(u32, bool)>>();
    let sh = shared.clone();
    let want = mic.clone();

    std::thread::spawn(move || {
        let run = || -> anyhow::Result<(cpal::Stream, u32, bool)> {
            let host = cpal::default_host();
            let chosen = want.as_ref().and_then(|want| {
                host.input_devices().ok()?.find(|d| device_name(d).as_deref() == Some(want.as_str()))
            });
            let used_fallback = want.is_some() && chosen.is_none();
            let device = match chosen {
                Some(d) => d,
                None => host.default_input_device().ok_or_else(|| anyhow::anyhow!("no_mic"))?,
            };
            let config = device.default_input_config()?;
            let channels = config.channels() as usize;
            let rate = config.sample_rate();
            let dead = sh.clone();
            let err_fn = move |e| {
                eprintln!("audio stream error: {e}");
                dead.dead.store(true, Ordering::SeqCst);
            };
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let sh = sh.clone();
                    device.build_input_stream(
                        config.into(),
                        move |data: &[f32], _: &_| push_mono(data.iter().copied(), channels, &sh),
                        err_fn,
                        None,
                    )?
                }
                cpal::SampleFormat::I16 => {
                    let sh = sh.clone();
                    device.build_input_stream(
                        config.into(),
                        move |data: &[i16], _: &_| push_mono(data.iter().map(|s| *s as f32 / i16::MAX as f32), channels, &sh),
                        err_fn,
                        None,
                    )?
                }
                other => anyhow::bail!("unsupported sample format {other:?}"),
            };
            stream.play()?;
            Ok((stream, rate, used_fallback))
        };
        match run() {
            Ok((stream, rate, used_fallback)) => {
                let _ = ready_tx.send(Ok((rate, used_fallback)));
                // Until stop_tx fires, or this stream is the warm one and has been idle for WARM_FOR.
                while let Err(mpsc::RecvTimeoutError::Timeout) = stop_rx.recv_timeout(Duration::from_secs(1)) {
                    let mut warm = WARM.lock().unwrap();
                    let mine = warm.as_ref().is_some_and(|w| Arc::ptr_eq(&w.shared, &sh));
                    if mine && sh.idle_since.lock().unwrap().is_some_and(|t| t.elapsed() >= WARM_FOR) {
                        *warm = None;
                        break;
                    }
                }
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        }
    });

    // Give slow Bluetooth and USB interfaces up to 8 s to open.
    let (rate, used_fallback) = ready_rx
        .recv_timeout(Duration::from_secs(8))
        .map_err(|_| anyhow::anyhow!("mic_timeout"))??;
    Ok(Stream { mic, rate, used_fallback, shared, stop_tx })
}

/// 16 kHz mono samples as a 16-bit WAV file (Groq uploads, saved recordings).
pub fn wav_bytes(samples: &[f32]) -> anyhow::Result<Vec<u8>> {
    let mut wav = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::new(&mut wav, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    w.finalize()?;
    Ok(wav.into_inner())
}

/// A WAV written by `wav_bytes` back as samples.
pub fn wav_samples(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    let mut r = hound::WavReader::new(std::io::Cursor::new(bytes))?;
    anyhow::ensure!(r.spec().sample_rate == 16_000 && r.spec().channels == 1, "not a Quietkey recording");
    Ok(r.samples::<i16>().map(|s| s.map(|v| v as f32 / i16::MAX as f32)).collect::<Result<_, _>>()?)
}

#[cfg(debug_assertions)]
fn from_wav(path: &str, on_level: impl Fn(f32) + Send + 'static) -> anyhow::Result<Recording> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => r.samples::<i32>().map(|s| s.map(|v| v as f32 / (1i64 << (spec.bits_per_sample - 1)) as f32)).collect::<Result<_, _>>()?,
    };
    let mono: Vec<f32> = data.chunks(ch.max(1)).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect();
    on_level(0.1);
    let (stop_tx, _) = mpsc::channel::<()>();
    // QUIETKEY_TEST_LIVE: play the file in at real speed like a microphone, so the live preview runs too.
    if std::env::var_os("QUIETKEY_TEST_LIVE").is_some() {
        let shared = Arc::new(Shared {
            samples: Mutex::new(Vec::new()),
            on_level: Mutex::new(Some(Box::new(on_level))),
            idle_since: Mutex::new(None),
            dead: AtomicBool::new(false),
        });
        let (sh, step) = (shared.clone(), spec.sample_rate as usize / 10);
        std::thread::spawn(move || {
            for chunk in mono.chunks(step) {
                std::thread::sleep(Duration::from_millis(100));
                if sh.on_level.lock().unwrap().is_none() {
                    return; // released
                }
                sh.samples.lock().unwrap().extend_from_slice(chunk);
            }
        });
        let stream = Arc::new(Stream { mic: None, rate: spec.sample_rate, used_fallback: false, shared, stop_tx });
        return Ok(Recording { stream, used_fallback: false });
    }
    let shared = Arc::new(Shared {
        samples: Mutex::new(mono),
        on_level: Mutex::new(None),
        idle_since: Mutex::new(None),
        dead: AtomicBool::new(false),
    });
    let stream = Arc::new(Stream { mic: None, rate: spec.sample_rate, used_fallback: false, shared, stop_tx });
    Ok(Recording { stream, used_fallback: false })
}

impl Recording {
    /// Ends the recording and returns 16 kHz mono samples plus the recorded duration in ms. The warm stream stays
    /// open for WARM_FOR; any other stream closes now.
    pub fn stop(self) -> (Vec<f32>, i64) {
        let warm = WARM.lock().unwrap();
        let sh = &self.stream.shared;
        *sh.on_level.lock().unwrap() = None;
        let raw = std::mem::take(&mut *sh.samples.lock().unwrap());
        *sh.idle_since.lock().unwrap() = Some(Instant::now());
        if !warm.as_ref().is_some_and(|w| Arc::ptr_eq(w, &self.stream)) {
            let _ = self.stream.stop_tx.send(());
        }
        drop(warm);
        let rate = self.stream.rate;
        let duration_ms = (raw.len() as i64 * 1000) / rate.max(1) as i64;
        (resample(&raw, rate, WHISPER_RATE), duration_ms)
    }
}

/// A read-only view of a running recording, for the live preview.
pub struct Tap(Arc<Stream>);

impl Recording {
    pub fn tap(&self) -> Tap {
        Tap(self.stream.clone())
    }
}

impl Tap {
    /// The last `secs` seconds at 16 kHz, or None once the recording has stopped.
    pub fn last(&self, secs: u32) -> Option<Vec<f32>> {
        let sh = &self.0.shared;
        if sh.on_level.lock().unwrap().is_none() {
            return None;
        }
        let s = sh.samples.lock().unwrap();
        let tail = &s[s.len().saturating_sub((secs * self.0.rate) as usize)..];
        Some(resample(tail, self.0.rate, WHISPER_RATE))
    }
}

fn push_mono(data: impl Iterator<Item = f32>, channels: usize, sh: &Shared) {
    let level = sh.on_level.lock().unwrap();
    let Some(on_level) = level.as_ref() else { return }; // idle warm stream: drop the audio
    let data: Vec<f32> = data.collect();
    let mono: Vec<f32> = data
        .chunks(channels.max(1))
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
    on_level(rms.min(1.0));
    sh.samples.lock().unwrap().extend_from_slice(&mono);
}
/// True if any 20 ms window of the 16 kHz clip is louder than faint room noise. Whisper invents text for
/// silence ("I'm a fan of the world's"), so silent clips must never reach it.
/// TODO: a real VAD (whisper.cpp ships Silero) would also catch steady background noise.
pub fn has_speech(samples: &[f32]) -> bool {
    const WINDOW: usize = 320; // 20 ms at 16 kHz
    const THRESHOLD: f32 = 0.006; // about -44 dBFS; quiet speech into a laptop mic is well above this
    samples
        .chunks(WINDOW)
        .any(|w| (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt() > THRESHOLD)
}

/// Box-filter resampler: each output sample averages the input span it covers (acts as a crude low-pass
/// when downsampling, linear hold when upsampling).
/// TODO: no proper anti-alias filter; fine for speech into Whisper, swap for a sinc resampler if accuracy suffers.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let start = (i as f64 * ratio) as usize;
            let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, input.len());
            input[start..end].iter().sum::<f32>() / (end - start) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{resample, wav_bytes, wav_samples};

    #[test]
    fn wav_round_trip() {
        let x = vec![0.0, 0.5, -0.5, 0.25];
        let back = wav_samples(&wav_bytes(&x).unwrap()).unwrap();
        assert!(x.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1e-3));
    }

    #[test]
    fn resample_lengths_and_values() {
        let one_sec_48k = vec![0.5f32; 48_000];
        let out = resample(&one_sec_48k, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        assert_eq!(resample(&vec![0.0; 44_100], 44_100, 16_000).len(), 16_000);
        assert_eq!(resample(&[1.0, 2.0], 16_000, 16_000), vec![1.0, 2.0]);
        assert!(resample(&[], 48_000, 16_000).is_empty());
        // averaging: [0,3,6] -> 3
        assert_eq!(resample(&[0.0, 3.0, 6.0], 48_000, 16_000), vec![3.0]);
    }
}

#[cfg(test)]
mod speech_gate_tests {
    use super::has_speech;

    #[test]
    fn silence_and_room_noise_are_not_speech() {
        assert!(!has_speech(&vec![0.0; 32_000]));
        // faint room noise, ~-60 dBFS
        let noise: Vec<f32> = (0..32_000).map(|i| ((i * 7919 % 101) as f32 / 101.0 - 0.5) * 0.002).collect();
        assert!(!has_speech(&noise));
        assert!(!has_speech(&[]));
    }

    #[test]
    fn quiet_speech_counts() {
        // 300 ms of a 200 Hz tone at amplitude 0.03 (quiet talking) inside 2 s of silence
        let mut a = vec![0.0f32; 32_000];
        for i in 8_000..12_800 {
            a[i] = (i as f32 * 2.0 * std::f32::consts::PI * 200.0 / 16_000.0).sin() * 0.03;
        }
        assert!(has_speech(&a));
    }
}

#[cfg(test)]
mod device_tests {
    /// Lists real input devices. Run: cargo test --lib list_inputs -- --ignored --nocapture
    #[test]
    #[ignore]
    fn list_inputs() {
        for m in super::input_devices() {
            println!("{}{}", m.name, if m.is_default { "  (default)" } else { "" });
        }
    }

    /// F2 on a real mic: the second recording reuses the warm stream. Run: cargo test --lib warm_mic -- --ignored --nocapture
    #[test]
    #[ignore]
    fn warm_mic_restarts_instantly() {
        use std::time::{Duration, Instant};
        let t = Instant::now();
        let rec = super::start(None, |_| {}).unwrap();
        let cold = t.elapsed();
        std::thread::sleep(Duration::from_millis(400));
        let (first, _) = rec.stop();
        let t = Instant::now();
        let rec = super::start(None, |_| {}).unwrap();
        let warm = t.elapsed();
        std::thread::sleep(Duration::from_millis(400));
        let (second, _) = rec.stop();
        println!("cold open {cold:?}, warm open {warm:?}, samples {} then {}", first.len(), second.len());
        assert!(warm < Duration::from_millis(20), "warm start took {warm:?}");
        assert!(second.len() > 16_000 * 3 / 10, "warm stream recorded {} samples", second.len());
    }
}
