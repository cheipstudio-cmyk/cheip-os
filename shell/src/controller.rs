//! Collega la UI Slint ai dati. Il cuore (shell, sistema, calendario, note,
//! OTA) sta qui; telefono, messaggi, browser, file e impostazioni nei moduli ctl_*.

use crate::calendar as cal;
use crate::store::Store;
use crate::system as sys;
use crate::update;
use crate::*;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Esegue un blocco con ui e stato, ignorando la chiamata se la finestra non esiste più
#[macro_export]
macro_rules! with {
    ($ui:expr, $st:expr, |$u:ident, $s:ident $(, $a:ident)*| $body:block) => {{
        let w = $ui.as_weak();
        let st = $st.clone();
        move |$($a),*| {
            let Some($u) = w.upgrade() else { return };
            #[allow(unused_mut)]
            let mut $s = st.borrow_mut();
            $body
        }
    }};
}

pub struct Toggle {
    pub id: &'static str,
    pub label: &'static str,
    pub on: bool,
}

pub struct Recent {
    pub id: String,
    pub name: String,
    pub android: bool,
}

static TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Tab {
    pub id: u64,
    pub history: Vec<String>,
    pub page: Option<crate::web::Page>,
    pub error: String,
    pub loading: bool,
}

impl Default for Tab {
    fn default() -> Self {
        Tab { id: TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), history: vec![], page: None, error: String::new(), loading: false }
    }
}

#[derive(Default)]
pub struct CallState {
    pub path: String,
    pub number: String,
    pub incoming: bool,
    pub answered_at: Option<i64>,
    pub started_at: i64,
}

pub struct State {
    pub store: Store,
    pub live: bool,
    pub today: NaiveDate,
    pub cal_year: i32,
    pub cal_month: u32,
    pub cal_sel: u32,
    pub toggles: Vec<Toggle>,
    pub recents: Vec<Recent>,
    pub search: String,
    pub note_filter: i32,
    pub modem_ok: bool,
    pub call: Option<CallState>,
    pub missed: i32,
    pub open_thread: String,
    pub notifs: Vec<NotifItem>,
    pub next_notif: i32,
    pub ring_timer: Option<slint::Timer>,
    pub hud_timer: Option<slint::Timer>,
    pub headsup_timer: Option<slint::Timer>,
    pub screen_off: bool,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub cwd: PathBuf,
    pub entries: Vec<crate::files::Entry>,
    pub scroll_gen: i32,
    pub fx: crate::ctl_files::FilesState,
}

pub type Shared = Rc<RefCell<State>>;

thread_local! {
    static STATE: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

/// Stato condiviso dal thread della UI, per le risposte dei thread di lavoro
pub fn shared() -> Option<Shared> {
    STATE.with(|s| s.borrow().clone())
}

pub fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

pub fn initial(s: &str) -> SharedString {
    s.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "#".into()).into()
}

/// "22:47" today, "Yesterday", otherwise "Nov 25"
pub fn when(ts: i64, now: NaiveDateTime) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&Local).naive_local()) else { return String::new() };
    let days = (now.date() - t.date()).num_days();
    match days {
        0 => format!("{:02}:{:02}", t.hour(), t.minute()),
        1 => "Yesterday".into(),
        _ => cal::short_label(t.date()),
    }
}

/// App di sistema: (id, nome, nel dock)
pub const SYSTEM_APPS: [(&str, &str, bool); 10] = [
    ("phone", "Phone", true),
    ("messages", "Messages", true),
    ("browser", "Browser", true),
    ("camera", "Camera", true),
    ("files", "Files", false),
    ("calendar", "Calendar", false),
    ("notes", "Notes", false),
    ("clock", "Clock", false),
    ("settings", "Settings", false),
    ("log", "Log", false),
];

pub fn glyph(ui: &AppWindow, id: &str) -> SharedString {
    let i = ui.global::<Icons>();
    match id {
        "phone" => i.get_phone_f(),
        "messages" => i.get_message_f(),
        "browser" => i.get_globe_f(),
        "camera" => i.get_camera_f(),
        "files" => i.get_folder_f(),
        "calendar" => i.get_calendar_f(),
        "notes" => i.get_notes_f(),
        "clock" => i.get_alarm_f(),
        "settings" | "system" => i.get_gear_f(),
        "log" => i.get_code_f(),
        "wifi" => i.get_wifi_f(),
        "dati" => i.get_data_f(),
        "bt" => i.get_bluetooth_f(),
        "torcia" => i.get_torch_f(),
        "rot" => i.get_rotate_f(),
        "eco" => i.get_leaf_f(),
        "dark" => i.get_moon_f(),
        "dnd" => i.get_dnd_f(),
        "airplane" => i.get_airplane_f(),
        "hotspot" => i.get_hotspot_f(),
        "alarm" => i.get_alarm_f(),
        _ => i.get_android_f(),
    }
}

impl State {
    fn new(store: Store, now: NaiveDateTime, live: bool) -> Self {
        let today = now.date();
        let wifi = if live { sys::has_wifi() && sys::wifi_on() } else { true };
        State {
            store,
            live,
            today,
            cal_year: today.year(),
            cal_month: today.month(),
            cal_sel: today.day(),
            toggles: vec![
                Toggle { id: "wifi", label: "Internet", on: wifi },
                Toggle { id: "bt", label: "Bluetooth", on: false },
                Toggle { id: "dnd", label: "Do Not Disturb", on: false },
                Toggle { id: "dark", label: "Dark theme", on: true },
                Toggle { id: "torcia", label: "Flashlight", on: false },
                Toggle { id: "dati", label: "Mobile data", on: false },
                Toggle { id: "eco", label: "Battery Saver", on: false },
                Toggle { id: "airplane", label: "Airplane mode", on: false },
                Toggle { id: "alarm", label: "Alarm", on: false },
            ],
            recents: Vec::new(),
            search: String::new(),
            note_filter: 0,
            modem_ok: false,
            call: None,
            missed: 0,
            open_thread: String::new(),
            notifs: Vec::new(),
            next_notif: 1,
            ring_timer: None,
            hud_timer: None,
            headsup_timer: None,
            screen_off: false,
            tabs: vec![Tab::default()],
            tab: 0,
            cwd: crate::files::home(),
            entries: Vec::new(),
            scroll_gen: 0,
            fx: Default::default(),
        }
    }

    pub fn toggle_on(&self, id: &str) -> bool {
        self.toggles.iter().find(|t| t.id == id).map(|t| t.on).unwrap_or(false)
    }

