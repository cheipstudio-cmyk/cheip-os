//! Collega la UI Slint ai dati: orologio, hardware, app, calendario, note, OTA.

use crate::calendar as cal;
use crate::store::Store;
use crate::system as sys;
use crate::update;
use crate::*;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Toggle {
    id: &'static str,
    label: &'static str,
    on_sub: &'static str,
    off_sub: &'static str,
    on: bool,
}

struct Recent {
    id: String,
    name: String,
    android: bool,
}

pub struct State {
    pub store: Store,
    live: bool,
    today: NaiveDate,
    cal_year: i32,
    cal_month: u32,
    cal_sel: u32,
    toggles: Vec<Toggle>,
    recents: Vec<Recent>,
    search: String,
    note_filter: i32,
}

pub type Shared = Rc<RefCell<State>>;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn system_apps() -> Vec<(String, String, bool)> {
    [("calendar", "Calendario"), ("notes", "Note"), ("camera", "Fotocamera"), ("settings", "Impostazioni")]
        .iter()
        .map(|(id, n)| (id.to_string(), n.to_string(), false))
        .collect()
}

fn all_apps() -> Vec<(String, String, bool)> {
    let mut v = system_apps();
    for (pkg, name) in sys::android_apps() {
        v.push((pkg, name, true));
    }
    v
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
                Toggle { id: "wifi", label: "Wi-Fi", on_sub: "Acceso", off_sub: "Spento", on: wifi },
                Toggle { id: "dati", label: "Dati mobili", on_sub: "Accesi", off_sub: "Spenti", on: false },
                Toggle { id: "bt", label: "Bluetooth", on_sub: "Acceso", off_sub: "Spento", on: false },
                Toggle { id: "torcia", label: "Torcia", on_sub: "Accesa", off_sub: "Spenta", on: false },
                Toggle { id: "rot", label: "Rotazione", on_sub: "Auto", off_sub: "Bloccata", on: false },
                Toggle { id: "eco", label: "Risparmio", on_sub: "Acceso", off_sub: "Spento", on: false },
            ],
            recents: Vec::new(),
            search: String::new(),
            note_filter: 0,
        }
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
        (true, "Attivo · si sospende dopo 2 min senza app".to_string())
    } else {
        (false, "Sospeso · parte quando apri un'app Android".to_string())
    };
    s.set_android_running(running);
    s.set_android_state(label.into());
}

fn push_toggles(ui: &AppWindow, st: &State) {
    let v = st
        .toggles
        .iter()
        .map(|t| ToggleItem {
            id: t.id.into(),
            label: t.label.into(),
            sub: (if t.on { t.on_sub } else { t.off_sub }).into(),
            on: t.on,
        })
        .collect();
    ui.global::<Sys>().set_toggles(model(v));
}

fn push_apps(ui: &AppWindow, st: &State) {
    let q = st.search.to_lowercase();
    let v = all_apps()
        .into_iter()
        .filter(|(_, n, _)| q.is_empty() || n.to_lowercase().contains(&q))
        .map(|(id, name, android)| AppEntry { id: id.into(), name: name.into(), android })
        .collect();
    ui.global::<Shell>().set_apps(model(v));
}

fn push_recents(ui: &AppWindow, st: &State) {
    let v = st
        .recents
        .iter()
        .enumerate()
        .map(|(i, r)| RecentItem {
            id: r.id.clone().into(),
            name: r.name.clone().into(),
            mem: 0,
            state: (if i == 0 { "ultima aperta" } else if r.android { "Android, in pausa" } else { "in pausa" }).into(),
            android: r.android,
        })
        .collect();
    ui.global::<Shell>().set_recents(model(v));
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
    let rows = ((list.len() + 1) / 2) as i32;
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
}

// ---------- OTA ----------

