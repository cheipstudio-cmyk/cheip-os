//! File system finto della demo web: nel browser non c'è un disco, quindi
//! foto, documenti e miniature arrivano incorporati nel wasm.

use crate::files::{kind_of, Entry, Kind};
use std::path::{Path, PathBuf};

pub struct VEntry {
    pub path: &'static str,
    pub size: u64,
    /// Secondi fa in cui il file è stato "modificato"
    pub age: i64,
    pub data: Option<&'static [u8]>,
    pub thumb: Option<&'static [u8]>,
    pub info: &'static str,
    pub title: &'static str,
    pub artist: &'static str,
    pub secs: u64,
    pub pages: &'static [&'static [u8]],
}

pub use crate::vfs_data::FILES;

pub const HOME: &str = "/home/cheip";

pub fn find(p: &Path) -> Option<&'static VEntry> {
    FILES.iter().find(|e| Path::new(e.path) == p)
}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

/// Contenuto diretto di una cartella: file e sottocartelle ricavate dai percorsi
pub fn list(dir: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in FILES {
        let p = Path::new(e.path);
        let Ok(rest) = p.strip_prefix(dir) else { continue };
        let mut comps = rest.components();
        let Some(first) = comps.next() else { continue };
        let name = first.as_os_str().to_string_lossy().to_string();
        if comps.next().is_some() {
            if !out.iter().any(|x| x.name == name) {
                out.push(Entry { name: name.clone(), path: dir.join(&name), kind: Kind::Dir, size: 0, modified: now() - e.age });
            }
        } else {
            out.push(Entry { kind: kind_of(&name, false), name, path: PathBuf::from(e.path), size: e.size, modified: now() - e.age });
        }
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<slint::Image> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some(slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h)))
}
