//! Gestore file: elenco cartelle, anteprime, nuova cartella, eliminazione.
//! Confinato nella home dell'utente e nelle schede SD montate.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Dir,
    Text,
    Image,
    Video,
    Audio,
    Pdf,
    Doc,
    Apk,
    Other,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub size: u64,
    pub modified: i64,
}

pub fn home() -> PathBuf {
    #[cfg(target_arch = "wasm32")]
    return PathBuf::from(crate::vfs::HOME);
    #[cfg(not(target_arch = "wasm32"))]
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Radici consentite: home e /media (schede SD)
pub fn allowed(p: &Path) -> bool {
    #[cfg(target_arch = "wasm32")]
    return p.starts_with(crate::vfs::HOME);
    #[cfg(not(target_arch = "wasm32"))]
    allowed_native(p)
}

#[cfg(not(target_arch = "wasm32"))]
fn allowed_native(p: &Path) -> bool {
    let Ok(c) = p.canonicalize() else { return false };
    let home = home().canonicalize().unwrap_or_else(|_| home());
    c.starts_with(&home) || c.starts_with("/media") || c.starts_with("/run/media")
}

pub fn kind_of(name: &str, is_dir: bool) -> Kind {
    if is_dir {
        return Kind::Dir;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "txt" | "md" | "log" | "json" | "csv" | "conf" | "cfg" | "ini" | "sh" | "rs" | "py" | "xml" | "html" | "srt" => Kind::Text,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => Kind::Image,
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "3gp" | "m4v" | "mpg" | "mpeg" => Kind::Video,
        "mp3" | "ogg" | "oga" | "opus" | "flac" | "wav" | "m4a" | "aac" | "wma" => Kind::Audio,
        "pdf" => Kind::Pdf,
        "docx" | "odt" => Kind::Doc,
        "apk" => Kind::Apk,
        _ => Kind::Other,
    }
}

pub fn list(dir: &Path, show_hidden: bool) -> Vec<Entry> {
    #[cfg(target_arch = "wasm32")]
    let mut v: Vec<Entry> = {
        let _ = show_hidden;
        crate::vfs::list(dir)
    };
    #[cfg(not(target_arch = "wasm32"))]
    let mut v: Vec<Entry> = fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if !show_hidden && name.starts_with('.') {
                        return None;
                    }
                    let md = e.metadata().ok()?;
                    let modified = md
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    Some(Entry { kind: kind_of(&name, md.is_dir()), name, path: e.path(), size: md.len(), modified })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| {
        (b.kind == Kind::Dir).cmp(&(a.kind == Kind::Dir)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    v
}

pub fn human_size(b: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// Primi 64 KB, per non riempire la RAM con un log da 200 MB
pub fn preview_text(p: &Path) -> String {
    #[cfg(target_arch = "wasm32")]
    return crate::vfs::find(p).and_then(|e| e.data).map(|d| String::from_utf8_lossy(d).to_string()).unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    preview_text_native(p)
}

#[cfg(not(target_arch = "wasm32"))]
fn preview_text_native(p: &Path) -> String {
    use std::io::Read;
    let mut buf = Vec::new();
    if let Ok(f) = fs::File::open(p) {
        let _ = f.take(64 * 1024).read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).to_string()
}

pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n != "." && n != ".." && !n.contains('/') && !n.contains('\0')
}

pub fn mkdir(dir: &Path, name: &str) -> bool {
    valid_name(name) && allowed(dir) && fs::create_dir(dir.join(name.trim())).is_ok()
}

pub fn remove(p: &Path) -> bool {
    if !allowed(p) || p.canonicalize().ok() == home().canonicalize().ok() {
        return false;
    }
    if p.is_dir() {
        fs::remove_dir_all(p).is_ok()
    } else {
        fs::remove_file(p).is_ok()
    }
}

/// (usato, totale) in byte del filesystem che contiene `p`
pub fn disk_usage(p: &Path) -> Option<(u64, u64)> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        return Some((3_110_000_000, 6_010_000_000));
    }
    #[cfg(not(target_arch = "wasm32"))]
    disk_usage_native(p)
}

#[cfg(not(target_arch = "wasm32"))]
fn disk_usage_native(p: &Path) -> Option<(u64, u64)> {
    let out = std::process::Command::new("df").args(["-Pk"]).arg(p).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().nth(1)?;
    let f: Vec<&str> = line.split_whitespace().collect();
    let total: u64 = f.get(1)?.parse().ok()?;
    let used: u64 = f.get(2)?.parse().ok()?;
    Some((used * 1024, total * 1024))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_and_ops() {
        let dir = std::env::temp_dir().join(format!("cheip-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("b-cartella")).unwrap();
        fs::write(dir.join("a.txt"), "ciao").unwrap();
        fs::write(dir.join("foto.JPG"), [0u8; 10]).unwrap();
        fs::write(dir.join(".nascosto"), "x").unwrap();
        let v = list(&dir, false);
        let names: Vec<&str> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["b-cartella", "a.txt", "foto.JPG"], "cartelle prima, nascosti esclusi");
        assert_eq!(v[2].kind, Kind::Image);
        assert_eq!(preview_text(&dir.join("a.txt")), "ciao");
        assert!(!valid_name("../x"));
        assert!(!valid_name("  "));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn confinement() {
        assert!(!allowed(Path::new("/etc")));
        assert!(allowed(&home()));
    }
}