    /// Aggiunge una notifica all'elenco, senza suoni né banner
    pub fn notify(&mut self, app_id: &str, title: &str, text: &str) -> NotifItem {
        let now = Local::now();
        let item = NotifItem {
            id: self.next_notif,
            app: title.into(),
            app_id: app_id.into(),
            glyph: icon_path(app_id).into(),
            time: format!("{:02}:{:02}", now.hour(), now.minute()).into(),
            text: text.into(),
            android: !SYSTEM_APPS.iter().any(|(id, _, _)| *id == app_id) && app_id != "system",
        };
        self.next_notif += 1;
        self.notifs.retain(|n| !(n.app_id == item.app_id && n.app == item.app));
        self.notifs.insert(0, item.clone());
        self.notifs.truncate(8);
        item
    }
}

thread_local! {
    static UI: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
}

pub fn ui_handle() -> Option<AppWindow> {
    UI.with(|u| u.borrow().as_ref().and_then(|w| w.upgrade()))
}

/// Percorso dell'icona di un'app (stesse icone del launcher)
pub fn icon_path(app_id: &str) -> String {
    ui_handle().map(|ui| glyph(&ui, app_id).to_string()).unwrap_or_default()
}

/// Notifica completa: elenco, banner in alto, suono e vibrazione secondo la modalità
pub fn post(ui: &AppWindow, st: &mut State, app_id: &str, title: &str, text: &str, sound: crate::feedback::Sound) {
    let item = st.notify(app_id, title, text);
    push_notifs(ui, st);
    push_apps(ui, st);
    let shell = ui.global::<Shell>();
    let in_app = shell.get_app_open() && shell.get_app_id() == app_id;
    if !in_app {
        shell.set_headsup(item);
        shell.set_headsup_shown(true);
        let t = slint::Timer::default();
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(4200), || {
            if let Some(ui) = ui_handle() {
                ui.global::<Shell>().set_headsup_shown(false);
            }
        });
        st.headsup_timer = Some(t);
    }
    alert(ui, sound, 220);
}

/// Suono o vibrazione secondo squillo / vibrazione / silenzioso
pub fn alert(ui: &AppWindow, sound: crate::feedback::Sound, vibrate_ms: u16) {
    let sys = ui.global::<Sys>();
    match sys.get_ringer() {
        0 => {
            let sound = match sound {
                crate::feedback::Sound::Notification | crate::feedback::Sound::Message => crate::feedback::Sound::Tone(sys.get_notif_tone()),
                s => s,
            };
            if HAS_AUDIO.with(|a| a.get()) {
                crate::feedback::play(sound, sys.get_volume());
            } else {
                // No sound card on this phone yet: at least feel it
                crate::feedback::vibrate(vibrate_ms.max(120));
            }
            if matches!(sound, crate::feedback::Sound::Ring(_)) && sys.get_vibrate_on_ring() {
                crate::feedback::vibrate(vibrate_ms);
            }
        }
        1 => crate::feedback::vibrate(vibrate_ms),
        _ => {}
    }
}

thread_local! {
    static TOAST: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

/// Messaggio breve in basso (snackbar)
pub fn toast(ui: &AppWindow, text: &str) {
    let sh = ui.global::<Shell>();
    sh.set_toast(text.into());
    sh.set_toast_shown(true);
    let t = slint::Timer::default();
    t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(2800), || {
        if let Some(ui) = ui_handle() {
            ui.global::<Shell>().set_toast_shown(false);
        }
    });
    TOAST.with(|x| *x.borrow_mut() = Some(t));
}

/// Squillo continuo per la chiamata in arrivo
pub fn start_ring(st: &mut State) {
    if st.ring_timer.is_some() {
        return;
    }
    let tick = || {
        if let Some(ui) = ui_handle() {
            let tone = ui.global::<Sys>().get_ringtone();
            alert(&ui, crate::feedback::Sound::Ring(tone), 900);
        }
    };
    tick();
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(crate::feedback::RING_CYCLE_MS), tick);
    st.ring_timer = Some(t);
}

pub fn stop_ring(st: &mut State) {
    st.ring_timer = None;
}

fn show_hud(ui: &AppWindow, st: &mut State) {
    ui.global::<Shell>().set_volume_hud(true);
    let t = slint::Timer::default();
    t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(2200), || {
        if let Some(ui) = ui_handle() {
            ui.global::<Shell>().set_volume_hud(false);
        }
    });
    st.hud_timer = Some(t);
}

fn volume_step(ui: &AppWindow, st: &mut State, dir: i32) {
    let sys = ui.global::<Sys>();
    let v = (sys.get_volume() + dir as f32 / 15.0).clamp(0.0, 1.0);
    sys.set_volume(v);
    if st.live {
        sys_set_volume(v);
    }
    // Come su Android: a zero si passa in vibrazione, risalendo torna lo squillo
    if v <= 0.001 && sys.get_ringer() == 0 {
        sys.set_ringer(1);
        crate::feedback::vibrate(120);
    } else if v > 0.001 && dir > 0 && sys.get_ringer() != 0 {
        sys.set_ringer(0);
    }
    show_hud(ui, st);
}

fn sys_set_volume(v: f32) {
    sys::set_volume(v);
}

/// Tasti fisici: arrivano dal thread evdev tramite il ciclo eventi
pub fn on_key(k: crate::keys::Key) {
    let (Some(ui), Some(st)) = (ui_handle(), shared()) else { return };
    let mut s = st.borrow_mut();
    match k {
        crate::keys::Key::VolumeUp => volume_step(&ui, &mut s, 1),
        crate::keys::Key::VolumeDown => volume_step(&ui, &mut s, -1),
        crate::keys::Key::PowerLong => {
            if s.ring_timer.is_some() {
                stop_ring(&mut s);
                return;
            }
            if s.screen_off {
                s.screen_off = false;
                ui.global::<Shell>().set_screen_off(false);
                if s.live { sys::set_screen(true); }
            }
            ui.global::<Shell>().set_power_menu(true);
        }
        crate::keys::Key::Power => {
            // Durante uno squillo il tasto accensione silenzia, come sui telefoni veri
            if s.ring_timer.is_some() {
                stop_ring(&mut s);
                return;
            }
            s.screen_off = !s.screen_off;
            if !s.screen_off {
                crate::keys::touch_activity();
            }
            let g = ui.global::<Shell>();
            if s.screen_off {
                g.set_controls_open(false);
                g.set_screen(ScreenId::Lock);
            }
            g.set_screen_off(s.screen_off);
            g.set_power_menu(false);
            if g.get_screen_off() == s.screen_off && ui.global::<Sys>().get_snd_lock() && ui.global::<Sys>().get_ringer() == 0 {
                crate::feedback::play(if s.screen_off { crate::feedback::Sound::Lock } else { crate::feedback::Sound::Unlock }, 0.4);
            }
            if s.live {
                sys::set_screen(!s.screen_off);
            }
        }
    }
}

// ---------- refresh verso la UI ----------

