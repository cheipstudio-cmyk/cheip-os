//! Lettura e controllo dell'hardware via sysfs/procfs. Tutto best effort:
//! sul PC di sviluppo i file mancano e si torna ai valori di riserva.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn read_trim(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string())
}

fn class_dirs(class: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(format!("/sys/class/{class}"))
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// (percentuale, in carica)
pub fn battery() -> Option<(i32, bool)> {
    for d in class_dirs("power_supply") {
        if read_trim(d.join("type")).as_deref() == Some("Battery") {
            let cap = read_trim(d.join("capacity"))?.parse().ok()?;
            let charging = read_trim(d.join("status")).as_deref() == Some("Charging");
            return Some((cap, charging));
        }
    }
    None
}

/// (usata MB, totale MB)
pub fn memory() -> (i32, i32) {
    let info = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| -> i64 {
        info.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    let total = field("MemTotal:");
    let avail = field("MemAvailable:");
    (((total - avail) / 1024) as i32, (total / 1024) as i32)
}

/// RAM dei processi nel container Waydroid, in MB
pub fn android_memory() -> i32 {
    let mut kb: i64 = 0;
    if let Ok(it) = fs::read_dir("/proc") {
        for e in it.flatten() {
            let name = e.file_name();
            let Some(pid) = name.to_str() else { continue };
            if !pid.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let cg = fs::read_to_string(e.path().join("cgroup")).unwrap_or_default();
            if !cg.contains("waydroid") {
                continue;
            }
            let status = fs::read_to_string(e.path().join("status")).unwrap_or_default();
            if let Some(v) = status
                .lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<i64>().ok())
            {
                kb += v;
            }
        }
    }
    (kb / 1024) as i32
}

pub fn model() -> String {
    read_trim("/proc/device-tree/model").unwrap_or_else(|| "Development PC".into())
}

pub fn kernel() -> String {
    read_trim("/proc/sys/kernel/osrelease").unwrap_or_default()
}

fn backlight_dir() -> Option<PathBuf> {
    class_dirs("backlight").into_iter().next()
}

pub fn brightness() -> Option<f32> {
    let d = backlight_dir()?;
    let cur: f32 = read_trim(d.join("brightness"))?.parse().ok()?;
    let max: f32 = read_trim(d.join("max_brightness"))?.parse().ok()?;
    Some(if max > 0.0 { cur / max } else { 1.0 })
}

pub fn set_brightness(v: f32) {
    if let Some(d) = backlight_dir() {
        if let Some(max) = read_trim(d.join("max_brightness")).and_then(|m| m.parse::<f32>().ok()) {
            let val = ((v.clamp(0.02, 1.0)) * max).round() as i64;
            let _ = fs::write(d.join("brightness"), val.to_string());
        }
    }
}

fn torch_dir() -> Option<PathBuf> {
    class_dirs("leds").into_iter().find(|d| {
        let n = d.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        n.contains("torch") || n.contains("flash")
    })
}

pub fn set_torch(on: bool) {
    if let Some(d) = torch_dir() {
        let max = read_trim(d.join("max_brightness")).unwrap_or_else(|| "1".into());
        let _ = fs::write(d.join("brightness"), if on { max } else { "0".into() });
    }
}

/// Esegue un comando ignorando gli errori (manca sul PC, fallisce senza permessi)
pub fn run(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// Azioni che richiedono root: passano dall'helper con regola doas
pub fn helper(args: &[&str]) -> bool {
    let mut full = vec!["/usr/libexec/cheip/cheip-helper"];
    full.extend_from_slice(args);
    run("doas", &full)
}

pub fn radio(kind: &str, on: bool) {
    // NetworkManager per wifi e dati, rfkill per il bluetooth
    match kind {
        "wifi" => {
            run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
        }
        "wwan" => {
            run("nmcli", &["radio", "wwan", if on { "on" } else { "off" }]);
        }
        "bluetooth" => {
            run("rfkill", &[if on { "unblock" } else { "block" }, "bluetooth"]);
        }
        _ => {}
    }
}

pub fn wifi_on() -> bool {
    Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
        .unwrap_or(true)
}

pub fn waydroid_installed() -> bool {
    Path::new("/usr/bin/waydroid").exists()
}

pub fn waydroid_running() -> bool {
    Command::new("waydroid")
        .arg("status")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
        .unwrap_or(false)
}

/// App Android installate: Waydroid crea un .desktop per ognuna.
/// (pacchetto, nome, percorso icona)
pub fn android_apps() -> Vec<(String, String, Option<PathBuf>)> {
    let mut out = Vec::new();
    let Some(home) = std::env::var_os("HOME") else { return out };
    let dir = Path::new(&home).join(".local/share/applications");
    if let Ok(it) = fs::read_dir(dir) {
        for e in it.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            let Some(pkg) = fname.strip_prefix("waydroid.").and_then(|s| s.strip_suffix(".desktop")) else { continue };
            let content = fs::read_to_string(e.path()).unwrap_or_default();
            let name = content
                .lines()
                .find_map(|l| l.strip_prefix("Name="))
                .unwrap_or(pkg)
                .to_string();
            let icon = content
                .lines()
                .find_map(|l| l.strip_prefix("Icon="))
                .map(PathBuf::from)
                .filter(|p| p.is_absolute() && p.exists());
            out.push((pkg.to_string(), name, icon));
        }
    }
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    out
}

// ---------- Wi-Fi ----------

#[derive(Clone, Debug, PartialEq)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: i32,
    pub secure: bool,
    pub active: bool,
}

