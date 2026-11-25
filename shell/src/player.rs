//! Lettore audio/video su GStreamer (playbin). I fotogrammi video escono da
//! un appsink in RGBA e arrivano alla UI tramite la callback `on_frame`.
//! Sul Lumia non c'è decodifica hardware in mainline: meglio restare sul 480p.

use crate::media;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::path::Path;

pub enum Event {
    None,
    Eos,
    Error(String),
}

pub struct Player {
    playbin: gst::Element,
    _sink: gst_app::AppSink,
    playing: bool,
}

impl Player {
    pub fn new<F>(max_width: u32, on_frame: F) -> Result<Self, String>
    where
        F: Fn(u32, u32, Vec<u8>) + Send + Sync + 'static,
    {
        if !media::gst_ready() {
            return Err("GStreamer non disponibile".into());
        }
        let playbin = gst::ElementFactory::make("playbin").build().map_err(|e| e.to_string())?;
        let (bin, sink) = media::video_branch(None)?;
        // Il limite di larghezza lo mette videoscale tramite le caps dell'appsink
        sink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                .field("width", gst::IntRange::new(16, max_width as i32))
                .build(),
        ));
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |s| {
                    let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if let Some((w, h, data)) = media::sample_rgba(&sample) {
                        on_frame(w, h, data);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        playbin.set_property("video-sink", &bin);
        if let Ok(name) = std::env::var("CHEIP_AUDIO_SINK") {
            if let Ok(a) = gst::ElementFactory::make(&name).build() {
                playbin.set_property("audio-sink", &a);
            }
        }
        Ok(Player { playbin, _sink: sink, playing: false })
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let uri = media::file_uri(path).ok_or("percorso non valido")?;
        let _ = self.playbin.set_state(gst::State::Null);
        self.playbin.set_property("uri", &uri);
        self.playbin.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        self.playing = true;
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn set_playing(&mut self, play: bool) {
        let _ = self.playbin.set_state(if play { gst::State::Playing } else { gst::State::Paused });
        self.playing = play;
    }

    pub fn stop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
        self.playing = false;
    }

    pub fn position(&self) -> u64 {
        self.playbin.query_position::<gst::ClockTime>().map(|t| t.seconds()).unwrap_or(0)
    }

    pub fn duration(&self) -> u64 {
        self.playbin.query_duration::<gst::ClockTime>().map(|t| t.seconds()).unwrap_or(0)
    }

    pub fn seek_fraction(&self, f: f32) {
        let d = self.playbin.query_duration::<gst::ClockTime>().unwrap_or(gst::ClockTime::ZERO);
        let t = gst::ClockTime::from_nseconds((d.nseconds() as f64 * f.clamp(0.0, 1.0) as f64) as u64);
        let _ = self.playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, t);
    }

    /// Da chiamare ogni tanto dal thread della UI
    pub fn poll(&self) -> Event {
        let Some(bus) = self.playbin.bus() else { return Event::None };
        while let Some(msg) = bus.pop() {
            match msg.view() {
                gst::MessageView::Eos(_) => return Event::Eos,
                gst::MessageView::Error(e) => return Event::Error(e.error().to_string()),
                _ => {}
            }
        }
        Event::None
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn plays_video_frames_and_reaches_eos() {
        let dir = std::env::temp_dir().join(format!("cheip-player-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("clip.mp4");
        let ok = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=1", "-c:v", "mpeg4"])
            .arg(&src)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok || !media::gst_ready() {
            return;
        }
        std::env::set_var("CHEIP_AUDIO_SINK", "fakesink");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut p = Player::new(480, move |w, h, data| {
            let _ = tx.lock().unwrap().send((w, h, data.len()));
        })
        .unwrap();
        p.load(&src).unwrap();
        let (w, h, len) = rx.recv_timeout(Duration::from_secs(10)).expect("nessun fotogramma");
        assert_eq!((w, h), (480, 270), "ridimensionato a 480 mantenendo le proporzioni");
        assert_eq!(len, 480 * 270 * 4);
        let mut eos = false;
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(100));
            if let Event::Eos = p.poll() {
                eos = true;
                break;
            }
        }
        assert!(eos, "il video da 1 s deve finire");
    }
}