fn check_update(ui: &AppWindow) {
    let s = ui.global::<Sys>();
    s.set_update_state("Controllo in corso…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let url = update::manifest_url();
        let res = update::fetch(&url);
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
    let s = ui.global::<Sys>();
    s.set_update_state("Download e installazione…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        // L'helper fa apk upgrade dal repo Cheip e riavvia la shell
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
    {
        let s = ui.global::<Sys>();
        s.set_version(VERSION.into());
        if live {
            s.set_model(sys::model().into());
            s.set_kernel(sys::kernel().into());
            if let Some(b) = sys::brightness() {
                s.set_brightness(b);
            }
        }
        s.set_update_state("Tocca Controlla per cercare aggiornamenti".into());
    }
    push_clock(ui, now);
    if live {
        push_hw(ui);
    }
    push_all(ui, &st.borrow());

    // Sys
    let sysg = ui.global::<Sys>();
    {
        let (w, st) = (ui.as_weak(), st.clone());
        sysg.on_toggle(move |id| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
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
        });
    }
    {
        let st = st.clone();
        sysg.on_brightness_changed(move |v| {
            if st.borrow().live {
                sys::set_brightness(v);
            }
        });
    }
    {
        let w = ui.as_weak();
        sysg.on_check_update(move || {
            if let Some(ui) = w.upgrade() {
                check_update(&ui);
            }
        });
    }
    {
        let w = ui.as_weak();
        sysg.on_install_update(move || {
            if let Some(ui) = w.upgrade() {
                install_update(&ui);
            }
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        sysg.on_android_toggle(move || {
            let Some(ui) = w.upgrade() else { return };
            if !st.borrow().live || !sys::waydroid_installed() {
                return;
            }
            let start = !ui.global::<Sys>().get_android_running();
            sys::helper(&["android", if start { "start" } else { "stop" }]);
            push_hw(&ui);
        });
    }

    // Shell
    let shell = ui.global::<Shell>();
    {
        let (w, st) = (ui.as_weak(), st.clone());
        shell.on_open_app(move |id, from_y| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            let apps = all_apps();
            let Some((_, name, android)) = apps.into_iter().find(|(a, _, _)| a == id.as_str()) else { return };
            s.recents.retain(|r| r.id != id.as_str());
            s.recents.insert(0, Recent { id: id.to_string(), name, android });
            if android && s.live {
                sys::run("waydroid", &["app", "launch", id.as_str()]);
            }
            let g = ui.global::<Shell>();
            g.set_from_y(from_y - 2.0);
            g.set_app_id(id);
            g.set_controls_open(false);
            g.set_screen(ScreenId::Home);
            g.set_app_open(true);
            push_recents(&ui, &s);
        });
    }
    {
        let w = ui.as_weak();
        shell.on_close_app(move || {
            if let Some(ui) = w.upgrade() {
                ui.global::<Shell>().set_app_open(false);
            }
        });
    }
    {
        let w = ui.as_weak();
        shell.on_unlock(move || {
            if let Some(ui) = w.upgrade() {
                ui.global::<Shell>().set_screen(ScreenId::Home);
            }
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        shell.on_search(move |q| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            s.search = q.to_string();
            push_apps(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        shell.on_close_recent(move |id| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            if let Some(r) = s.recents.iter().find(|r| r.id == id.as_str()) {
                if r.android && s.live {
                    sys::run("waydroid", &["app", "stop", id.as_str()]);
                }
            }
            s.recents.retain(|r| r.id != id.as_str());
            push_recents(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        shell.on_close_all(move || {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            if s.live {
                for r in s.recents.iter().filter(|r| r.android) {
                    sys::run("waydroid", &["app", "stop", &r.id]);
                }
            }
            s.recents.clear();
            push_recents(&ui, &s);
            ui.global::<Shell>().set_screen(ScreenId::Home);
        });
    }

    // Calendario
    let calg = ui.global::<Cal>();
    {
        let (w, st) = (ui.as_weak(), st.clone());
        calg.on_prev_month(move || {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            (s.cal_year, s.cal_month) = cal::shift_month(s.cal_year, s.cal_month, -1);
            s.cal_sel = 1;
            push_calendar(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        calg.on_next_month(move || {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            (s.cal_year, s.cal_month) = cal::shift_month(s.cal_year, s.cal_month, 1);
            s.cal_sel = 1;
            push_calendar(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        calg.on_select(move |d| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            s.cal_sel = d.max(1) as u32;
            push_calendar(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        calg.on_add_event(move |time, title| {
            let Some(ui) = w.upgrade() else { return };
            let title = title.trim().to_string();
            if title.is_empty() {
                return;
            }
            let mut s = st.borrow_mut();
            let date = cal::iso(s.cal_year, s.cal_month, s.cal_sel);
            s.store.add_event(&date, &cal::normalize_time(&time), &title);
            s.store.save();
            push_calendar(&ui, &s);
        });
    }

    // Note
    let notes = ui.global::<Notes>();
    {
        let (w, st) = (ui.as_weak(), st.clone());
        notes.on_set_filter(move |f| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            s.note_filter = f;
            push_notes(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        notes.on_save(move |id, title, body| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            let title = if title.trim().is_empty() { SharedString::from("Senza titolo") } else { title };
            s.store.upsert_note(id, &title, &body, Local::now().timestamp());
            s.store.save();
            push_notes(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        notes.on_toggle_pin(move |id| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            s.store.toggle_pin(id);
            s.store.save();
            push_notes(&ui, &s);
        });
    }
    {
        let (w, st) = (ui.as_weak(), st.clone());
        notes.on_remove(move |id| {
            let Some(ui) = w.upgrade() else { return };
            let mut s = st.borrow_mut();
            s.store.remove_note(id);
            s.store.save();
            push_notes(&ui, &s);
        });
    }

    // Fotocamera: in mainline i sensori dei Lumia non hanno driver
    ui.global::<Cam>().set_available(std::path::Path::new("/dev/video0").exists() && live);

    st
}

/// Timer di sistema: orologio ogni secondo, hardware ogni 5
pub fn start_timers(ui: &AppWindow) -> Vec<slint::Timer> {
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
    vec![clock, hw]
}