thread_local! {
    static H24: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

pub fn set_h24(on: bool) {
    H24.with(|h| h.set(on));
    LAST_MIN.with(|m| m.set(-1));
    if let Some(ui) = ui_handle() {
        push_clock(&ui, Local::now().naive_local());
    }
}

thread_local! {
    static LAST_MIN: std::cell::Cell<i64> = const { std::cell::Cell::new(-1) };
}

pub fn push_clock(ui: &AppWindow, now: NaiveDateTime) {
    // Redraw the clock texts only when the minute changes, not every second
    let key = now.and_utc().timestamp() / 60 + if H24.with(|h| h.get()) { 0 } else { 1 << 40 };
    if LAST_MIN.with(|m| m.replace(key)) == key {
        return;
    }
    let s = ui.global::<Sys>();
    let hour = if H24.with(|h| h.get()) { now.hour() } else { (now.hour() + 11) % 12 + 1 };
    s.set_time(format!("{:02}:{:02}", hour, now.minute()).into());
    s.set_hours(format!("{:02}", hour).into());
    s.set_minutes(format!("{:02}", now.minute()).into());
    s.set_date(cal::long_label(now.date()).into());
}

/// Hardware state read on a background thread: process spawns (waydroid,
/// mmcli) and sysfs scans never block the UI thread, which on the Lumia
/// has a single slow core to itself
#[derive(Clone, Default)]
pub struct Hw {
    pub battery: Option<(i32, bool)>,
    pub has_wifi: bool,
    pub has_cell: bool,
    pub diag: String,
    pub mem: (i32, i32),
    pub mem_android: i32,
    pub android: (bool, String),
    pub modem: Option<crate::modem::Status>,
    /// (temperature, description) from open-meteo, refreshed every 30 minutes
    pub weather: Option<(String, String)>,
    pub disk: (String, f32),
}

static HW: std::sync::Mutex<Option<Hw>> = std::sync::Mutex::new(None);
type Slow = (Option<(bool, String)>, Option<crate::modem::Status>);
static SLOW: std::sync::Mutex<Slow> = std::sync::Mutex::new((None, None));
static WEATHER_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WEATHER_CITY: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn hw_snapshot() -> Option<Hw> {
    HW.lock().ok().and_then(|h| h.clone())
}

fn read_hw(last_ticks: &mut Option<u64>, secs: f32, waydroid_tick: bool, prev: &Option<Hw>) -> Hw {
    let now = sys::self_ticks();
    let cpu = match (now, *last_ticks) {
        (Some(a), Some(b)) => Some((a.saturating_sub(b)) as f32 / secs),
        _ => None,
    };
    *last_ticks = now;
    let (used, total) = sys::memory();
    let _ = (waydroid_tick, prev);
    // Slow probes (waydroid is a Python program, mmcli can wait on D-Bus) live
    // in their own thread: battery and the rest never wait for them
    let (android, modem) = SLOW.lock().map(|s| s.clone()).unwrap_or_default();
    let android = android.unwrap_or((false, "Checking".into()));
    let mem_android = if android.0 { sys::android_memory().min(used) } else { 0 };
    Hw {
        battery: sys::battery(),
        has_wifi: sys::has_wifi(),
        has_cell: sys::has_cell(),
        diag: sys::diag_line(cpu),
        mem: (used, total),
        mem_android,
        android,
        modem,
        weather: prev.as_ref().and_then(|p| p.weather.clone()),
        disk: disk_usage(),
    }
}

#[cfg(not(feature = "native"))]
fn disk_usage() -> (String, f32) { (String::new(), 0.0) }

#[cfg(feature = "native")]
fn disk_usage() -> (String, f32) {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let path = std::ffi::CString::new("/").unwrap();
    if unsafe { libc::statvfs(path.as_ptr(), &mut st) } != 0 {
        return (String::new(), 0.0);
    }
    let total = st.f_blocks as f64 * st.f_frsize as f64;
    let free = st.f_bavail as f64 * st.f_frsize as f64;
    let gb = |b: f64| b / 1_073_741_824.0;
    (format!("{:.1} / {:.1} GB", gb(total - free), gb(total)), ((total - free) / total.max(1.0)) as f32)
}

fn weather_text(code: i64) -> &'static str {
    match code {
        0 => "Clear",
        1 | 2 => "Partly cloudy",
        3 => "Cloudy",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 | 80..=82 => "Rain",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "",
    }
}

#[cfg(not(feature = "native"))]
fn fetch_weather(_city: &str) -> Option<(String, String)> { None }

/// open-meteo: free, no key. City name to coordinates, then current weather
#[cfg(feature = "native")]
fn fetch_weather(city: &str) -> Option<(String, String)> {
    let get = |url: &str| -> Option<serde_json::Value> {
        ureq::get(url).timeout(std::time::Duration::from_secs(10)).call().ok()?.into_json().ok()
    };
    let q: String = city.chars().map(|c| if c == ' ' { '+' } else { c }).filter(|c| c.is_alphanumeric() || *c == '+' || *c == '-').collect();
    let g = get(&format!("https://geocoding-api.open-meteo.com/v1/search?name={q}&count=1"))?;
    let r = g.get("results")?.get(0)?;
    let (lat, lon) = (r.get("latitude")?.as_f64()?, r.get("longitude")?.as_f64()?);
    let w = get(&format!("https://api.open-meteo.com/v1/forecast?latitude={lat:.3}&longitude={lon:.3}&current=temperature_2m,weather_code"))?;
    let c = w.get("current")?;
    let t = c.get("temperature_2m")?.as_f64()?;
    let code = c.get("weather_code").and_then(|v| v.as_i64()).unwrap_or(-1);
    Some((format!("{:.0}°C", t), weather_text(code).to_string()))
}

/// Starts the background reader (call once, on the phone)
pub fn start_hw_thread() {
    std::thread::spawn(|| loop {
        let android = if !sys::waydroid_installed() {
            (false, "Waydroid not installed".to_string())
        } else if sys::waydroid_running() {
            (true, "Running, suspends after 2 minutes with no apps".to_string())
        } else {
            (false, "Suspended, starts when you open an Android app".to_string())
        };
        if let Ok(mut s) = SLOW.lock() {
            s.0 = Some(android);
        }
        let modem = crate::modem::status();
        if let Ok(mut s) = SLOW.lock() {
            s.1 = Some(modem);
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
    });
    std::thread::spawn(|| {
        let mut ticks = None;
        let mut n: u32 = 0;
        loop {
            let prev = hw_snapshot();
            // A panic in one reading must not stop the battery and the rest
            let Ok(mut hw) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_hw(&mut ticks, 5.0, n % 6 == 0, &prev))) else {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            };
            // Weather every 30 minutes (every 360 rounds of 5 s), or right after a city change
            let city = WEATHER_CITY.lock().map(|c| c.clone()).unwrap_or_default();
            let force = n % 360 == 0 || hw.weather.is_none() && n % 12 == 0;
            if WEATHER_ON.load(std::sync::atomic::Ordering::Relaxed) && !city.is_empty() && force {
                if let Some(w) = fetch_weather(&city) {
                    hw.weather = Some(w);
                }
            }
            if let Ok(mut g) = HW.lock() {
                *g = Some(hw);
            }
            n += 1;
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });
}

