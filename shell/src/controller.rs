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
pub const SYSTEM_APPS: [(&str, &str, bool); 8] = [
    ("phone", "Phone", true),
    ("messages", "Messages", true),
    ("browser", "Browser", true),
    ("camera", "Camera", true),
    ("files", "Files", false),
    ("calendar", "Calendar", false),
    ("notes", "Notes", false),
    ("settings", "Settings", false),
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
                Toggle { id: "dati", label: "Mobile data", on: false },
                Toggle { id: "bt", label: "Bluetooth", on: false },
                Toggle { id: "torcia", label: "Flashlight", on: false },
                Toggle { id: "rot", label: "Auto-rotate", on: false },
                Toggle { id: "eco", label: "Battery saver", on: false },
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
            fx: Default::default(),
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
        (false, "Waydroid not installed".to_string())
    } else if sys::waydroid_running() {
        (true, "Running, suspends after 2 minutes with no apps".to_string())
    } else {
        (false, "Suspended, starts when you open an Android app".to_string())
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

thread_local! {
    static DEMO_ANDROID: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
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
        ui.global::<Settings>().set_page("appinfo".into());
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
    let live = st.borrow().live;
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
