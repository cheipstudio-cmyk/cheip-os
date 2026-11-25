//! OTA v1: un manifest JSON per canale e un repository apk firmato.
//! Il manifest dice qual è l'ultima versione; l'installazione la fa apk
//! tramite l'helper privilegiato.

use serde::Deserialize;
use std::fs;

pub const DEFAULT_MANIFEST: &str = "https://updates.cheipstudio.org/cheip-os/stable/manifest.json";

#[derive(Deserialize, Debug, Clone)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    /// Modelli supportati da questa release (compatible del device tree). Vuoto = tutti
    #[serde(default)]
    pub devices: Vec<String>,
}

#[derive(Deserialize)]
struct Config {
    manifest: String,
}

pub fn manifest_url() -> String {
    fs::read_to_string("/etc/cheip/update.json")
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .map(|c| c.manifest)
        .unwrap_or_else(|| DEFAULT_MANIFEST.to_string())
}

#[cfg(not(feature = "native"))]
pub fn fetch(_url: &str) -> Result<Manifest, String> {
    Err("offline".into())
}

#[cfg(feature = "native")]
pub fn fetch(url: &str) -> Result<Manifest, String> {
    ureq::get(url)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| e.to_string())?
        .into_json::<Manifest>()
        .map_err(|e| e.to_string())
}

fn parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(|c: char| c == '.' || c == '-')
        .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0))
        .collect()
}

/// true se `remote` è più recente di `local`
pub fn is_newer(remote: &str, local: &str) -> bool {
    let (r, l) = (parts(remote), parts(local));
    for i in 0..r.len().max(l.len()) {
        let (a, b) = (*r.get(i).unwrap_or(&0), *l.get(i).unwrap_or(&0));
        if a != b {
            return a > b;
        }
    }
    false
}

/// Il telefono è tra quelli supportati dal manifest?
pub fn supports_device(m: &Manifest) -> bool {
    if m.devices.is_empty() {
        return true;
    }
    let compat = fs::read("/proc/device-tree/compatible").unwrap_or_default();
    let list: Vec<String> = compat
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect();
    // Sul PC non c'è device tree: lasciamo passare
    list.is_empty() || m.devices.iter().any(|d| list.contains(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("0.1", "0.1.0"));
    }

    #[test]
    fn manifest_parse() {
        let m: Manifest = serde_json::from_str(r#"{"version":"0.1.1","notes":"Fix","devices":[]}"#).unwrap();
        assert_eq!(m.version, "0.1.1");
        assert!(supports_device(&m));
    }
}