thread_local! {
    static PREV_CHARGING: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static LOW_WARNED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CHARGE_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    pub static HAS_AUDIO: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

fn push_hw(ui: &AppWindow) {
    let Some(hw) = hw_snapshot() else { return };
    let s = ui.global::<Sys>();
    if let Some((_, ch)) = hw.battery {
        // Charger plugged in: short rising sound
        if PREV_CHARGING.with(|p| p.replace(Some(ch))) == Some(false) && ch {
            if s.get_snd_charge() && s.get_ringer() == 0 {
                if HAS_AUDIO.with(|a| a.get()) {
                    crate::feedback::play(crate::feedback::Sound::Charge, s.get_volume().max(0.3));
                } else {
                    crate::feedback::vibrate(150);
                }
            }
            // Wake the screen for a moment (even from off) and show the charging animation
            let shell = ui.global::<Shell>();
            let was_off = shell.get_screen_off();
            shell.set_screen_off(false);
            shell.set_charge_anim(true);
            let t = slint::Timer::default();
            t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(2600), move || {
                if let Some(ui) = ui_handle() {
                    let shell = ui.global::<Shell>();
                    shell.set_charge_anim(false);
                    if was_off {
                        shell.set_screen_off(true);
                    }
                }
            });
            CHARGE_TIMER.with(|c| *c.borrow_mut() = Some(t));
        }
    }
    if let Some((level, ch)) = hw.battery.filter(|b| b.0 >= 0) {
        if ch || level > 20 {
            LOW_WARNED.with(|w| w.set(false));
        } else if level <= 15 && !LOW_WARNED.with(|w| w.replace(true)) {
            if let Some(st) = shared() {
                if let Ok(mut st) = st.try_borrow_mut() {
                    let snd = if s.get_snd_lowbat() { crate::feedback::Sound::LowBattery } else { crate::feedback::Sound::Notification };
                    post(ui, &mut st, "settings", "Battery low", &format!("{level}% left. Connect the charger."), snd);
                }
            }
        }
    }
    match hw.battery {
        Some((b, ch)) if b >= 0 => {
            s.set_has_battery(true);
            s.set_battery(b);
            s.set_charging(ch);
        }
        // Charger known, level unknown (no voltage reading yet)
        Some((_, ch)) => {
            s.set_has_battery(false);
            s.set_charging(ch);
        }
        None => s.set_has_battery(false),
    }
    s.set_has_wifi(hw.has_wifi);
    s.set_disk_used(hw.disk.0.into());
    s.set_disk_frac(hw.disk.1);
    match &hw.weather {
        Some((t, d)) => {
            s.set_weather_temp(t.into());
            s.set_weather_desc(d.into());
        }
        None => {
            s.set_weather_temp("".into());
            s.set_weather_desc("".into());
        }
    }
    s.set_has_cell(hw.has_cell);
    s.set_diag(hw.diag.clone().into());
    let mut lines: Vec<SharedString> = hw.diag.split(" | ").map(|l| l.into()).collect();
    if let Ok(k) = std::fs::read_to_string("/var/log/cheip-smp.txt") {
        if !k.trim().is_empty() {
            lines.push(format!("kernel: {}", k.trim()).into());
        }
    }
    if let Ok(f) = std::fs::read_to_string("/var/log/cheip-firmware.txt") {
        for l in f.lines().filter(|l| !l.trim().is_empty()).take(30) {
            lines.push(format!("wifi: {l}").into());
        }
    }
    if let Ok(g) = std::fs::read_to_string("/var/log/cheip-grow.txt") {
        if let Some(l) = g.lines().last() {
            lines.push(format!("storage grow: {l}").into());
        }
    }
    s.set_log_lines(model(lines));
    s.set_mem_used(hw.mem.0);
    s.set_mem_total(hw.mem.1);
    s.set_mem_android(hw.mem_android);
    s.set_android_running(hw.android.0);
    s.set_android_state(hw.android.1.into());
}

pub fn push_toggles(ui: &AppWindow, st: &State) {
    let v = st
        .toggles
        .iter()
        .map(|t| ToggleItem { id: t.id.into(), label: t.label.into(), glyph: glyph(ui, t.id), on: t.on })
        .collect();
    let s = ui.global::<Sys>();
    s.set_toggles(model(v));
    s.set_wifi_on(st.toggle_on("wifi"));
    s.set_data_on(st.toggle_on("dati"));
    s.set_bt_on(st.toggle_on("bt"));
    s.set_eco_on(st.toggle_on("eco"));
}

fn badge_for(st: &State, id: &str) -> i32 {
    match id {
        "messages" => st.store.threads().iter().map(|t| t.2 as i32).sum(),
        "phone" => st.missed,
        _ => 0,
    }
}

fn entry(ui: &AppWindow, st: &State, id: &str, name: &str, android: bool, icon: Option<PathBuf>) -> AppEntry {
    let picture = icon.as_deref().and_then(|p| Image::load_from_path(p).ok());
    AppEntry {
        id: id.into(),
        name: name.into(),
        initial: initial(name),
        android,
        glyph: if android { SharedString::new() } else { glyph(ui, id) },
        has_picture: picture.is_some(),
        picture: picture.unwrap_or_default(),
        badge: badge_for(st, id),
        tone: crate::style::app_tone(id).0,
        tone_on: crate::style::app_tone(id).1,
    }
}