/// nmcli -t usa ':' come separatore e '\:' per i due punti dentro i valori
pub fn split_terse(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in line.chars() {
        if esc {
            out.last_mut().unwrap().push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == ':' {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
    }
    out
}

/// Output di: nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY dev wifi list
pub fn parse_wifi(s: &str) -> Vec<WifiNet> {
    let mut v: Vec<WifiNet> = Vec::new();
    for line in s.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let net = WifiNet {
            active: f[0].trim() == "*",
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0),
            secure: !f[3].trim().is_empty() && f[3].trim() != "--",
        };
        // Stessa rete su più access point: teniamo il segnale migliore
        match v.iter_mut().find(|n| n.ssid == net.ssid) {
            Some(n) => {
                n.signal = n.signal.max(net.signal);
                n.active |= net.active;
            }
            None => v.push(net),
        }
    }
    v.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    v
}

pub fn wifi_scan() -> Vec<WifiNet> {
    Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list", "--rescan", "yes"])
        .output()
        .map(|o| parse_wifi(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn wifi_connect(ssid: &str, password: &str) -> Result<(), String> {
    let mut args = vec!["dev", "wifi", "connect", ssid];
    if !password.is_empty() {
        args.push("password");
        args.push(password);
    }
    let out = Command::new("nmcli").args(&args).output().map_err(|_| "NetworkManager not available".to_string())?;
    if out.status.success() {
        Ok(())
    } else if String::from_utf8_lossy(&out.stderr).contains("Secrets were required") {
        Err("Wrong password".into())
    } else {
        Err("Connection failed".into())
    }
}

// ---------- Audio ----------

/// Volume ALSA 0..1 (la shell gira fuori dalla sessione utente, niente PipeWire)
pub fn volume() -> Option<f32> {
    let out = Command::new("amixer").args(["-M", "get", "Master"]).output().ok()?;
    parse_volume(&String::from_utf8_lossy(&out.stdout))
}

pub fn parse_volume(s: &str) -> Option<f32> {
    let start = s.find('[')?;
    let end = s[start..].find("%]")? + start;
    s[start + 1..end].parse::<f32>().ok().map(|v| v / 100.0)
}

pub fn set_volume(v: f32) {
    run("amixer", &["-q", "-M", "set", "Master", &format!("{}%", (v.clamp(0.0, 1.0) * 100.0).round())]);
}

// ---------- Bluetooth ----------

pub fn bt_paired() -> Vec<String> {
    Command::new("bluetoothctl")
        .args(["devices", "Paired"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.strip_prefix("Device ").and_then(|r| r.split_once(' ')).map(|(_, n)| n.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn timezone() -> String {
    fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_str().and_then(|s| s.split("zoneinfo/").nth(1)).map(|s| s.to_string()))
        .unwrap_or_else(|| "Europe/Rome".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_parse() {
        let s = "*:Casa Casale:82:WPA2\n :Casa Casale:40:WPA2\n :Bar\\:Centro:55:\n :FASTWEB-12:61:WPA1 WPA2\n ::30:WPA2\n";
        let v = parse_wifi(s);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], WifiNet { ssid: "Casa Casale".into(), signal: 82, secure: true, active: true });
        assert_eq!(v[2].ssid, "Bar:Centro");
        assert!(!v[2].secure);
    }

    #[test]
    fn volume_parse() {
        let s = "Simple mixer control 'Master',0\n  Mono: Playback 42 [65%] [on]";
        assert_eq!(parse_volume(s), Some(0.65));
    }
}
