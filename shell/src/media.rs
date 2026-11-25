//! Media per l'app Files: indice per categorie, miniature, metadati audio,
//! PDF (MuPDF) e documenti DOCX/ODT. Tutto quello che è lento gira fuori
//! dal thread della UI e lascia file in cache.

use crate::files::{self, Entry, Kind};
#[cfg(feature = "native")]
use gstreamer as gst;
#[cfg(feature = "native")]
use gstreamer::prelude::*;
#[cfg(feature = "native")]
use gstreamer_app as gst_app;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Downloads,
    Images,
    Videos,
    Audio,
    Documents,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Downloads, Category::Images, Category::Videos, Category::Audio, Category::Documents];

    pub fn id(self) -> &'static str {
        match self {
            Category::Downloads => "downloads",
            Category::Images => "images",
            Category::Videos => "videos",
            Category::Audio => "audio",
            Category::Documents => "documents",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Downloads => "Downloads",
            Category::Images => "Images",
            Category::Videos => "Videos",
            Category::Audio => "Audio",
            Category::Documents => "Documents",
        }
    }

    pub fn from_id(id: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn matches(self, e: &Entry, home: &Path) -> bool {
        match self {
            Category::Downloads => e.path.starts_with(home.join("Downloads")),
            Category::Images => e.kind == Kind::Image,
            Category::Videos => e.kind == Kind::Video,
            Category::Audio => e.kind == Kind::Audio,
            Category::Documents => matches!(e.kind, Kind::Pdf | Kind::Doc | Kind::Text),
        }
    }
}

const MAX_FILES: usize = 20_000;

