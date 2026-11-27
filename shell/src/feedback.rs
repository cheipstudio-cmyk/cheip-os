//! Suoni e vibrazione. Le suonerie sono sintetizzate (niente file audio
//! di terzi): sul telefono diventano WAV in cache suonati con GStreamer,
//! nella demo web passano da Web Audio. La vibrazione usa il motore del
//! telefono (force feedback evdev) o navigator.vibrate nel browser.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Notification,
    Message,
    Ring(i32),
    /// Notification tone picked in Settings (0 Pop, 1 Chime, 2 Drop)
    Tone(i32),
    /// Charger plugged in
    Charge,
    /// Battery under 15%
    LowBattery,
    /// Screen turned off / on with the power key
    Lock,
    Unlock,
}

/// (frequenza Hz, inizio ms, durata ms, volume 0..1)
pub fn notes(s: Sound) -> Vec<(f32, u32, u32, f32)> {
    match s {
        Sound::Notification => vec![(988.0, 0, 140, 0.5), (1319.0, 120, 320, 0.45)],
        Sound::Message => vec![(880.0, 0, 110, 0.45), (1175.0, 95, 110, 0.45), (1568.0, 190, 300, 0.4)],
        Sound::Tone(1) => vec![(784.0, 0, 260, 0.4), (1046.5, 140, 260, 0.38), (1568.0, 280, 520, 0.34)],
        Sound::Tone(2) => vec![(1760.0, 0, 70, 0.4), (1318.5, 60, 90, 0.42), (880.0, 130, 380, 0.45)],
        Sound::Tone(_) => vec![(988.0, 0, 140, 0.5), (1319.0, 120, 320, 0.45)],
        // Rising two-note "plugged in"
        Sound::Charge => vec![(659.25, 0, 120, 0.4), (987.77, 110, 260, 0.42)],
        // Falling, a bit insistent
        Sound::LowBattery => vec![(880.0, 0, 160, 0.45), (659.25, 170, 160, 0.45), (523.25, 340, 380, 0.45)],
        Sound::Lock => vec![(523.25, 0, 70, 0.3)],
        Sound::Unlock => vec![(783.99, 0, 70, 0.3)],
        Sound::Ring(0) => {
            // Lakeside: arpeggio morbido
            let mut v = vec![];
            for (k, f) in [523.25, 659.25, 783.99, 1046.5, 783.99, 659.25].iter().enumerate() {
                v.push((*f, k as u32 * 170, 420, 0.42));
            }
            v
        }
        Sound::Ring(1) => {
            // Overtake: due colpi decisi ripetuti
            let mut v = vec![];
            for k in 0..4 {
                v.push((1174.66, k * 380, 150, 0.5));
                v.push((1567.98, k * 380 + 160, 180, 0.5));
            }
            v
        }
        Sound::Ring(_) => {
            // Lumen: campana a tre note
            vec![(659.25, 0, 900, 0.45), (987.77, 260, 900, 0.4), (1318.51, 520, 1100, 0.35)]
        }
    }
}

/// Lunghezza del ciclo di squillo in ms
pub const RING_CYCLE_MS: u64 = 2600;

/// Campioni PCM mono 16 bit: seno con un'armonica e inviluppo esponenziale
pub fn render(s: Sound, rate: u32) -> Vec<i16> {
    let ns = notes(s);
    let total_ms = ns.iter().map(|(_, t, d, _)| t + d).max().unwrap_or(0) + 40;
    let len = (total_ms as u64 * rate as u64 / 1000) as usize;
    let mut buf = vec![0f32; len];
    for (f, t, d, g) in ns {
        let start = (t as u64 * rate as u64 / 1000) as usize;
        let n = (d as u64 * rate as u64 / 1000) as usize;
        for i in 0..n {
            let idx = start + i;
            if idx >= len {
                break;
            }
            let time = i as f32 / rate as f32;
            let attack = (i as f32 / (rate as f32 * 0.006)).min(1.0);
            let env = attack * (-time * 6.0 / (d as f32 / 1000.0).max(0.05)).exp();
            let w = 2.0 * std::f32::consts::PI * f * time;
            buf[idx] += g * env * (w.sin() + 0.25 * (2.0 * w).sin());
        }
    }
    let peak = buf.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-6);
    buf.iter().map(|v| (v / peak * 0.85 * i16::MAX as f32) as i16).collect()
}

pub fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for s in samples {
        out.extend(s.to_le_bytes());
    }
    out
}

// ---------- telefono ----------

#[cfg(feature = "native")]
mod imp {
    use super::*;
    use gstreamer as gst;
    use gstreamer::prelude::*;

