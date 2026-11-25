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
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub cwd: PathBuf,
    pub entries: Vec<crate::files::Entry>,
    pub scroll_gen: i32,
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

/// "22:47" oggi, "ieri", altrimenti "25/11"
pub fn when(ts: i64, now: NaiveDateTime) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&Local).naive_local()) else { return String::new() };
    let days = (now.date() - t.date()).num_days();
    match days {
        0 => format!("{:02}:{:02}", t.hour(), t.minute()),
        1 => "ieri".into(),
        _ => format!("{:02}/{:02}", t.day(), t.month()),
    }
}

/// App di sistema: (id, nome, nel dock)
pub const SYSTEM_APPS: [(&str, &str, bool); 8] = [
    ("phone", "Telefono", true),
    ("messages", "Messaggi", true),
    ("browser", "Browser", true),
    ("camera", "Fotocamera", true),
    ("files", "File", false),
    ("calendar", "Calendario", false),
    ("notes", "Note", false),
    ("settings", "Impostazioni", false),
];

pub fn glyph(ui: &AppWindow, id: &str) -> SharedString {
    let i = ui.global::<Icons>();
    match id {
        "phone" => i.get_phone(),
        "messages" => i.get_message(),
        "browser" => i.get_globe(),
        "camera" => i.get_camera(),
        "files" => i.get_folder(),
        "calendar" => i.get_calendar(),
        "notes" => i.get_notes(),
        "settings" => i.get_gear(),
        "wifi" => i.get_wifi(),
        "dati" => i.get_data(),
        "bt" => i.get_bluetooth(),
        "torcia" => i.get_torch(),
        "rot" => i.get_rotate(),
        "eco" => i.get_leaf(),
        _ => i.get_android(),
    }
}

impl State {
    fn new(store: Store, now: NaiveDateTime, live: bool) -> Self {
        let today = now.date();
        let wifi = if live { sys::wifi_on() } else { true };
        State {
            store,
            live,
            today,
            cal_year: today.year(),
            cal_month: today.month(),
            cal_sel: today.day(),
            toggles: vec![
                Toggle { id: "wifi", label: "Wi-Fi", on: wifi },
                Toggle { id: "dati", label: "Dati mobili", on: false },
                Toggle { id: "bt", label: "Bluetooth", on: false },
                Toggle { id: "torcia", label: "Torcia", on: false },
                Toggle { id: "rot", label: "Rotazione automatica", on: false },
                Toggle { id: "eco", label: "Risparmio energetico", on: false },
            ],
            recents: Vec::new(),
            search: String::new(),
            note_filter: 0,
            modem_ok: false,
            call: None,
            missed: 0,
            open_thread: String::new(),
            notifs: Vec::new(),
            tabs: vec![Tab::default()],
            tab: 0,
            cwd: crate::files::home(),
            entries: Vec::new(),
            scroll_gen: 0,
        }
    }

    pub fn toggle_on(&self, id: &str) -> bool {
        self.toggles.iter().find(|t| t.id == id).map(|t| t.on).unwrap_or(false)
    }

    pub fn notify(&mut self, app: &str, text: &str, android: bool) {
        let now = Local::now();
        self.notifs.insert(0, NotifItem { app: app.into(), time: format!("{:02}:{:02}", now.hour(), now.minute()).into(), text: text.into(), android });
        self.notifs.truncate(3);
    }
}

// ---------- refresh verso la UI ----------

pub fn push_clock(ui: &AppWindow, now: NaiveDateTime) {
    let s = ui.global::<Sys>();
    s.set_time(format!("{:02}:{:02}", now.hour(), now.minute()).into());
    s.set_hours(format!("{:02}", now.hour()).into());
    s.set_minutes(format!("{:02}", now.minute()).into());
    s.set_date(cal::long_label(now.date()).into());
}

fn push_hw(ui: &AppWindow) {
    let s = ui.global::<Sys>();
    if let Some((b, ch)) = sys::battery() {
        s.set_battery(b);
        s.set_charging(ch);
    }
    let (used, total) = sys::memory();
    s.set_mem_used(used);
    s.set_mem_total(total);
    s.set_mem_android(sys::android_memory().min(used));
    let (running, label) = if !sys::waydroid_installed() {
        (false, "Waydroid non installato".to_string())
    } else if sys::waydroid_running() {
        (true, "Attivo, si sospende dopo 2 minuti senza app".to_string())
    } else {
        (false, "Sospeso, parte quando apri un'app Android".to_string())
    };
    s.set_android_running(running);
    s.set_android_state(label.into());
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
    }
}

