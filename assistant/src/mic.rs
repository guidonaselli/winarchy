use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 16_000;

pub struct Recording {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    in_rate: u32,
}

/// Empieza a grabar el micrófono por defecto; `stop` devuelve mono a 16 kHz.
pub fn start() -> Result<Recording, String> {
    let host = cpal::default_host();
    let want = std::env::var("WINARCHY_MIC").unwrap_or_default().to_lowercase();
    let picked = host.input_devices().ok().and_then(|mut d| d.find(|x| !want.is_empty() && x.name().is_ok_and(|n| n.to_lowercase().contains(&want))));
    let dev = picked.or_else(|| host.default_input_device()).ok_or("no input device")?;
    eprintln!("mic: {}", dev.name().unwrap_or_default());
    let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
    let (ch, in_rate) = (cfg.channels() as usize, cfg.sample_rate().0);
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = buf.clone();
    let err = |e| eprintln!("mic: {e}");
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => dev.build_input_stream(&cfg.into(), move |d: &[f32], _| push(&sink, d, ch), err, None),
        cpal::SampleFormat::I16 => dev.build_input_stream(&cfg.into(), move |d: &[i16], _| {
            let f: Vec<f32> = d.iter().map(|s| *s as f32 / 32768.0).collect();
            push(&sink, &f, ch)
        }, err, None),
        f => return Err(format!("unsupported sample format {f:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recording { stream, buf, in_rate })
}

fn push(sink: &Mutex<Vec<f32>>, d: &[f32], ch: usize) {
    let mut b = sink.lock().unwrap();
    b.extend(d.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
}

impl Recording {
    pub fn stop(self) -> Vec<f32> {
        drop(self.stream);
        let raw = std::mem::take(&mut *self.buf.lock().unwrap());
        resample(&raw, self.in_rate)
    }
}

fn resample(x: &[f32], from: u32) -> Vec<f32> {
    if from == RATE {
        return x.to_vec();
    }
    let step = from as f64 / RATE as f64;
    (0..(x.len() as f64 / step) as usize)
        .map(|i| {
            let p = i as f64 * step;
            let (a, b) = (x[p as usize], x[(p as usize + 1).min(x.len() - 1)]);
            a + (b - a) * (p.fract() as f32)
        })
        .collect()
}

/// Banco de pruebas: nivel (rms) de cada entrada durante `secs` segundos, todas a la vez.
pub fn levels(secs: u64) {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok()).unwrap_or_default();
    let mut live = Vec::new();
    for dev in host.input_devices().into_iter().flatten() {
        let name = dev.name().unwrap_or_default();
        let Ok(cfg) = dev.default_input_config() else { continue };
        let acc = Arc::new(Mutex::new((0f64, 0u64)));
        let sink = acc.clone();
        let ch = cfg.channels() as usize;
        let s = match cfg.sample_format() {
            cpal::SampleFormat::F32 => dev.build_input_stream(&cfg.into(), move |d: &[f32], _| tally(&sink, d, ch), |_| {}, None),
            cpal::SampleFormat::I16 => dev.build_input_stream(&cfg.into(), move |d: &[i16], _| {
                let f: Vec<f32> = d.iter().map(|s| *s as f32 / 32768.0).collect();
                tally(&sink, &f, ch)
            }, |_| {}, None),
            _ => continue,
        };
        if let Ok(s) = s {
            if s.play().is_ok() {
                live.push((name, s, acc));
            }
        }
    }
    println!("hablá ahora ({secs}s)...");
    std::thread::sleep(std::time::Duration::from_secs(secs));
    for (name, _s, acc) in &live {
        let (sum, n) = *acc.lock().unwrap();
        let rms = (sum / n.max(1) as f64).sqrt();
        println!("{} rms {rms:.4} ({n} muestras){}", name, if *name == default { "  <- predeterminado" } else { "" });
    }
}

fn tally(sink: &Mutex<(f64, u64)>, d: &[f32], ch: usize) {
    let mut a = sink.lock().unwrap();
    for f in d.chunks(ch) {
        let m = f.iter().sum::<f32>() / ch as f32;
        a.0 += (m * m) as f64;
        a.1 += 1;
    }
}

/// Silencio = ninguna ventana de 100 ms supera el umbral de voz (el promedio global diluye frases cortas).
pub fn is_silent(pcm: &[f32]) -> bool {
    !pcm.chunks(RATE as usize / 10).any(|w| (w.iter().map(|x| x * x).sum::<f32>() / w.len() as f32).sqrt() > 0.01)
}

pub fn write_wav(path: &std::path::Path, pcm: &[f32]) -> std::io::Result<()> {
    let data: Vec<u8> = pcm.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut h = Vec::with_capacity(44 + data.len());
    h.extend(b"RIFF");
    h.extend((36 + data.len() as u32).to_le_bytes());
    h.extend(b"WAVEfmt ");
    h.extend(16u32.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(RATE.to_le_bytes());
    h.extend((RATE * 2).to_le_bytes());
    h.extend(2u16.to_le_bytes());
    h.extend(16u16.to_le_bytes());
    h.extend(b"data");
    h.extend((data.len() as u32).to_le_bytes());
    h.extend(data);
    std::fs::write(path, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_48k_to_16k_keeps_duration() {
        let x = vec![0.5f32; 48_000];
        assert_eq!(resample(&x, 48_000).len(), 16_000);
    }

    #[test]
    fn short_speech_in_long_silence_is_not_silent() {
        let mut x = vec![0.0f32; 96_000];
        x[40_000..44_800].fill(0.05);
        assert!(!is_silent(&x));
        assert!(is_silent(&vec![0.002f32; 96_000]));
    }

    #[test]
    fn wav_header_is_44_bytes_plus_data() {
        let p = std::env::temp_dir().join("winarchy-mic-test.wav");
        write_wav(&p, &[0.0; 100]).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 44 + 200);
        std::fs::remove_file(p).ok();
    }
}