thread_local! {
    static DEMO_ANDROID: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

/// Aggiunge un'app Android finta (installatore APK nella demo)
pub fn add_demo_android(id: &str, name: &str) {
    DEMO_ANDROID.with(|d| {
        let mut d = d.borrow_mut();
        if !d.iter().any(|(i, _)| i == id) {
            d.push((id.to_string(), name.to_string()));
        }
    });
}

/// App Android finte per demo e screenshot
pub fn set_demo_android(v: Vec<(String, String)>) {
    DEMO_ANDROID.with(|d| *d.borrow_mut() = v);
}

/// Tutte le app: (id, nome, android, icona)
pub fn catalog() -> Vec<(String, String, bool, Option<PathBuf>)> {
    let mut v: Vec<_> = SYSTEM_APPS.iter().map(|(id, n, _)| (id.to_string(), n.to_string(), false, None)).collect();
    DEMO_ANDROID.with(|d| {
        for (id, n) in d.borrow().iter() {
            v.push((id.clone(), n.clone(), true, None));
        }
    });
    for (pkg, name, icon) in sys::android_apps() {
        v.push((pkg, name, true, icon));
    }
    v
}

/// App della griglia nell'ordine scelto dall'utente (le nuove in coda), senza quelle del dock
pub fn grid_apps(st: &State) -> Vec<(String, String, bool, Option<PathBuf>)> {
    let mut all: Vec<_> = catalog().into_iter().filter(|(id, _, _, _)| !st.store.data.dock.contains(id)).collect();
    let order = &st.store.data.app_order;
    all.sort_by_key(|(id, _, _, _)| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    all
}

pub fn push_apps(ui: &AppWindow, st: &State) {
    let q = st.search.to_lowercase();
    let grid: Vec<AppEntry> = if q.is_empty() {
        grid_apps(st).into_iter().map(|(id, n, a, icon)| entry(ui, st, &id, &n, a, icon)).collect()
    } else {
        catalog().into_iter().filter(|(_, n, _, _)| n.to_lowercase().contains(&q)).map(|(id, n, a, icon)| entry(ui, st, &id, &n, a, icon)).collect()
    };
    let cat = catalog();
    let dock = st
        .store
        .data
        .dock
        .iter()
        .filter_map(|id| cat.iter().find(|(c, _, _, _)| c == id))
        .map(|(id, n, a, icon)| entry(ui, st, id, n, *a, icon.clone()))
        .collect::<Vec<_>>();
    let g = ui.global::<Shell>();
    g.set_dock_full(dock.len() >= crate::store::DOCK_MAX);
    g.set_apps(model(grid));
    g.set_dock(model(dock));
}

fn remove_demo_android(id: &str) {
    DEMO_ANDROID.with(|d| d.borrow_mut().retain(|(x, _)| x != id));
}

pub fn push_recents(ui: &AppWindow, st: &State) {
    let v = st
        .recents
        .iter()
        .enumerate()
        .map(|(i, r)| RecentItem {
            id: r.id.clone().into(),
            name: r.name.clone().into(),
            glyph: glyph(ui, &r.id),
            mem: 0,
            state: (if i == 0 { "last opened" } else if r.android { "Android, paused" } else { "paused" }).into(),
            android: r.android,
        })
        .collect();
    ui.global::<Shell>().set_recents(model(v));
}

pub fn push_notifs(ui: &AppWindow, st: &State) {
    ui.global::<Sys>().set_notifications(model(st.notifs.clone()));
}

fn push_calendar(ui: &AppWindow, st: &State) {
    let (y, m) = (st.cal_year, st.cal_month);
    let first = cal::first_column(y, m);
    let days = cal::days_in_month(y, m);
    let sel = st.cal_sel.min(days);
    let cells = (1..=days)
        .map(|d| {
            let slot = first + d - 1;
            DayCell {
                n: d as i32,
                col: (slot % 7) as i32,
                row: (slot / 7) as i32,
                in_month: true,
                today: NaiveDate::from_ymd_opt(y, m, d) == Some(st.today),
                selected: d == sel,
                has_events: st.store.has_events(&cal::iso(y, m, d)),
            }
        })
        .collect();
    let c = ui.global::<Cal>();
    c.set_month_label(format!("{} {}", cal::MONTHS[(m - 1) as usize], y).into());
    c.set_cells(model(cells));
    if let Some(date) = NaiveDate::from_ymd_opt(y, m, sel) {
        c.set_selected_label(cal::long_label(date).into());
    }
    let events = st
        .store
        .events_on(&cal::iso(y, m, sel))
        .into_iter()
        .map(|e| EventItem { time: e.time.into(), title: e.title.into() })
        .collect();
    c.set_events(model(events));
}

fn push_notes(ui: &AppWindow, st: &State) {
    let list = st.store.sorted_notes(st.note_filter == 1);
    let rows = list.len().div_ceil(2) as i32;
    let v = list
        .into_iter()
        .enumerate()
        .map(|(i, n)| NoteItem {
            id: n.id,
            title: n.title.into(),
            body: n.body.into(),
            pinned: n.pinned,
            col: (i % 2) as i32,
            row: (i / 2) as i32,
        })
        .collect();
    let g = ui.global::<Notes>();
    g.set_notes(model(v));
    g.set_rows(rows);
    g.set_filter(st.note_filter);
}

pub fn push_all(ui: &AppWindow, st: &State) {
    push_toggles(ui, st);
    push_apps(ui, st);
    push_recents(ui, st);
    push_calendar(ui, st);
    push_notes(ui, st);
    push_notifs(ui, st);
    crate::ctl_comm::push(ui, st);
    crate::ctl_web::push(ui, st);
    crate::ctl_files::push(ui, st);
}

// ---------- OTA ----------

fn check_update(ui: &AppWindow, live: bool) {
    let s = ui.global::<Sys>();
    if !live {
        s.set_update_available(false);
        s.set_update_state("You're up to date".into());
        return;
    }
    s.set_update_state("Checking…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let res = update::fetch(&update::manifest_url());
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.global::<Sys>();
            match res {
                Ok(m) if update::is_newer(&m.version, VERSION) && update::supports_device(&m) => {
                    s.set_update_available(true);
                    s.set_update_version(m.version.clone().into());
                    let note = if m.notes.is_empty() { "Ready to install".to_string() } else { m.notes.clone() };
                    s.set_update_state(note.into());
                }
                Ok(_) => {
                    s.set_update_available(false);
                    s.set_update_state("You're up to date".into());
                }
                Err(_) => s.set_update_state("Server unreachable".into()),
            }
        });
    });
}

fn install_update(ui: &AppWindow, live: bool) {
    if !live {
        let s = ui.global::<Sys>();
        s.set_update_available(false);
        s.set_update_version("".into());
        s.set_update_state("Installed. This is a demo, nothing changed".into());
        return;
    }
    ui.global::<Sys>().set_update_state("Downloading and installing…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let ok = sys::helper(&["update"]);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.global::<Sys>();
            if ok {
                s.set_update_available(false);
                s.set_update_state("Installed, restarting the shell".into());
            } else {
                s.set_update_state("Install failed, try again".into());
            }
        });
    });
}

// ---------- collegamento ----------

/// `live = false` per screenshot e test: niente timer, niente comandi di sistema.
fn set_pref_ui(ui: &AppWindow, k: &str, v: i32) {
    let s = ui.global::<Sys>();
    match k {
        "notif_tone" => s.set_notif_tone(v),
        "snd_charge" => s.set_snd_charge(v != 0),
        "snd_lowbat" => s.set_snd_lowbat(v != 0),
        "snd_lock" => s.set_snd_lock(v != 0),
        "kbd_vibrate" => s.set_kbd_vibrate(v != 0),
        "nav_buttons" => s.set_nav_buttons(v != 0),
        "w_show" => s.set_w_show(v != 0),
        "w_ram" => s.set_w_ram(v != 0),
        "w_bat" => s.set_w_bat(v != 0),
        "w_disk" => s.set_w_disk(v != 0),
        "w_weather" => {
            s.set_w_weather(v != 0);
            WEATHER_ON.store(v != 0, std::sync::atomic::Ordering::Relaxed);
        }
        _ => {}
    }
}