/// Tutte le app: (id, nome, android, icona)
pub fn catalog() -> Vec<(String, String, bool, Option<PathBuf>)> {
    let mut v: Vec<_> = SYSTEM_APPS.iter().map(|(id, n, _)| (id.to_string(), n.to_string(), false, None)).collect();
    for (pkg, name, icon) in sys::android_apps() {
        v.push((pkg, name, true, icon));
    }
    v
}

pub fn push_apps(ui: &AppWindow, st: &State) {
    let q = st.search.to_lowercase();
    let searching = !q.is_empty();
    let grid = catalog()
        .into_iter()
        .filter(|(id, n, _, _)| {
            if searching {
                n.to_lowercase().contains(&q)
            } else {
                !SYSTEM_APPS.iter().any(|(s, _, dock)| s == id && *dock)
            }
        })
        .map(|(id, n, a, icon)| entry(ui, st, &id, &n, a, icon))
        .collect();
    let dock = SYSTEM_APPS.iter().filter(|(_, _, d)| *d).map(|(id, n, _)| entry(ui, st, id, n, false, None)).collect();
    let g = ui.global::<Shell>();
    g.set_apps(model(grid));
    g.set_dock(model(dock));
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
            state: (if i == 0 { "ultima aperta" } else if r.android { "Android, in pausa" } else { "in pausa" }).into(),
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

fn check_update(ui: &AppWindow) {
    let s = ui.global::<Sys>();
    s.set_update_state("Controllo in corso…".into());
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
                    let note = if m.notes.is_empty() { "Pronto da installare".to_string() } else { m.notes.clone() };
                    s.set_update_state(note.into());
                }
                Ok(_) => {
                    s.set_update_available(false);
                    s.set_update_state("Sei all'ultima versione".into());
                }
                Err(_) => s.set_update_state("Server non raggiungibile".into()),
            }
        });
    });
}

fn install_update(ui: &AppWindow) {
    ui.global::<Sys>().set_update_state("Download e installazione…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let ok = sys::helper(&["update"]);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.global::<Sys>();
            if ok {
                s.set_update_available(false);
                s.set_update_state("Installato, riavvio della shell".into());
            } else {
                s.set_update_state("Installazione non riuscita, riprova".into());
            }
        });
    });
}

// ---------- collegamento ----------

/// `live = false` per screenshot e test: niente timer, niente comandi di sistema.
pub fn wire(ui: &AppWindow, store: Store, now: NaiveDateTime, live: bool) -> Shared {
    let st: Shared = Rc::new(RefCell::new(State::new(store, now, live)));
    STATE.with(|s| *s.borrow_mut() = Some(st.clone()));
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
        s.set_update_state("Tocca Controlla per cercare aggiornamenti".into());
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
                    _ => {}
                }
            }
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
    sysg.on_check_update(with!(ui, st, |ui, _s| { check_update(&ui); }));
    sysg.on_install_update(with!(ui, st, |ui, _s| { install_update(&ui); }));
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
        let g = ui.global::<Shell>();
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
        let title = if title.trim().is_empty() { SharedString::from("Senza titolo") } else { title };
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
    st
}

/// Timer di sistema: orologio ogni secondo, hardware ogni 5, modem in ctl_comm
pub fn start_timers(ui: &AppWindow, st: &Shared) -> Vec<slint::Timer> {
    let clock = slint::Timer::default();
    let w = ui.as_weak();
    clock.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(1), move || {
        if let Some(ui) = w.upgrade() {
            push_clock(&ui, Local::now().naive_local());
        }
    });
    let hw = slint::Timer::default();
    let w = ui.as_weak();
    hw.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(5), move || {
        if let Some(ui) = w.upgrade() {
            push_hw(&ui);
        }
    });
    let mut v = vec![clock, hw];
    v.extend(crate::ctl_comm::timers(ui, st));
    v
}
