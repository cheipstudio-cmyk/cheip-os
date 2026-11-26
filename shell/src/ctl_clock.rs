//! App Orologio: sveglie, fusi orari, timer e cronometro.

use crate::controller::{model, ui_handle, Shared};
use crate::store::Alarm;
use crate::with;
use crate::*;
use chrono::{Datelike, Duration as CDur, Local, NaiveDateTime, Timelike};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::time::Duration;

/// Citta disponibili con lo scarto da UTC in minuti (ora solare)
pub const CITIES: [(&str, i32); 14] = [
    ("New York", -300),
    ("London", 0),
    ("Tokyo", 540),
    ("Los Angeles", -480),
    ("Dubai", 240),
    ("Tunis", 60),
    ("Stockholm", 60),
    ("Sydney", 660),
    ("Sao Paulo", -180),
    ("Mumbai", 330),
    ("Singapore", 480),
    ("Reykjavik", 0),
    ("Cairo", 120),
    ("Mexico City", -360),
];

#[derive(Default)]
struct Clk {
    timer_total: i64,
    timer_end: i64,
    timer_left: i64,
    timer_state: i32,
    sw_start: i64,
    sw_acc: i64,
    sw_running: bool,
    laps: Vec<i64>,
    ticker: Option<slint::Timer>,
    minute: Option<slint::Timer>,
    ring: Option<slint::Timer>,
    snooze_at: Option<(i64, String)>,
    last_check: String,
}