/// Lite rendering: forced on with software rendering, or chosen in Accessibility
pub fn set_lite_pref(ui: &AppWindow, on: bool) {
    let software = std::env::var("SLINT_BACKEND").map(|b| b.contains("software")).unwrap_or(false);
    ui.global::<Theme>().set_lite(on || software);
}

pub fn apply_display_size(ui: &AppWindow, i: i32) {
    let scale = [1.75f32, 2.0, 2.3][i.clamp(0, 2) as usize];
    ui.global::<Sys>().set_display_size(i.clamp(0, 2));
    ui.window().dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: scale });
}

const TIMEOUTS: [u64; 6] = [15, 30, 60, 120, 300, 0];

/// Every second: turn the screen off after the chosen time without touches
fn screen_timeout_tick(ui: &AppWindow) {
    let Some(st) = shared() else { return };
    let idx = ui.global::<Sys>().get_screen_timeout().clamp(0, 5) as usize;
    let limit = TIMEOUTS[idx];
    if limit == 0 {
        return;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let last = crate::keys::LAST_ACTIVITY.load(std::sync::atomic::Ordering::Relaxed);
    let busy = {
        let s = st.borrow();
        s.screen_off || s.ring_timer.is_some() || s.call.is_some()
    };
    if !busy && last > 0 && now.saturating_sub(last) >= limit {
        on_key(crate::keys::Key::Power);
    }
}

pub fn wire(ui: &AppWindow, store: Store, now: NaiveDateTime, live: bool) -> Shared {
    let st: Shared = Rc::new(RefCell::new(State::new(store, now, live)));
    UI.with(|u| *u.borrow_mut() = Some(ui.as_weak()));
    STATE.with(|s| *s.borrow_mut() = Some(st.clone()));
    {
        if live {
            let s = ui.global::<Sys>();
            s.set_has_backlight(sys::has_backlight());
            HAS_AUDIO.with(|a| a.set(sys::has_audio()));
            s.set_has_audio(sys::has_audio());
            // Design defaults (78%, Wi-Fi on) are for the demo: on the phone show nothing until read
            s.set_has_battery(false);
            s.set_has_wifi(false);
            s.set_has_cell(false);
        }
        // Simple int/bool settings saved by name
        let w = ui.as_weak();
        ui.global::<Sys>().on_pref(move |k, v| {
            let Some(ui) = w.upgrade() else { return };
            set_pref_ui(&ui, k.as_str(), v);
            if let Some(st) = shared() {
                let mut s = st.borrow_mut();
                s.store.data.prefs.insert(format!("p_{k}"), v.to_string());
                s.store.save();
            }
        });
        let w = ui.as_weak();
        ui.global::<Sys>().on_set_weather_city(move |c| {
            let c = c.trim().to_string();
            if let Ok(mut g) = WEATHER_CITY.lock() {
                *g = c.clone();
            }
            if let Ok(mut h) = HW.lock() {
                if let Some(h) = h.as_mut() { h.weather = None; }
            }
            if let Some(ui) = w.upgrade() {
                ui.global::<Sys>().set_weather_city(c.clone().into());
            }
            if let Some(st) = shared() {
                let mut s = st.borrow_mut();
                s.store.data.prefs.insert("weather_city".into(), c);
                s.store.save();
            }
        });
        ui.global::<Sys>().on_preview_tone(|i| crate::feedback::play(crate::feedback::Sound::Tone(i), 0.7));
        let w = ui.as_weak();
        ui.global::<Kbd>().on_click(move || {
            if let Some(ui) = w.upgrade() {
                if ui.global::<Sys>().get_kbd_vibrate() {
                    crate::feedback::vibrate(14);
                }
            }
        });
        // Display size: live change of the scale factor, saved in prefs
        let w = ui.as_weak();
        ui.global::<Sys>().on_set_display_size(move |i| {
            let Some(ui) = w.upgrade() else { return };
            apply_display_size(&ui, i);
            if let Some(st) = shared() {
                let mut s = st.borrow_mut();
                s.store.data.prefs.insert("display_size".into(), i.to_string());
                s.store.save();
            }
        });
        let w = ui.as_weak();
        ui.global::<Sys>().on_set_screen_timeout(move |i| {
            let Some(ui) = w.upgrade() else { return };
            ui.global::<Sys>().set_screen_timeout(i);
            crate::keys::touch_activity();
            if let Some(st) = shared() {
                let mut s = st.borrow_mut();
                s.store.data.prefs.insert("screen_timeout".into(), i.to_string());
                s.store.save();
            }
        });
        // On-screen keyboard: every key becomes a real key event for the focused field
        let w = ui.as_weak();
        ui.global::<Kbd>().on_key(move |t| {
            if let Some(ui) = w.upgrade() {
                use slint::platform::WindowEvent;
                ui.window().dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
                ui.window().dispatch_event(WindowEvent::KeyReleased { text: t });
            }
        });
    }
    {
        let s = ui.global::<Sys>();
        s.set_version(VERSION.into());
        if live {
            s.set_model(sys::model().into());
            s.set_kernel(sys::kernel().into());
            if let Some(b) = sys::brightness() {
                s.set_brightness(b);
            }
            if let Some(v) = sys::volume() {
                s.set_volume(v);
            }
        }
        s.set_update_state("Tap Check to look for updates".into());
    }
    push_clock(ui, now);
    if live {
        push_hw(ui);
        st.borrow_mut().entries = crate::files::list(&crate::files::home(), false);
    }
    push_all(ui, &st.borrow());

    // Sistema
    let sysg = ui.global::<Sys>();
    sysg.on_toggle(with!(ui, st, |ui, s, id| {
        let live = s.live;
        if let Some(t) = s.toggles.iter_mut().find(|t| t.id == id.as_str()) {
            t.on = !t.on;
            if live {
                match t.id {
                    "wifi" => sys::radio("wifi", t.on),
                    "dati" => sys::radio("wwan", t.on),
                    "bt" => sys::radio("bluetooth", t.on),
                    "torcia" => sys::set_torch(t.on),
                    "eco" => {
                        sys::helper(&["governor", if t.on { "powersave" } else { "schedutil" }]);
                    }
                    "airplane" => {
                        sys::radio("all", !t.on);
                    }
                    _ => {}
                }
            }
        }
        if id.as_str() == "dark" {
            let on = s.toggle_on("dark");
            s.store.data.style.theme_mode = if on { 0 } else { 1 };
            s.store.save();
            crate::ctl_style::apply_from(&ui, &s);
        }
        if id.as_str() == "alarm" {
            drop(s);
            ui.global::<Shell>().set_controls_open(false);
            ui.global::<Shell>().invoke_open_app("clock".into(), 180.0, 60.0, 56.0);
            return;
        }
        push_toggles(&ui, &s);
    }));
    sysg.on_brightness_changed(with!(ui, st, |_ui, s, v| {
        if s.live {
            sys::set_brightness(v);
        }
    }));
    sysg.on_volume_changed(with!(ui, st, |_ui, s, v| {
        if s.live {
            sys::set_volume(v);
        }
    }));
    sysg.on_check_update(with!(ui, st, |ui, s| { check_update(&ui, s.live); }));
    sysg.on_install_update(with!(ui, st, |ui, s| { install_update(&ui, s.live); }));
    sysg.on_android_toggle(with!(ui, st, |ui, s| {
        if !s.live || !sys::waydroid_installed() {
            return;
        }
        let start = !ui.global::<Sys>().get_android_running();
        sys::helper(&["android", if start { "start" } else { "stop" }]);
        push_hw(&ui);
    }));

    // Shell
    let shell = ui.global::<Shell>();
    shell.on_open_app(with!(ui, st, |ui, s, id, x, y, size| {
        let Some((_, name, android, _)) = catalog().into_iter().find(|(a, _, _, _)| a == id.as_str()) else { return };
        s.recents.retain(|r| r.id != id.as_str());
        s.recents.insert(0, Recent { id: id.to_string(), name, android });
        if android && s.live {
            sys::run("waydroid", &["app", "launch", id.as_str()]);
        }
        if id.as_str() == "phone" {
            s.missed = 0;
            push_apps(&ui, &s);
        }
        if id.as_str() == "settings" {
            ui.global::<Settings>().set_page("".into());
        }
        let g = ui.global::<Shell>();
        g.set_edit_mode(false);
        g.set_from_x(x);
        g.set_from_y(y);
        g.set_from_size(size);
        g.set_app_id(id);
        g.set_controls_open(false);
        g.set_searching(false);
        g.set_screen(ScreenId::Home);
        g.set_app_open(true);
        push_recents(&ui, &s);
    }));
    shell.on_close_app(with!(ui, st, |ui, _s| { ui.global::<Shell>().set_app_open(false); }));
    shell.on_open_notification(with!(ui, st, |ui, s, id| {
        let Some(n) = s.notifs.iter().find(|n| n.id == id).cloned() else { return };
        s.notifs.retain(|x| x.id != id);
        push_notifs(&ui, &s);
        drop(s);
        let g = ui.global::<Shell>();
        g.set_headsup_shown(false);
        g.set_screen(ScreenId::Home);
        if !n.app_id.is_empty() && n.app_id != "system" {
            let w = ui.window().size().to_logical(ui.window().scale_factor());
            g.invoke_open_app(n.app_id.clone(), w.width / 2.0 - 28.0, 60.0, 56.0);
        } else if n.app_id == "system" {
            let w = ui.window().size().to_logical(ui.window().scale_factor());
            g.invoke_open_app("settings".into(), w.width / 2.0 - 28.0, 60.0, 56.0);
            ui.global::<Settings>().set_page("update".into());
        }
    }));
    shell.on_dismiss_notification(with!(ui, st, |ui, s, id| {
        s.notifs.retain(|x| x.id != id);
        push_notifs(&ui, &s);
    }));
    shell.on_clear_notifications(with!(ui, st, |ui, s| {
        s.notifs.clear();
        push_notifs(&ui, &s);
    }));
    shell.on_volume_step(with!(ui, st, |ui, s, d| { volume_step(&ui, &mut s, d); }));
    shell.on_hud_touch(with!(ui, st, |ui, s| { show_hud(&ui, &mut s); }));
    shell.on_demo_call(with!(ui, st, |_ui, _s| {
        let t = slint::Timer::default();
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(2000), || {
            let (Some(ui), Some(st)) = (ui_handle(), shared()) else { return };
            let mut s = st.borrow_mut();
            crate::ctl_comm::simulate_incoming(&ui, &mut s, "+393475567788");
        });
        std::mem::forget(t);
    }));
    shell.on_demo_message(with!(ui, st, |_ui, _s| {
        let t = slint::Timer::default();
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(2000), || {
            let (Some(ui), Some(st)) = (ui_handle(), shared()) else { return };
            let mut s = st.borrow_mut();
            crate::ctl_comm::simulate_message(&ui, &mut s, "+393331234567", "Are you on the Lumia yet? Send me a screenshot");
        });
        std::mem::forget(t);
    }));
    let sysg2 = ui.global::<Sys>();
    sysg2.on_set_ringer(with!(ui, st, |ui, s, m| {
        ui.global::<Sys>().set_ringer(m);
        if m == 1 {
            crate::feedback::vibrate(160);
        }
        if ui.global::<Shell>().get_volume_hud() {
            show_hud(&ui, &mut s);
        }
    }));
    sysg2.on_set_ringtone(with!(ui, st, |ui, _s, t| {
        let sys = ui.global::<Sys>();
        sys.set_ringtone(t);
        crate::feedback::play(crate::feedback::Sound::Ring(t), sys.get_volume());
    }));
    shell.on_move_app(with!(ui, st, |ui, s, from, to| {
        if !s.search.is_empty() {
            return;
        }
        let mut ids: Vec<String> = grid_apps(&s).into_iter().map(|(id, _, _, _)| id).collect();
        crate::store::move_item(&mut ids, from as usize, to as usize);
        s.store.data.app_order = ids;
        s.store.save();
        push_apps(&ui, &s);
    }));
    shell.on_pin(with!(ui, st, |ui, s, id| {
        if s.store.data.dock.len() < crate::store::DOCK_MAX && !s.store.data.dock.iter().any(|d| d == id.as_str()) {
            s.store.data.dock.push(id.to_string());
            s.store.save();
        }
        push_apps(&ui, &s);
    }));
    shell.on_unpin(with!(ui, st, |ui, s, id| {
        s.store.data.dock.retain(|d| d != id.as_str());
        if !s.store.data.app_order.iter().any(|o| o == id.as_str()) {
            s.store.data.app_order.push(id.to_string());
        }
        s.store.save();
        push_apps(&ui, &s);
    }));
    shell.on_app_info(with!(ui, st, |ui, s, id| {
        let Some((_, name, android, _)) = catalog().into_iter().find(|(a, _, _, _)| a == id.as_str()) else { return };
        let set = ui.global::<Settings>();
        set.set_info_id(id.clone());
        set.set_info_name(name.into());
        set.set_info_android(android);
        set.set_info_kind((if android { "Android app" } else { "System app" }).into());
        set.set_info_glyph(glyph(&ui, &id));
        drop(s);
        let w = ui.window().size().to_logical(ui.window().scale_factor());
        ui.global::<Shell>().invoke_open_app("settings".into(), w.width / 2.0 - 28.0, w.height / 2.0 - 28.0, 56.0);
        ui.global::<Settings>().invoke_open_page("appinfo".into());
    }));
    shell.on_uninstall(with!(ui, st, |ui, s, id| {
        if s.live {
            sys::run("waydroid", &["app", "remove", id.as_str()]);
        }
        remove_demo_android(&id);
        s.store.data.app_order.retain(|o| o != id.as_str());
        s.store.data.dock.retain(|d| d != id.as_str());
        s.recents.retain(|r| r.id != id.as_str());
        s.store.save();
        push_apps(&ui, &s);
        push_recents(&ui, &s);
        let set = ui.global::<Settings>();
        if set.get_info_id() == id {
            set.set_page("".into());
            ui.global::<Shell>().set_app_open(false);
        }
    }));
    shell.on_power_action(|id| {
        match id.as_str() {
            "off" => { sys::helper(&["power", "off"]); }
            "reboot" => { sys::helper(&["power", "reboot"]); }
            _ => on_key(crate::keys::Key::Power),
        }
    });
    shell.on_unlock(with!(ui, st, |ui, _s| { ui.global::<Shell>().set_screen(ScreenId::Home); }));
    shell.on_search(with!(ui, st, |ui, s, q| {
        s.search = q.to_string();
        push_apps(&ui, &s);
    }));
    shell.on_close_recent(with!(ui, st, |ui, s, id| {
        if s.live && s.recents.iter().any(|r| r.id == id.as_str() && r.android) {
            sys::run("waydroid", &["app", "stop", id.as_str()]);
        }
        s.recents.retain(|r| r.id != id.as_str());
        push_recents(&ui, &s);
    }));
    shell.on_close_all(with!(ui, st, |ui, s| {
        if s.live {
            for r in s.recents.iter().filter(|r| r.android) {
                sys::run("waydroid", &["app", "stop", &r.id]);
            }
        }
        s.recents.clear();
        push_recents(&ui, &s);
        ui.global::<Shell>().set_screen(ScreenId::Home);
    }));

    // Calendario
    let calg = ui.global::<Cal>();
    calg.on_prev_month(with!(ui, st, |ui, s| {
        (s.cal_year, s.cal_month) = cal::shift_month(s.cal_year, s.cal_month, -1);
        s.cal_sel = 1;
        push_calendar(&ui, &s);
    }));
    calg.on_next_month(with!(ui, st, |ui, s| {
        (s.cal_year, s.cal_month) = cal::shift_month(s.cal_year, s.cal_month, 1);
        s.cal_sel = 1;
        push_calendar(&ui, &s);
    }));
    calg.on_select(with!(ui, st, |ui, s, d| {
        s.cal_sel = d.max(1) as u32;
        push_calendar(&ui, &s);
    }));
    calg.on_add_event(with!(ui, st, |ui, s, time, title| {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let date = cal::iso(s.cal_year, s.cal_month, s.cal_sel);
        s.store.add_event(&date, &cal::normalize_time(&time), &title);
        s.store.save();
        push_calendar(&ui, &s);
    }));

    // Note
    let notes = ui.global::<Notes>();
    notes.on_set_filter(with!(ui, st, |ui, s, f| {
        s.note_filter = f;
        push_notes(&ui, &s);
    }));
    notes.on_save(with!(ui, st, |ui, s, id, title, body| {
        let title = if title.trim().is_empty() { SharedString::from("Untitled") } else { title };
        s.store.upsert_note(id, &title, &body, Local::now().timestamp());
        s.store.save();
        push_notes(&ui, &s);
    }));
    notes.on_toggle_pin(with!(ui, st, |ui, s, id| {
        s.store.toggle_pin(id);
        s.store.save();
        push_notes(&ui, &s);
    }));
    notes.on_remove(with!(ui, st, |ui, s, id| {
        s.store.remove_note(id);
        s.store.save();
        push_notes(&ui, &s);
    }));

    // Fotocamera: in mainline i sensori dei Lumia non hanno driver
    ui.global::<Cam>().set_available(live && std::path::Path::new("/dev/video0").exists());

    crate::ctl_comm::wire(ui, &st);
    crate::ctl_web::wire(ui, &st);
    crate::ctl_files::wire(ui, &st);
    crate::ctl_settings::wire(ui, &st);
    crate::ctl_files::wire_apk(ui, &st);
    crate::ctl_style::wire(ui, &st);
    crate::ctl_clock::wire(ui, &st);
    {
        let s = st.borrow();
        set_h24(s.store.data.prefs.get("h24").map(|v| v == "1").unwrap_or(true));
        let size = s.store.data.prefs.get("display_size").and_then(|v| v.parse().ok()).unwrap_or(1);
        let timeout = s.store.data.prefs.get("screen_timeout").and_then(|v| v.parse().ok()).unwrap_or(1);
        ui.global::<Sys>().set_screen_timeout(timeout);
        if s.store.data.prefs.get("noanim").map(|v| v == "1").unwrap_or(false) {
            set_lite_pref(ui, true);
        }
        if let Some(c) = s.store.data.prefs.get("weather_city") {
            ui.global::<Sys>().set_weather_city(c.clone().into());
            if let Ok(mut g) = WEATHER_CITY.lock() { *g = c.clone(); }
        }
        for (k, v) in s.store.data.prefs.iter() {
            if let (Some(name), Ok(v)) = (k.strip_prefix("p_"), v.parse::<i32>()) {
                set_pref_ui(ui, name, v);
            }
        }
        let _ = size; // display size option removed: the default scale stays
    }
    st
}

/// Timer di sistema: orologio ogni secondo, hardware ogni 5, modem in ctl_comm
pub fn start_timers(ui: &AppWindow, st: &Shared) -> Vec<slint::Timer> {
    let clock = slint::Timer::default();
    let w = ui.as_weak();
    let live_t = st.borrow().live;
    clock.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(1), move || {
        if let Some(ui) = w.upgrade() {
            if live_t {
                screen_timeout_tick(&ui);
            }
            push_clock(&ui, Local::now().naive_local());
        }
    });
    let hw = slint::Timer::default();
    let w = ui.as_weak();
    let live = st.borrow().live;
    if live { start_hw_thread(); }
    if live { hw.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(5), move || {
        if let Some(ui) = w.upgrade() {
            push_hw(&ui);
        }
    }); }
    let mut v = vec![clock, hw];
    v.extend(crate::ctl_comm::timers(ui, st));
    v.push(crate::ctl_files::timer(ui, st));
    v
}
