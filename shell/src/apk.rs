//! Lettura di un APK prima di installarlo: pacchetto, versione, SDK minimo,
//! nome e architetture delle librerie native (il 735 è a 32 bit: serve armeabi-v7a).

use std::io::Read;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ApkInfo {
    pub package: String,
    pub label: String,
    pub version_name: String,
    pub version_code: i64,
    pub min_sdk: i64,
    pub abis: Vec<String>,
    pub size: u64,
}

impl ApkInfo {
    /// Nessuna libreria nativa (gira ovunque) oppure librerie anche a 32 bit
    pub fn runs_on_armv7(&self) -> bool {
        self.abis.is_empty() || self.abis.iter().any(|a| a == "armeabi-v7a" || a == "armeabi")
    }
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Pool di stringhe del formato XML binario di Android
fn string_pool(b: &[u8], start: usize) -> Option<Vec<String>> {
    let count = u32_at(b, start + 8)? as usize;
    let flags = u32_at(b, start + 16)?;
    let strings_start = u32_at(b, start + 20)? as usize;
    let utf8 = flags & 0x100 != 0;
    let header = u16_at(b, start + 2)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count.min(100_000) {
        let off = u32_at(b, start + header + i * 4)? as usize;
        let mut p = start + strings_start + off;
        if utf8 {
            // lunghezza in caratteri, poi in byte (uno o due byte ciascuna)
            let c = *b.get(p)?;
            p += if c & 0x80 != 0 { 2 } else { 1 };
            let mut n = *b.get(p)? as usize;
            if n & 0x80 != 0 {
                n = ((n & 0x7f) << 8) | *b.get(p + 1)? as usize;
                p += 2;
            } else {
                p += 1;
            }
            out.push(String::from_utf8_lossy(b.get(p..p + n)?).into_owned());
        } else {
            let mut n = u16_at(b, p)? as usize;
            p += 2;
            if n & 0x8000 != 0 {
                n = ((n & 0x7fff) << 16) | u16_at(b, p)? as usize;
                p += 2;
            }
            let units: Vec<u16> = (0..n).filter_map(|k| u16_at(b, p + k * 2)).collect();
            out.push(String::from_utf16_lossy(&units));
        }
    }
    Some(out)
}

/// Estrae dal manifest binario gli attributi che ci servono
pub fn parse_manifest(b: &[u8], info: &mut ApkInfo) -> Option<()> {
    if u16_at(b, 0)? != 0x0003 {
        return None;
    }
    let total = (u32_at(b, 4)? as usize).min(b.len());
    let mut pos = u16_at(b, 2)? as usize;
    let mut strings: Vec<String> = Vec::new();
    let s = |strings: &Vec<String>, i: u32| strings.get(i as usize).cloned().unwrap_or_default();
    while pos + 8 <= total {
        let kind = u16_at(b, pos)?;
        let size = u32_at(b, pos + 4)? as usize;
        if size == 0 {
            break;
        }
        match kind {
            0x0001 => strings = string_pool(b, pos)?,
            0x0102 => {
                let tag = s(&strings, u32_at(b, pos + 20)?);
                let attr_start = u16_at(b, pos + 24)? as usize;
                let attr_size = u16_at(b, pos + 26)? as usize;
                let attr_count = u16_at(b, pos + 28)? as usize;
                for k in 0..attr_count {
                    let a = pos + 16 + attr_start + k * attr_size.max(20);
                    let name = s(&strings, u32_at(b, a + 4)?);
                    let raw = u32_at(b, a + 8)?;
                    let dtype = *b.get(a + 15)?;
                    let data = u32_at(b, a + 16)?;
                    let text = if raw != u32::MAX { Some(s(&strings, raw)) } else if dtype == 0x03 { Some(s(&strings, data)) } else { None };
                    let int = if (0x10..=0x11).contains(&dtype) { Some(data as i32 as i64) } else { None };
                    match (tag.as_str(), name.as_str()) {
                        ("manifest", "package") => info.package = text.unwrap_or_default(),
                        ("manifest", "versionName") => info.version_name = text.or(int.map(|i| i.to_string())).unwrap_or_default(),
                        ("manifest", "versionCode") => info.version_code = int.or(text.and_then(|t| t.parse().ok())).unwrap_or(0),
                        ("uses-sdk", "minSdkVersion") => info.min_sdk = int.or(text.and_then(|t| t.parse().ok())).unwrap_or(0),
                        ("application", "label") => {
                            if let Some(t) = text.filter(|t| !t.is_empty()) {
                                info.label = t;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        pos += size;
    }
    Some(())
}

pub fn read(path: &Path) -> Result<ApkInfo, String> {
    #[cfg(target_arch = "wasm32")]
    let bytes: Vec<u8> = crate::vfs::find(path).and_then(|e| e.data).map(|d| d.to_vec()).ok_or_else(|| "File not found".to_string())?;
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let size = bytes.len() as u64;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "Not a valid APK file".to_string())?;
    let mut info = ApkInfo { size, ..Default::default() };
    let mut abis: Vec<String> = Vec::new();
    let names: Vec<String> = zip.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    for name in names.iter() {
        {
            if let Some(rest) = name.strip_prefix("lib/") {
                if let Some(abi) = rest.split('/').next() {
                    if name.ends_with(".so") && !abis.iter().any(|a| a == abi) {
                        abis.push(abi.to_string());
                    }
                }
            }
        }
    }
    info.abis = abis;
    let mut buf = Vec::new();
    zip.by_name("AndroidManifest.xml").map_err(|_| "Missing AndroidManifest.xml".to_string())?.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    parse_manifest(&buf, &mut info).ok_or_else(|| "Unreadable manifest".to_string())?;
    if info.label.is_empty() {
        info.label = path.file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_default();
    }
    Ok(info)
}

/// Livello API Android in nome leggibile
pub fn android_name(sdk: i64) -> String {
    let v = match sdk {
        0 => return "any".into(),
        21 | 22 => "5",
        23 => "6",
        24 | 25 => "7",
        26 | 27 => "8",
        28 => "9",
        29 => "10",
        30 => "11",
        31 | 32 => "12",
        33 => "13",
        34 => "14",
        35 => "15",
        36 => "16",
        _ if sdk < 21 => "4",
        _ => "17+",
    };
    format!("Android {v}")
}

/// Manifest binario minimo costruito a mano (pool UTF-16), per test e demo
pub fn build_manifest(package: &str, version: &str, code: u32, min_sdk: u32) -> Vec<u8> {
        let strs = ["manifest", "package", "versionName", "versionCode", package, version, "uses-sdk", "minSdkVersion"];
        let mut pool = Vec::new();
        let mut offs = Vec::new();
        for s in strs {
            offs.push(pool.len() as u32);
            let u: Vec<u16> = s.encode_utf16().collect();
            pool.extend((u.len() as u16).to_le_bytes());
            for c in u {
                pool.extend(c.to_le_bytes());
            }
            pool.extend([0, 0]);
        }
        while pool.len() % 4 != 0 {
            pool.push(0);
        }
        let hdr = 28u32;
        let strings_start = hdr + 4 * strs.len() as u32;
        let mut sp = Vec::new();
        sp.extend(1u16.to_le_bytes());
        sp.extend((hdr as u16).to_le_bytes());
        sp.extend((strings_start + pool.len() as u32).to_le_bytes());
        sp.extend((strs.len() as u32).to_le_bytes());
        sp.extend(0u32.to_le_bytes());
        sp.extend(0u32.to_le_bytes());
        sp.extend(strings_start.to_le_bytes());
        sp.extend(0u32.to_le_bytes());
        for o in offs {
            sp.extend(o.to_le_bytes());
        }
        sp.extend(pool);
        let elem = |tag: u32, attrs: &[(u32, u32, u8, u32)]| -> Vec<u8> {
            let mut e = Vec::new();
            e.extend(0x0102u16.to_le_bytes());
            e.extend(16u16.to_le_bytes());
            e.extend((36 + 20 * attrs.len() as u32).to_le_bytes());
            e.extend(1u32.to_le_bytes());
            e.extend(u32::MAX.to_le_bytes());
            e.extend(u32::MAX.to_le_bytes());
            e.extend(tag.to_le_bytes());
            e.extend(20u16.to_le_bytes());
            e.extend(20u16.to_le_bytes());
            e.extend((attrs.len() as u16).to_le_bytes());
            e.extend([0u8; 6]);
            for (name, raw, dtype, data) in attrs {
                e.extend(u32::MAX.to_le_bytes());
                e.extend(name.to_le_bytes());
                e.extend(raw.to_le_bytes());
                e.extend(8u16.to_le_bytes());
                e.push(0);
                e.push(*dtype);
                e.extend(data.to_le_bytes());
            }
            e
        };
        let m = elem(0, &[(1, 4, 3, 4), (2, 5, 3, 5), (3, u32::MAX, 0x10, code)]);
        let u = elem(6, &[(7, u32::MAX, 0x10, min_sdk)]);
        let mut out = Vec::new();
        out.extend(3u16.to_le_bytes());
        out.extend(8u16.to_le_bytes());
        out.extend((8 + sp.len() + m.len() + u.len()).to_le_bytes()[..4].to_vec());
        out.extend(sp);
        out.extend(m);
        out.extend(u);
        out
    }


/// APK finto ma valido per la demo: manifest e librerie a 32 e 64 bit
#[cfg(not(target_arch = "wasm32"))]
pub fn demo_apk(package: &str, version: &str, code: u32, min_sdk: u32) -> Vec<u8> {
    use std::io::Write;
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let o = zip::write::SimpleFileOptions::default();
        let _ = z.start_file("AndroidManifest.xml", o);
        let _ = z.write_all(&build_manifest(package, version, code, min_sdk));
        for abi in ["armeabi-v7a", "arm64-v8a"] {
            let _ = z.start_file(format!("lib/{abi}/libapp.so"), o);
            let _ = z.write_all(&[0u8; 2048]);
        }
        let _ = z.start_file("classes.dex", o);
        let _ = z.write_all(&[0u8; 300_000]);
        let _ = z.finish();
    }
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binary_manifest() {
        let mut i = ApkInfo::default();
        parse_manifest(&build_manifest("com.example.demo", "1.2.3", 42, 26), &mut i).unwrap();
        assert_eq!(i.package, "com.example.demo");
        assert_eq!(i.version_name, "1.2.3");
        assert_eq!(i.version_code, 42);
        assert_eq!(i.min_sdk, 26);
        assert_eq!(android_name(26), "Android 8");
        let arm64 = ApkInfo { abis: vec!["arm64-v8a".into()], ..Default::default() };
        assert!(!arm64.runs_on_armv7());
        assert!(ApkInfo::default().runs_on_armv7());
    }
}