/// Tutti i file sotto `root`, più recenti prima. Salta cartelle nascoste e cache.
pub fn scan(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= MAX_FILES {
            break;
        }
        for e in files::list(&dir, false) {
            if e.kind == Kind::Dir {
                if depth < 8 && e.name != "node_modules" {
                    stack.push((e.path, depth + 1));
                }
            } else {
                out.push(e);
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

pub fn filter(all: &[Entry], cat: Category, home: &Path) -> Vec<Entry> {
    all.iter().filter(|e| cat.matches(e, home)).cloned().collect()
}

pub fn total_size(list: &[Entry]) -> u64 {
    list.iter().map(|e| e.size).sum()
}

/// "3:07", "1:02:03"
pub fn fmt_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| files::home().join(".cache"));
    base.join("cheip/thumbs")
}

/// Percorso della miniatura in cache: cambia se il file viene modificato
pub fn thumb_path(src: &Path) -> PathBuf {
    let mtime = fs::metadata(src).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    mtime.hash(&mut h);
    cache_dir().join(format!("{:016x}.png", h.finish()))
}

fn save_rgba_thumb(img: image::DynamicImage, dst: &Path, size: u32, square: bool) -> bool {
    let t = if square { img.resize_to_fill(size, size, image::imageops::FilterType::Triangle) } else { img.thumbnail(size, size) };
    if let Some(d) = dst.parent() {
        let _ = fs::create_dir_all(d);
    }
    t.to_rgb8().save(dst).is_ok()
}

pub fn image_thumb(src: &Path, dst: &Path, size: u32) -> bool {
    // Foto da 13 MP su 1 GB di RAM: si decodifica una alla volta, in un thread
    match image::open(src) {
        Ok(img) => save_rgba_thumb(img, dst, size, true),
        Err(_) => false,
    }
}

#[cfg(feature = "native")]
static GST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn gst_ready() -> bool {
    #[cfg(feature = "native")]
    return *GST.get_or_init(|| gst::init().is_ok());
    #[cfg(not(feature = "native"))]
    false
}

/// Senza GStreamer e lofty (demo web) i metadati arrivano dal file system finto
#[cfg(not(feature = "native"))]
pub fn video_thumb(_src: &Path, _dst: &Path, _width: u32) -> Option<u64> {
    None
}

#[cfg(not(feature = "native"))]
pub fn audio_meta(p: &Path) -> AudioMeta {
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    if let Some(e) = crate::vfs::find(p) {
        return AudioMeta { title: if e.title.is_empty() { stem } else { e.title.into() }, artist: e.artist.into(), album: String::new(), secs: e.secs, cover: e.thumb.map(|t| t.to_vec()) };
    }
    AudioMeta { title: stem, ..Default::default() }
}

#[cfg(feature = "native")]
pub fn file_uri(p: &Path) -> Option<String> {
    gst::glib::filename_to_uri(p, None).ok().map(|u| u.to_string())
}

#[cfg(feature = "native")]
/// Coda video: videoconvert ! videoscale ! RGBA largo `width` ! appsink
pub fn video_branch(width: Option<u32>) -> Result<(gst::Bin, gst_app::AppSink), String> {
    let bin = gst::Bin::new();
    let conv = gst::ElementFactory::make("videoconvert").build().map_err(|e| e.to_string())?;
    let scale = gst::ElementFactory::make("videoscale").build().map_err(|e| e.to_string())?;
    let mut caps = gst::Caps::builder("video/x-raw").field("format", "RGBA").field("pixel-aspect-ratio", gst::Fraction::new(1, 1));
    if let Some(w) = width {
        caps = caps.field("width", w as i32);
    }
    let sink = gst_app::AppSink::builder().caps(&caps.build()).max_buffers(2).drop(true).build();
    bin.add_many([&conv, &scale, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    gst::Element::link_many([&conv, &scale, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    let pad = conv.static_pad("sink").ok_or("pad")?;
    bin.add_pad(&gst::GhostPad::with_target(&pad).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok((bin, sink))
}

#[cfg(feature = "native")]
/// Copia un fotogramma RGBA togliendo l'eventuale padding delle righe
pub fn sample_rgba(sample: &gst::Sample) -> Option<(u32, u32, Vec<u8>)> {
    let caps = sample.caps()?;
    let info = gstreamer_video::VideoInfo::from_caps(caps).ok()?;
    let buf = sample.buffer()?;
    let map = buf.map_readable().ok()?;
    let (w, h) = (info.width(), info.height());
    let stride = info.stride()[0] as usize;
    let row = w as usize * 4;
    let mut out = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        let start = y * stride;
        out.extend_from_slice(map.get(start..start + row)?);
    }
    Some((w, h, out))
}

#[cfg(feature = "native")]
/// Fotogramma a circa 1 s e durata del video. Restituisce la durata in secondi.
pub fn video_thumb(src: &Path, dst: &Path, width: u32) -> Option<u64> {
    if !gst_ready() {
        return None;
    }
    let uri = file_uri(src)?;
    let playbin = gst::ElementFactory::make("playbin").property("uri", &uri).build().ok()?;
    let (bin, sink) = video_branch(Some(width)).ok()?;
    playbin.set_property("video-sink", &bin);
    playbin.set_property("audio-sink", gst::ElementFactory::make("fakesink").build().ok()?);
    let result = (|| {
        playbin.set_state(gst::State::Paused).ok()?;
        let (res, _, _) = playbin.state(gst::ClockTime::from_seconds(8));
        res.ok()?;
        let dur = playbin.query_duration::<gst::ClockTime>().map(|d| d.seconds()).unwrap_or(0);
        if dur > 2 {
            let _ = playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, gst::ClockTime::from_seconds(1));
            let _ = playbin.state(gst::ClockTime::from_seconds(5));
        }
        let sample = sink.try_pull_preroll(gst::ClockTime::from_seconds(5))?;
        let (w, h, data) = sample_rgba(&sample)?;
        let img = image::RgbaImage::from_raw(w, h, data)?;
        if let Some(d) = dst.parent() {
            let _ = fs::create_dir_all(d);
        }
        image::DynamicImage::ImageRgba8(img).to_rgb8().save(dst).ok()?;
        Some(dur)
    })();
    let _ = playbin.set_state(gst::State::Null);
    result
}

#[derive(Clone, Debug, Default)]
pub struct AudioMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub secs: u64,
    pub cover: Option<Vec<u8>>,
}

#[cfg(feature = "native")]
pub fn audio_meta(p: &Path) -> AudioMeta {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut m = AudioMeta { title: stem, ..Default::default() };
    let Ok(tf) = lofty::read_from_path(p) else { return m };
    m.secs = tf.properties().duration().as_secs();
    if let Some(tag) = tf.primary_tag().or_else(|| tf.first_tag()) {
        if let Some(t) = tag.title() {
            if !t.trim().is_empty() {
                m.title = t.to_string();
            }
        }
        m.artist = tag.artist().map(|s| s.to_string()).unwrap_or_default();
        m.album = tag.album().map(|s| s.to_string()).unwrap_or_default();
        m.cover = tag.pictures().first().map(|pic| pic.data().to_vec());
    }
    m
}

pub fn cover_thumb(bytes: &[u8], dst: &Path, size: u32) -> bool {
    image::load_from_memory(bytes).map(|img| save_rgba_thumb(img, dst, size, true)).unwrap_or(false)
}

// ---------- Documenti ----------

pub fn has_mutool() -> bool {
    Command::new("mutool").arg("-v").output().is_ok()
}

pub fn pdf_pages(p: &Path) -> Option<u32> {
    #[cfg(target_arch = "wasm32")]
    return crate::vfs::find(p).map(|e| e.pages.len() as u32);
    #[cfg(not(target_arch = "wasm32"))]
    pdf_pages_native(p)
}

#[cfg(not(target_arch = "wasm32"))]
fn pdf_pages_native(p: &Path) -> Option<u32> {
    let out = Command::new("mutool").arg("info").arg(p).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix("Pages:").and_then(|n| n.trim().parse().ok()))
}

/// Rende una pagina (da 1) in PNG largo `width` pixel
pub fn pdf_render(p: &Path, page: u32, width: u32, dst: &Path) -> bool {
    if let Some(d) = dst.parent() {
        let _ = fs::create_dir_all(d);
    }
    Command::new("mutool")
        .args(["draw", "-q", "-w", &width.to_string(), "-o"])
        .arg(dst)
        .arg(p)
        .arg(page.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && dst.exists()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Testo da XML: un paragrafo per `para_end`, tag rimossi
pub fn xml_text(xml: &str, para_end: &str, tab: &str) -> String {
    let x = xml.replace(para_end, "\n").replace(tab, "\t");
    let mut out = String::with_capacity(x.len() / 3);
    let mut in_tag = false;
    for c in x.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let text = unescape(&out);
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// Testo di DOCX e ODT, primi 2 MB di XML
pub fn office_text(p: &Path) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    let f = std::io::Cursor::new(crate::vfs::find(p)?.data?);
    #[cfg(not(target_arch = "wasm32"))]
    let f = fs::File::open(p).ok()?;
    let mut zip = zip::ZipArchive::new(f).ok()?;
    let ext = p.extension()?.to_string_lossy().to_lowercase();
    let (inner, para, tab) = if ext == "odt" { ("content.xml", "</text:p>", "<text:tab/>") } else { ("word/document.xml", "</w:p>", "<w:tab/>") };
    let mut xml = String::new();
    zip.by_name(inner).ok()?.take(2 * 1024 * 1024).read_to_string(&mut xml).ok()?;
    Some(xml_text(&xml, para, tab))
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cheip-media-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(187), "3:07");
        assert_eq!(fmt_duration(3723), "1:02:03");
    }

    #[test]
    fn categories() {
        let d = tmp("cat");
        fs::create_dir_all(d.join("Downloads")).unwrap();
        fs::create_dir_all(d.join(".nascosta")).unwrap();
        for f in ["Downloads/a.pdf", "b.jpg", "c.mp4", "d.flac", "e.docx", ".nascosta/x.jpg", "f.bin"] {
            fs::write(d.join(f), b"x").unwrap();
        }
        let all = scan(&d);
        assert_eq!(all.len(), 6, "file nascosti esclusi");
        let n = |c| filter(&all, c, &d).len();
        assert_eq!(n(Category::Downloads), 1);
        assert_eq!(n(Category::Images), 1);
        assert_eq!(n(Category::Videos), 1);
        assert_eq!(n(Category::Audio), 1);
        assert_eq!(n(Category::Documents), 2);
    }

    #[test]
    fn docx_text() {
        let d = tmp("docx");
        let p = d.join("lettera.docx");
        let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
        z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(br#"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> Lumia &amp; co</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        z.finish().unwrap();
        assert_eq!(office_text(&p).unwrap(), "Hello Lumia & co\nSecond");
    }

    #[test]
    fn image_thumbnail() {
        let d = tmp("img");
        let src = d.join("foto.png");
        image::RgbImage::from_pixel(400, 300, image::Rgb([200, 50, 50])).save(&src).unwrap();
        let dst = d.join("t.png");
        assert!(image_thumb(&src, &dst, 96));
        let t = image::open(&dst).unwrap();
        assert_eq!((t.width(), t.height()), (96, 96));
    }

    /// Questi test usano ffmpeg, GStreamer e MuPDF se ci sono; altrimenti passano senza fare nulla
    fn ffmpeg(args: &[&str]) -> bool {
        Command::new("ffmpeg").args(["-y", "-loglevel", "error"]).args(args).status().map(|s| s.success()).unwrap_or(false)
    }

    #[test]
    fn video_thumbnail_and_duration() {
        let d = tmp("vid");
        let src = d.join("clip.mp4");
        if !ffmpeg(&["-f", "lavfi", "-i", "testsrc=size=320x240:rate=15:duration=3", "-c:v", "mpeg4", src.to_str().unwrap()]) || !gst_ready() {
            eprintln!("ffmpeg o GStreamer assenti, salto");
            return;
        }
        let dst = d.join("t.png");
        let dur = video_thumb(&src, &dst, 160).expect("miniatura video");
        assert_eq!(dur, 3);
        let t = image::open(&dst).unwrap();
        assert_eq!((t.width(), t.height()), (160, 120));
    }

    #[test]
    fn audio_tags() {
        let d = tmp("aud");
        let src = d.join("track.flac");
        if !ffmpeg(&["-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-metadata", "title=Night Drive", "-metadata", "artist=Second Dream", src.to_str().unwrap()]) {
            return;
        }
        let m = audio_meta(&src);
        assert_eq!(m.title, "Night Drive");
        assert_eq!(m.artist, "Second Dream");
        assert_eq!(m.secs, 2);
    }

    #[test]
    fn pdf_render_page() {
        if !has_mutool() {
            return;
        }
        let d = tmp("pdf");
        let p = d.join("doc.pdf");
        fs::write(&p, minimal_pdf()).unwrap();
        assert_eq!(pdf_pages(&p), Some(2));
        let out = d.join("p2.png");
        assert!(pdf_render(&p, 2, 300, &out));
        assert_eq!(image::open(&out).unwrap().width(), 300);
    }

    /// PDF di due pagine scritto a mano, con xref calcolato
    fn minimal_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>",
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }
}