thread_local! {
    static C: RefCell<Clk> = RefCell::new(Clk::default());
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn hms(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn sw_fmt(ms: i64) -> String {
    let cs = ms / 10;
    let (m, s, c) = (cs / 6000, (cs / 100) % 60, cs % 100);
    if m >= 60 {
        format!("{}:{:02}:{s:02}.{c:02}", m / 60, m % 60)
    } else {
        format!("{m:02}:{s:02}.{c:02}")
    }
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn days_label(mask: u8) -> String {
    match mask {
        0 => "Once".into(),
        0x7f => "Every day".into(),
        0x1f => "Weekdays".into(),
        0x60 => "Weekends".into(),
        m => (0..7).filter(|i| m & (1 << i) != 0).map(|i| DAYS[i]).collect::<Vec<_>>().join(", "),
    }
}

/// Prossimo momento in cui suona la sveglia
fn next_fire(a: &Alarm, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if !a.on {
        return None;
    }
    for d in 0..8 {
        let day = now.date() + CDur::days(d);
        let t = day.and_hms_opt(a.h, a.m, 0)?;
        if t <= now {
            continue;
        }
        let wd = day.weekday().num_days_from_monday();
        if a.days == 0 || a.days & (1 << wd) != 0 {
            return Some(t);
        }
    }
    None
}

fn until(t: NaiveDateTime, now: NaiveDateTime) -> String {
    let mins = (t - now).num_minutes() + 1;
    let (h, m) = (mins / 60, mins % 60);
    match (h, m) {
        (0, m) => format!("In {m} min"),
        (h, 0) => format!("In {h} h"),
        (h, m) => format!("In {h} h {m} min"),
    }
}

pub fn push_alarms(ui: &AppWindow, alarms: &[Alarm]) {
    let now = Local::now().naive_local();
    let mut v: Vec<&Alarm> = alarms.iter().collect();
    v.sort_by_key(|a| (a.h, a.m));
    let rows: Vec<AlarmRow> = v
        .iter()
        .map(|a| AlarmRow {
            id: a.id,
            time: format!("{:02}:{:02}", a.h, a.m).into(),
            label: a.label.clone().into(),
            days: days_label(a.days).into(),
            on: a.on,
            next: next_fire(a, now).map(|t| format!("{} · {}", days_label(a.days), until(t, now))).unwrap_or_default().into(),
            mask: a.days as i32,
        })
        .collect();
    let next = alarms.iter().filter_map(|a| next_fire(a, now)).min();
    let c = ui.global::<Clock>();
    c.set_alarms(model(rows));
    c.set_next_alarm(
        next.map(|t| {
            let wd = t.date().weekday().num_days_from_monday() as usize;
            if t.date() == now.date() {
                format!("{:02}:{:02}", t.hour(), t.minute())
            } else {
                format!("{} {:02}:{:02}", DAYS[wd], t.hour(), t.minute())
            }
        })
        .unwrap_or_default()
        .into(),
    );
}

pub fn push_cities(ui: &AppWindow, cities: &[String]) {
    let local = Local::now();
    let local_off = local.offset().local_minus_utc() / 60;
    let utc = chrono::Utc::now().naive_utc();
    let rows: Vec<CityRow> = cities
        .iter()
        .filter_map(|name| CITIES.iter().find(|(n, _)| n == name))
        .map(|(n, off)| {
            let t = utc + CDur::minutes(*off as i64);
            let diff = off - local_off;
            let day = if t.date() > local.naive_local().date() { "Tomorrow" } else if t.date() < local.naive_local().date() { "Yesterday" } else { "Today" };
            let hours = diff as f32 / 60.0;
            let d = if diff == 0 {
                format!("{day}, same time")
            } else {
                let h = if hours.fract() == 0.0 { format!("{}", hours.abs() as i32) } else { format!("{:.1}", hours.abs()) };
                format!("{day}, {h} h {}", if diff > 0 { "ahead" } else { "behind" })
            };
            CityRow { city: (*n).into(), time: format!("{:02}:{:02}", t.hour(), t.minute()).into(), diff: d.into(), night: !(7..19).contains(&t.hour()) }
        })
        .collect();
    ui.global::<Clock>().set_cities(model(rows));
}

fn push_timer(ui: &AppWindow, c: &Clk) {
    let g = ui.global::<Clock>();
    let left = match c.timer_state {
        1 => c.timer_end - now_ms(),
        2 => c.timer_left,
        3 => 0,
        _ => 0,
    };
    g.set_timer_state(c.timer_state);
    g.set_timer_label(if c.timer_state == 3 { "0:00".into() } else { hms(left).into() });
    g.set_timer_progress(if c.timer_total > 0 { (left.max(0) as f32 / c.timer_total as f32).clamp(0.0, 1.0) } else { 0.0 });
}

fn sw_elapsed(c: &Clk) -> i64 {
    c.sw_acc + if c.sw_running { now_ms() - c.sw_start } else { 0 }
}

fn push_sw(ui: &AppWindow, c: &Clk) {
    let g = ui.global::<Clock>();
    let e = sw_elapsed(c);
    g.set_sw_label(sw_fmt(e).into());
    g.set_sw_running(c.sw_running);
    g.set_sw_sweep((e % 60_000) as f32 / 60_000.0);
    let mut prev = 0;
    let mut rows: Vec<LapRow> = c
        .laps
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let r = LapRow { n: i as i32 + 1, lap: sw_fmt(t - prev).into(), total: sw_fmt(*t).into() };
            prev = *t;
            r
        })
        .collect();
    rows.reverse();
    g.set_laps(model(rows));
}

/// Aggiornamento rapido mentre timer o cronometro corrono
fn ensure_ticker() {
    C.with(|c| {
        let mut c = c.borrow_mut();
        let need = c.timer_state == 1 || c.sw_running;
        if !need {
            c.ticker = None;
            return;
        }
        if c.ticker.is_some() {
            return;
        }
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_millis(33), tick);
        c.ticker = Some(t);
    });
}

fn tick() {
    let Some(ui) = ui_handle() else { return };
    let done = C.with(|c| {
        let mut c = c.borrow_mut();
        let mut done = false;
        if c.timer_state == 1 && now_ms() >= c.timer_end {
            c.timer_state = 3;
            done = true;
        }
        push_timer(&ui, &c);
        if c.sw_running {
            push_sw(&ui, &c);
        }
        done
    });
    if done {
        ring(&ui, "timer", "Time's up", "Timer");
    }
    ensure_ticker();
}

/// Suoneria a ripetizione e schermata a tutto schermo
fn ring(ui: &AppWindow, kind: &str, label: &str, title: &str) {
    let g = ui.global::<Clock>();
    g.set_ring_kind(kind.into());
    g.set_ring_label(label.into());
    g.set_ring_time(if kind == "timer" { title.into() } else { ui.global::<Sys>().get_time() });
    g.set_ringing(true);
    C.with(|c| {
        let t = slint::Timer::default();
        let play = || {
            if let Some(ui) = ui_handle() {
                let tone = ui.global::<Sys>().get_ringtone();
                crate::feedback::play(crate::feedback::Sound::Ring(tone), ui.global::<Sys>().get_volume().max(0.5));
                crate::feedback::vibrate(800);
            }
        };
        play();
        t.start(slint::TimerMode::Repeated, Duration::from_millis(crate::feedback::RING_CYCLE_MS), play);
        c.borrow_mut().ring = Some(t);
    });
}

