use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub const WHISPER_RATE: u32 = 16_000;

/// A running capture. cpal streams are not Send on macOS, so the stream lives on its own thread.
pub struct Recording {
    stop_tx: mpsc::Sender<()>,
    handle: JoinHandle<()>,
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

/// Starts capturing from the default input device. `on_level` gets an RMS value (0..1) per audio callback.
pub fn start(on_level: impl Fn(f32) + Send + 'static) -> anyhow::Result<Recording> {
    // Test hook: with UTTER_TEST_WAV set (dev builds only), "recording" replays that file instead of the mic,
    // so the whole hotkey -> transcribe -> paste -> save loop can be tested without speaking.
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("UTTER_TEST_WAV") {
        return from_wav(&path, on_level);
    }
    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(16_000 * 60)));
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<u32>>();
    let buf = samples.clone();

    let handle = std::thread::spawn(move || {
        let run = || -> anyhow::Result<(cpal::Stream, u32)> {
            let device = cpal::default_host()
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("no_mic"))?;
            let config = device.default_input_config()?;
            let channels = config.channels() as usize;
            let rate = config.sample_rate();
            let err_fn = |e| eprintln!("audio stream error: {e}");
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let buf = buf.clone();
                    device.build_input_stream(
                        config.into(),
                        move |data: &[f32], _: &_| push_mono(data.iter().copied(), channels, &buf, &on_level),
                        err_fn,
                        None,
                    )?
                }
                cpal::SampleFormat::I16 => {
                    let buf = buf.clone();
                    device.build_input_stream(
                        config.into(),
                        move |data: &[i16], _: &_| {
                            push_mono(data.iter().map(|s| *s as f32 / i16::MAX as f32), channels, &buf, &on_level)
                        },
                        err_fn,
                        None,
                    )?
                }
                other => anyhow::bail!("unsupported sample format {other:?}"),
            };
            stream.play()?;
            Ok((stream, rate))
        };
        match run() {
            Ok((stream, rate)) => {
                let _ = ready_tx.send(Ok(rate));
                let _ = stop_rx.recv(); // block until stop() or the Recording is dropped
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        }
    });

    // Give slow Bluetooth and USB interfaces up to 8 s to open.
    let rate = ready_rx
        .recv_timeout(std::time::Duration::from_secs(8))
        .map_err(|_| anyhow::anyhow!("mic_timeout"))??;
    Ok(Recording { stop_tx, handle, samples, rate })
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
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let handle = std::thread::spawn(move || {
        let _ = stop_rx.recv();
    });
    Ok(Recording { stop_tx, handle, samples: Arc::new(Mutex::new(mono)), rate: spec.sample_rate })
}

impl Recording {
    /// Stops the stream and returns 16 kHz mono samples plus the recorded duration in ms.
    pub fn stop(self) -> (Vec<f32>, i64) {
        let _ = self.stop_tx.send(());
        let _ = self.handle.join();
        let raw = std::mem::take(&mut *self.samples.lock().unwrap());
        let duration_ms = (raw.len() as i64 * 1000) / self.rate.max(1) as i64;
        (resample(&raw, self.rate, WHISPER_RATE), duration_ms)
    }
}

fn push_mono(data: impl Iterator<Item = f32>, channels: usize, buf: &Mutex<Vec<f32>>, on_level: &dyn Fn(f32)) {
    let data: Vec<f32> = data.collect();
    let mono: Vec<f32> = data
        .chunks(channels.max(1))
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
    on_level(rms.min(1.0));
    buf.lock().unwrap().extend_from_slice(&mono);
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
    use super::resample;

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
