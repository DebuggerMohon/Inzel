//! Microphone capture (WASAPI through cpal) with a live level meter, silence auto-stop and segment splitting.
//! Output audio is always 16 kHz mono int16, exactly like the Python version.
use crate::events::{emit, Event};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::sync::{mpsc, Arc, Mutex};

pub const SAMPLE_RATE: u32 = 16000;
pub const FRAME_MS: usize = 30;
pub const FRAME: usize = 480; // samples per 30 ms block

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut out: Vec<String> = Vec::new();
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            if let Ok(n) = d.name() {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    out
}

#[derive(Clone)]
pub struct RecOptions {
    pub device: Option<String>,
    pub auto_stop: bool,
    pub silence_s: f32,
}

pub type AutoStopFn = Arc<dyn Fn(&'static str) + Send + Sync>;

struct Proc {
    channels: usize,
    ratio: f64,
    auto_stop: bool,
    silence_s: f32,
    on_auto_stop: AutoStopFn,
    // channel -> mono
    ch_i: usize,
    ch_acc: f32,
    // box-filter resampler
    acc: f32,
    cnt: u32,
    in_pos: f64,
    next_out: f64,
    // frames
    buf: Vec<i16>,
    frames: Vec<i16>,
    n: usize,
    floor: f32,
    speech_frames: usize,
    last_speech: usize,
    fired: bool,
}

impl Proc {
    fn new(channels: usize, in_rate: u32, o: &RecOptions, on_auto_stop: AutoStopFn) -> Proc {
        let ratio = in_rate as f64 / SAMPLE_RATE as f64;
        Proc {
            channels: channels.max(1),
            ratio,
            auto_stop: o.auto_stop,
            silence_s: o.silence_s,
            on_auto_stop,
            ch_i: 0,
            ch_acc: 0.0,
            acc: 0.0,
            cnt: 0,
            in_pos: 0.0,
            next_out: ratio,
            buf: Vec::with_capacity(FRAME),
            frames: Vec::new(),
            n: 0,
            floor: 1000.0,
            speech_frames: 0,
            last_speech: 0,
            fired: false,
        }
    }

    fn feed<I: Iterator<Item = f32>>(&mut self, samples: I) {
        for s in samples {
            self.ch_acc += s;
            self.ch_i += 1;
            if self.ch_i == self.channels {
                let mono = self.ch_acc / self.channels as f32 * 32767.0;
                self.ch_acc = 0.0;
                self.ch_i = 0;
                self.mono(mono);
            }
        }
    }

    fn mono(&mut self, m: f32) {
        self.acc += m;
        self.cnt += 1;
        self.in_pos += 1.0;
        if self.in_pos >= self.next_out {
            let v = self.acc / self.cnt as f32;
            self.acc = 0.0;
            self.cnt = 0;
            self.next_out += self.ratio;
            self.buf.push(v.clamp(-32768.0, 32767.0) as i16);
            if self.buf.len() == FRAME {
                self.frame_done();
            }
        }
    }

    fn frame_done(&mut self) {
        let sum: f64 = self.buf.iter().map(|&x| (x as f64) * (x as f64)).sum();
        let rms = (sum / FRAME as f64).sqrt() as f32;
        self.frames.extend_from_slice(&self.buf);
        self.buf.clear();
        self.n += 1;
        self.floor = rms.min(self.floor * 1.005);
        let threshold = (self.floor * 3.0).max(200.0);
        if rms > threshold {
            self.speech_frames += 1;
            self.last_speech = self.n;
        }
        emit(Event::Level(((rms / 3000.0).sqrt()).min(1.0)));

        if self.fired {
            return;
        }
        let elapsed = self.n as f32 * FRAME_MS as f32 / 1000.0;
        let has_speech = self.speech_frames >= 3;
        let since = (self.n - self.last_speech) as f32 * FRAME_MS as f32 / 1000.0;
        let reason: Option<&'static str> = if self.auto_stop && has_speech && since >= self.silence_s {
            Some("silence")
        } else if self.auto_stop && !has_speech && elapsed >= 10.0 {
            Some("no_speech")
        } else if elapsed >= 180.0 {
            Some("max")
        } else {
            None
        };
        if let Some(r) = reason {
            self.fired = true;
            (self.on_auto_stop)(r);
        }
    }
}