fn stop_ring(ui: &AppWindow) {
    ui.global::<Clock>().set_ringing(false);
    C.with(|c| c.borrow_mut().ring = None);
}

/// Controllo delle sveglie, una volta al secondo
fn check_alarms(st: &Shared) {
    let Some(ui) = ui_handle() else { return };
    let now = Local::now().naive_local();
    let key = now.format("%Y%m%d%H%M").to_string();
    // posticipata
    let snoozed = C.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((at, label)) = c.snooze_at.clone() {
            if now_ms() >= at {
                c.snooze_at = None;
                return Some(label);
            }
        }
        None
    });
    if let Some(label) = snoozed {
        ring(&ui, "alarm", &label, "");
    }
    let fresh = C.with(|c| {
        let mut c = c.borrow_mut();
        if c.last_check == key {
            false
        } else {
            c.last_check = key;
            true
        }
    });
    if !fresh {
        return;
    }
    let mut s = st.borrow_mut();
    let wd = now.date().weekday().num_days_from_monday();
    let mut fire: Option<String> = None;
    for a in s.store.data.alarms.iter_mut() {
        if a.on && a.h == now.hour() && a.m == now.minute() && (a.days == 0 || a.days & (1 << wd) != 0) {
            fire = Some(if a.label.is_empty() { "Alarm".into() } else { a.label.clone() });
            if a.days == 0 {
                a.on = false;
            }
        }
    }
    if fire.is_some() {
        s.store.save();
    }
    push_alarms(&ui, &s.store.data.alarms);
    push_cities(&ui, &s.store.data.cities);
    drop(s);
    if let Some(label) = fire {
        ring(&ui, "alarm", &label, "");
    }
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    {
        let s = st.borrow();
        push_alarms(ui, &s.store.data.alarms);
        push_cities(ui, &s.store.data.cities);
        let tz = if s.live { crate::system::timezone() } else { "Europe/Rome".into() };
        ui.global::<Clock>().set_local_city(tz.rsplit('/').next().unwrap_or("Rome").replace('_', " ").into());
    }
    C.with(|c| {
        let mut c = c.borrow_mut();
        push_timer(ui, &c);
        push_sw(ui, &c);
        let t = slint::Timer::default();
        let st2 = st.clone();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || check_alarms(&st2));
        c.minute = Some(t);
    });
    let g = ui.global::<Clock>();

    g.on_alarm_add(with!(ui, st, |ui, s, h, m, label| {
        let id = s.store.data.alarms.iter().map(|a| a.id).max().unwrap_or(0) + 1;
        s.store.data.alarms.push(Alarm { id, h: h.clamp(0, 23) as u32, m: m.clamp(0, 59) as u32, label: label.to_string(), days: 0, on: true });
        s.store.save();
        push_alarms(&ui, &s.store.data.alarms);
        if let Some(a) = s.store.data.alarms.last() {
            if let Some(t) = next_fire(a, Local::now().naive_local()) {
                crate::controller::toast(&ui, &format!("Alarm set for {}", until(t, Local::now().naive_local()).to_lowercase()));
            }
        }
    }));
    g.on_alarm_toggle(with!(ui, st, |ui, s, id, on| {
        if let Some(a) = s.store.data.alarms.iter_mut().find(|a| a.id == id) {
            a.on = on;
        }
        s.store.save();
        push_alarms(&ui, &s.store.data.alarms);
    }));
    g.on_alarm_delete(with!(ui, st, |ui, s, id| {
        s.store.data.alarms.retain(|a| a.id != id);
        s.store.save();
        push_alarms(&ui, &s.store.data.alarms);
    }));
    g.on_alarm_day(with!(ui, st, |ui, s, id, day| {
        if let Some(a) = s.store.data.alarms.iter_mut().find(|a| a.id == id) {
            a.days ^= 1 << day.clamp(0, 6);
            a.on = true;
        }
        s.store.save();
        push_alarms(&ui, &s.store.data.alarms);
    }));
    g.on_city_add(with!(ui, st, |ui, s| {
        if let Some((n, _)) = CITIES.iter().find(|(n, _)| !s.store.data.cities.iter().any(|c| c == n)) {
            s.store.data.cities.push(n.to_string());
            s.store.save();
        }
        push_cities(&ui, &s.store.data.cities);
    }));
    g.on_city_remove(with!(ui, st, |ui, s, i| {
        if (i as usize) < s.store.data.cities.len() {
            s.store.data.cities.remove(i as usize);
            s.store.save();
        }
        push_cities(&ui, &s.store.data.cities);
    }));

    g.on_timer_start(|| {
        let Some(ui) = ui_handle() else { return };
        let digits = ui.global::<Clock>().get_timer_input().to_string();
        let n: i64 = digits.parse().unwrap_or(0);
        let (h, m, s) = (n / 10000, (n / 100) % 100, n % 100);
        let total = ((h * 3600 + m * 60 + s) * 1000).max(1000);
        C.with(|c| {
            let mut c = c.borrow_mut();
            c.timer_total = total;
            c.timer_end = now_ms() + total;
            c.timer_state = 1;
            push_timer(&ui, &c);
        });
        ui.global::<Clock>().set_timer_input("".into());
        ensure_ticker();
    });
    g.on_timer_pause(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            c.timer_left = c.timer_end - now_ms();
            c.timer_state = 2;
            push_timer(&ui, &c);
        });
        ensure_ticker();
    });
    g.on_timer_resume(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            c.timer_end = now_ms() + c.timer_left;
            c.timer_state = 1;
            push_timer(&ui, &c);
        });
        ensure_ticker();
    });
    g.on_timer_reset(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            c.timer_state = 0;
            push_timer(&ui, &c);
        });
        ensure_ticker();
    });
    g.on_timer_plus(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            match c.timer_state {
                1 => c.timer_end += 60_000,
                2 => c.timer_left += 60_000,
                3 => {
                    c.timer_end = now_ms() + 60_000;
                    c.timer_state = 1;
                }
                _ => {}
            }
            c.timer_total += 60_000;
            push_timer(&ui, &c);
        });
        ensure_ticker();
    });

    g.on_sw_toggle(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            if c.sw_running {
                c.sw_acc += now_ms() - c.sw_start;
                c.sw_running = false;
            } else {
                c.sw_start = now_ms();
                c.sw_running = true;
            }
            push_sw(&ui, &c);
        });
        ensure_ticker();
    });
    g.on_sw_lap(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            let e = sw_elapsed(&c);
            c.laps.push(e);
            push_sw(&ui, &c);
        });
    });
    g.on_sw_reset(|| {
        let Some(ui) = ui_handle() else { return };
        C.with(|c| {
            let mut c = c.borrow_mut();
            c.sw_acc = 0;
            c.sw_running = false;
            c.laps.clear();
            push_sw(&ui, &c);
        });
        ensure_ticker();
    });

    g.on_ring_stop(|| {
        let Some(ui) = ui_handle() else { return };
        stop_ring(&ui);
        C.with(|c| {
            let mut c = c.borrow_mut();
            if c.timer_state == 3 {
                c.timer_state = 0;
                push_timer(&ui, &c);
            }
        });
    });
    g.on_ring_snooze(|| {
        let Some(ui) = ui_handle() else { return };
        let kind = ui.global::<Clock>().get_ring_kind().to_string();
        let label = ui.global::<Clock>().get_ring_label().to_string();
        stop_ring(&ui);
        if kind == "timer" {
            C.with(|c| {
                let mut c = c.borrow_mut();
                c.timer_end = now_ms() + 60_000;
                c.timer_total = 60_000;
                c.timer_state = 1;
                push_timer(&ui, &c);
            });
            ensure_ticker();
        } else {
            C.with(|c| c.borrow_mut().snooze_at = Some((now_ms() + 10 * 60_000, label)));
            crate::controller::toast(&ui, "Snoozed for 10 minutes");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn next_alarm_respects_days() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap().and_hms_opt(8, 0, 0).unwrap(); // venerdi
        let a = Alarm { id: 1, h: 7, m: 0, label: String::new(), days: 0x1f, on: true };
        assert_eq!(next_fire(&a, now).unwrap().date(), NaiveDate::from_ymd_opt(2026, 11, 30).unwrap());
        let once = Alarm { days: 0, ..a.clone() };
        assert_eq!(next_fire(&once, now).unwrap().date(), NaiveDate::from_ymd_opt(2026, 11, 28).unwrap());
        assert_eq!(days_label(0x60), "Weekends");
        assert_eq!(sw_fmt(61_230), "01:01.23");
        assert_eq!(hms(59_001), "01:00");
    }
}