    fn file(s: Sound) -> Option<std::path::PathBuf> {
        let name = match s {
            Sound::Notification => "notification".to_string(),
            Sound::Message => "message".to_string(),
            Sound::Ring(i) => format!("ring{i}"),
            Sound::Tone(i) => format!("tone{i}"),
            Sound::Charge => "charge".to_string(),
            Sound::LowBattery => "lowbattery".to_string(),
            Sound::Lock => "lock".to_string(),
            Sound::Unlock => "unlock".to_string(),
        };
        let p = crate::media::cache_dir().with_file_name("sounds").join(format!("{name}.wav"));
        if !p.exists() {
            std::fs::create_dir_all(p.parent()?).ok()?;
            std::fs::write(&p, wav(&render(s, 22050), 22050)).ok()?;
        }
        Some(p)
    }

    pub fn play(s: Sound, volume: f32) {
        let Some(path) = file(s) else { return };
        if !crate::media::gst_ready() {
            return;
        }
        std::thread::spawn(move || {
            let Some(uri) = crate::media::file_uri(&path) else { return };
            let Ok(pb) = gst::ElementFactory::make("playbin").property("uri", &uri).build() else { return };
            pb.set_property("volume", volume.clamp(0.0, 1.0) as f64);
            if let Ok(name) = std::env::var("CHEIP_AUDIO_SINK") {
                if let Ok(a) = gst::ElementFactory::make(&name).build() {
                    pb.set_property("audio-sink", &a);
                }
            }
            if pb.set_state(gst::State::Playing).is_err() {
                return;
            }
            if let Some(bus) = pb.bus() {
                let _ = bus.timed_pop_filtered(gst::ClockTime::from_seconds(6), &[gst::MessageType::Eos, gst::MessageType::Error]);
            }
            let _ = pb.set_state(gst::State::Null);
        });
    }

    /// Motore di vibrazione del telefono: il primo dispositivo con FF_RUMBLE
    pub fn vibrate(ms: u16) {
        std::thread::spawn(move || {
            for mut dev in crate::evdev_lite::enumerate() {
                if dev.has_rumble() && dev.rumble(ms) {
                    return;
                }
            }
        });
    }
}

// ---------- browser ----------

#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use web_sys::{AudioContext, OscillatorType};

    thread_local! {
        static CTX: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
    }

    pub fn play(s: Sound, volume: f32) {
        CTX.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = AudioContext::new().ok();
            }
            let Some(ctx) = c.as_ref() else { return };
            let _ = ctx.resume();
            let now = ctx.current_time();
            for (f, t, d, g) in notes(s) {
                let (Ok(osc), Ok(gain)) = (ctx.create_oscillator(), ctx.create_gain()) else { continue };
                osc.set_type(OscillatorType::Sine);
                osc.frequency().set_value(f);
                let start = now + t as f64 / 1000.0;
                let end = start + d as f64 / 1000.0;
                let p = gain.gain();
                let peak = (g * volume.clamp(0.0, 1.0) * 0.6).max(0.0001);
                let _ = p.set_value_at_time(0.0001, start);
                let _ = p.exponential_ramp_to_value_at_time(peak, start + 0.008);
                let _ = p.exponential_ramp_to_value_at_time(0.0001, end);
                let _ = osc.connect_with_audio_node(&gain);
                let _ = gain.connect_with_audio_node(&ctx.destination());
                let _ = osc.start_with_when(start);
                let _ = osc.stop_with_when(end + 0.05);
            }
        });
    }

    pub fn vibrate(ms: u16) {
        if let Some(w) = web_sys::window() {
            let _ = w.navigator().vibrate_with_duration(ms as u32);
        }
    }
}

#[cfg(not(any(feature = "native", all(target_arch = "wasm32", feature = "web"))))]
mod imp {
    use super::Sound;
    pub fn play(_s: Sound, _volume: f32) {}
    pub fn vibrate(_ms: u16) {}
}

pub use imp::{play, vibrate};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_and_wav() {
        let pcm = render(Sound::Message, 22050);
        assert!(pcm.len() > 22050 * 4 / 10, "almeno 0,4 s");
        let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!(peak > 20000 && peak < 32767, "normalizzato senza saturare");
        let w = wav(&pcm, 22050);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + pcm.len() * 2);
        for i in 0..3 {
            let ring = render(Sound::Ring(i), 22050);
            assert!((ring.len() as u64 * 1000 / 22050) < RING_CYCLE_MS, "lo squillo sta nel ciclo");
        }
    }
}