fn make_stream<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    proc: Arc<Mutex<Proc>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            cfg,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if let Ok(mut p) = proc.lock() {
                    p.feed(data.iter().map(|s| f32::from_sample(*s)));
                }
            },
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

fn open(o: &RecOptions, on_auto_stop: AutoStopFn) -> Result<(cpal::Stream, Arc<Mutex<Proc>>), String> {
    let host = cpal::default_host();
    let mut device: Option<cpal::Device> = None;
    if let Some(name) = &o.device {
        if let Ok(mut it) = host.input_devices() {
            device = it.find(|d| d.name().map(|n| &n == name).unwrap_or(false));
        }
    }
    let device = match device {
        Some(d) => d,
        None => host.default_input_device().ok_or_else(|| "no input device".to_string())?,
    };
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let proc = Arc::new(Mutex::new(Proc::new(config.channels as usize, config.sample_rate.0, o, on_auto_stop)));
    let stream = match format {
        cpal::SampleFormat::F32 => make_stream::<f32>(&device, &config, proc.clone())?,
        cpal::SampleFormat::I16 => make_stream::<i16>(&device, &config, proc.clone())?,
        cpal::SampleFormat::U16 => make_stream::<u16>(&device, &config, proc.clone())?,
        cpal::SampleFormat::I32 => make_stream::<i32>(&device, &config, proc.clone())?,
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, proc))
}

/// A running recording. The cpal stream lives on its own thread (streams are not Send).
pub struct Recorder {
    stop_tx: mpsc::Sender<()>,
    done_rx: mpsc::Receiver<Vec<i16>>,
}

impl Recorder {
    pub fn start(opts: RecOptions, on_auto_stop: AutoStopFn) -> Result<Recorder, String> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<Vec<i16>>();
        let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();
        std::thread::spawn(move || match open(&opts, on_auto_stop) {
            Ok((stream, proc)) => {
                let _ = init_tx.send(Ok(()));
                let _ = stop_rx.recv();
                drop(stream);
                let frames = match proc.lock() {
                    Ok(mut p) => std::mem::take(&mut p.frames),
                    Err(_) => Vec::new(),
                };
                let _ = done_tx.send(frames);
            }
            Err(e) => {
                let _ = init_tx.send(Err(e));
            }
        });
        match init_rx.recv() {
            Ok(Ok(())) => Ok(Recorder { stop_tx, done_rx }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("audio thread failed".to_string()),
        }
    }

    /// Stops the stream and returns all audio (16 kHz mono int16).
    pub fn stop(self) -> Vec<i16> {
        let _ = self.stop_tx.send(());
        self.done_rx.recv().unwrap_or_default()
    }
}

/// Cut long audio at pauses so each piece is short enough for Google's free endpoint.
pub fn split_segments(pcm: &[i16]) -> Vec<Vec<i16>> {
    let (soft_s, max_s, min_pause_s) = (12.0f32, 25.0f32, 0.5f32);
    let n = pcm.len() / FRAME;
    if n == 0 {
        return Vec::new();
    }
    let rms: Vec<f32> = (0..n)
        .map(|i| {
            let f = &pcm[i * FRAME..(i + 1) * FRAME];
            let s: f64 = f.iter().map(|&x| (x as f64) * (x as f64)).sum();
            (s / FRAME as f64).sqrt() as f32
        })
        .collect();
    // 10th percentile with linear interpolation (like numpy)
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = 0.1 * (n as f32 - 1.0);
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let p10 = sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f32);
    let thr = (p10 * 3.0).max(200.0).min(1000.0);
    let quiet: Vec<bool> = rms.iter().map(|&r| r < thr).collect();

    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let (mut start, mut q) = (0usize, 0usize);
    for i in 0..n {
        q = if quiet[i] { q + 1 } else { 0 };
        let length = (i - start + 1) as f32 * FRAME_MS as f32 / 1000.0;
        let pause = q as f32 * FRAME_MS as f32 / 1000.0;
        if (length >= soft_s && pause >= min_pause_s) || length >= max_s {
            cuts.push((start, i + 1));
            start = i + 1;
            q = 0;
        }
    }
    if start < n {
        cuts.push((start, n));
    }
    let mut out = Vec::new();
    for (a, b) in cuts {
        let voiced = quiet[a..b].iter().filter(|x| !**x).count();
        if voiced >= 3 {
            out.push(pcm[a * FRAME..b * FRAME].to_vec());
        }
    }
    out
}
