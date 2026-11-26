//! Dati finti condivisi da screenshot e demo web: rubrica, messaggi,
//! note, reti Wi-Fi, app Android. Nessun accesso al sistema.

use crate::controller::{self, Shared};
use crate::store::{Bookmark, CallLog, Contact, Data, Event, Message, Note, Store};
use crate::system::WifiNet;
use crate::*;
use chrono::{Local, TimeZone};
use slint::ComponentHandle;

fn ts(d: u32, h: u32, m: u32) -> i64 {
    Local.with_ymd_and_hms(2026, 11, d, h, m, 0).single().map(|t| t.timestamp()).unwrap_or(0)
}

pub fn store() -> Store {
    let n = |id, t: &str, b: &str, p, u| Note { id, title: t.into(), body: b.into(), pinned: p, updated: u };
    let e = |d: &str, t: &str, x: &str| Event { date: d.into(), time: t.into(), title: x.into() };
    let c = |n: &str, num: &str| Contact { name: n.into(), number: num.into() };
    let call = |num: &str, t, dir: &str, secs| CallLog { number: num.into(), ts: t, dir: dir.into(), secs };
    let m = |num: &str, t: &str, at, out, read| Message { number: num.into(), text: t.into(), ts: at, outgoing: out, read };
    Store::memory(Data {
        app_order: vec![],
        dock: crate::store::default_dock(),
        notes: vec![
            n(1, "Groceries", "Milk, bread, cat food", true, 10),
            n(2, "OS ideas", "Edge gestures, weather widget on home", true, 9),
            n(3, "To do", "Boot shim, touch test, Waydroid", false, 8),
            n(4, "Wi-Fi code", "Ask at home", false, 7),
        ],
        events: vec![
            e("2026-11-03", "09:30", "Dentist"),
            e("2026-11-25", "18:00", "Groceries"),
            e("2026-11-25", "23:00", "Cheip OS design review"),
            e("2026-11-27", "10:00", "Ship build 0.3"),
        ],
        contacts: vec![
            c("Andrea", "+39 340 112 2334"),
            c("Giulia", "+39 347 556 7788"),
            c("Marco", "+39 333 123 4567"),
            c("Rossi Garage", "0331 123456"),
        ],
        calls: vec![
            call("+393331234567", ts(25, 21, 12), "out", 240),
            call("+393475567788", ts(25, 18, 40), "missed", 0),
            call("0331123456", ts(24, 10, 5), "in", 95),
            call("+393401122334", ts(22, 16, 30), "out", 600),
        ],
        messages: vec![
            m("+393331234567", "Trying Cheip OS on the Lumia tomorrow?", ts(25, 22, 30), false, true),
            m("+393331234567", "Yes, 0.3 has phone, browser and a media player", ts(25, 22, 33), true, true),
            m("+393331234567", "Great. I'll bring the 640 so we can test that too", ts(25, 22, 47), false, false),
            m("+393475567788", "Called you, call me back when you can", ts(25, 18, 41), false, false),
            m("3912", "Your remaining credit is 8.40 euro", ts(23, 9, 0), false, true),
        ],
        bookmarks: vec![
            Bookmark { title: "Cheip Studio".into(), url: "https://cheipstudio.org".into() },
            Bookmark { title: "postmarketOS wiki".into(), url: "https://wiki.postmarketos.org".into() },
            Bookmark { title: "Weather Sesto Calende".into(), url: "https://html.duckduckgo.com/html/?q=weather%20sesto%20calende".into() },
        ],
        alarms: vec![
            crate::store::Alarm { id: 1, h: 7, m: 0, label: "Work".into(), days: 0x1f, on: true },
            crate::store::Alarm { id: 2, h: 9, m: 30, label: "Weekend".into(), days: 0x60, on: false },
        ],
        cities: crate::store::default_cities(),
        style: crate::store::StyleCfg::default(),
        ..Default::default()
    })
}

pub const PAGE: &str = r#"<html><head><title>Cheip OS 0.3 is here</title></head><body>
<nav><a href="/">Home</a></nav>
<h1>Cheip OS 0.3: media, files and an English UI</h1>
<p>The third release turns the Files app into a media hub: photo gallery, video player, music player with tags and covers, PDF and document viewers.</p>
<h2>A browser that fits in a few MB</h2>
<p>Instead of a full web engine, pages are reduced to native headings, text and links. On 1 GB of RAM that makes the difference.</p>
<ul><li>Tabs with per-tab history</li><li>Bookmarks</li><li>Built-in search</li></ul>
<p><a href="https://cheipstudio.org/cheip-os">Full release notes</a></p>
<blockquote><p>The real web engine arrives when the shell becomes a compositor.</p></blockquote>
</body></html>"#;


/// Valori di sistema come su un 735 reale, da chiamare dopo controller::wire
pub fn apply(ui: &AppWindow, st: &Shared) {
    ui.global::<Sys>().set_demo(true);
    {
        let mut s = st.borrow_mut();
        s.modem_ok = true;
        s.notify("system", "System", "Cheip OS 0.3.1 ready to install");
        s.notify("messages", "Marco", "Great. I'll bring the 640 so we can test that too");
        controller::push_all(&ui, &s);
    }
    let s = ui.global::<Sys>();
    s.set_battery(78);
    s.set_mem_used(588);
    s.set_mem_total(1024);
    s.set_mem_android(212);
    s.set_android_running(true);
    s.set_android_state("Running, suspends after 2 minutes with no apps".into());
    s.set_model("Nokia Lumia 735".into());
    s.set_kernel("6.12.9-msm8x26".into());
    s.set_update_available(true);
    s.set_update_version("0.3.1".into());
    s.set_update_state("Faster browser, SMS fixes".into());
    s.set_volume(0.6);
    let set = ui.global::<Settings>();
    set.set_modem_operator("TIM".into());
    set.set_modem_signal("67%".into());
    set.set_modem_state("registered".into());
    set.set_storage_used("2.9 GB".into());
    set.set_storage_total("5.6 GB".into());
    set.set_storage_ratio(0.52);
    crate::ctl_settings::demo_networks(
        &ui,
        vec![
            WifiNet { ssid: "Casa Casale".into(), signal: 82, secure: true, active: true },
            WifiNet { ssid: "FASTWEB-7A21".into(), signal: 61, secure: true, active: false },
            WifiNet { ssid: "Sesto Library".into(), signal: 44, secure: false, active: false },
            WifiNet { ssid: "TIM-90312".into(), signal: 30, secure: true, active: false },
        ],
    );

    // App Android finte nella griglia
    controller::set_demo_android(vec![
        ("org.telegram.messenger".into(), "Telegram".into()),
        ("com.whatsapp".into(), "WhatsApp".into()),
        ("com.spotify.music".into(), "Spotify".into()),
    ]);
    controller::push_apps(ui, &st.borrow());

}
