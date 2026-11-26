//! Impostazioni: pagine Wi-Fi, Bluetooth, rete mobile, archiviazione, ecc.

use crate::controller::{catalog, glyph, model, shared, toast, Shared, State};
use crate::system as sys;
use crate::with;
use crate::*;
use slint::{ComponentHandle, SharedString};

fn wifi_rows(v: Vec<sys::WifiNet>) -> Vec<WifiRow> {
    v.into_iter().map(|n| WifiRow { ssid: n.ssid.into(), signal: n.signal, secure: n.secure, active: n.active }).collect()
}

fn scan(ui: &AppWindow, msg: Option<String>) {
    let s = ui.global::<Settings>();
    s.set_wifi_busy(true);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let nets = sys::wifi_scan();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.global::<Settings>();
            s.set_wifi_busy(false);
            let label = msg.unwrap_or_else(|| if nets.is_empty() { "No networks found".into() } else { format!("{} networks", nets.len()) });
            s.set_wifi_msg(label.into());
            s.set_networks(model(wifi_rows(nets)));
        });
    });
}

/// Reti finte per screenshot e test
pub fn demo_networks(ui: &AppWindow, nets: Vec<sys::WifiNet>) {
    let s = ui.global::<Settings>();
    s.set_wifi_msg(format!("{} networks", nets.len()).into());
    s.set_networks(model(wifi_rows(nets)));
}

// ---------- Pagine generiche ----------

thread_local! {
    static STACK: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static ERASE_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BUILD_TAPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Pagine disegnate a mano nello .slint
const CUSTOM: [&str; 8] = ["wifi", "sound", "battery", "storage", "android", "update", "appinfo", "style"];

fn row(kind: i32, key: &str, title: &str, sub: &str, glyph: SharedString, on: bool, value: f32) -> SRow {
    SRow { kind, key: key.into(), title: title.into(), subtitle: sub.into(), glyph, on, value }
}
fn h(t: &str) -> SRow {
    row(0, "", t, "", "".into(), false, 0.0)
}
fn nav(k: &str, t: &str, sub: &str, g: SharedString) -> SRow {
    row(1, k, t, sub, g, false, 0.0)
}
fn sw(k: &str, t: &str, sub: &str, on: bool) -> SRow {
    row(2, k, t, sub, "".into(), on, 0.0)
}
fn val(k: &str, t: &str, sub: &str) -> SRow {
    row(3, k, t, sub, "".into(), false, 0.0)
}
fn sl(k: &str, t: &str, v: f32) -> SRow {
    row(4, k, t, "", "".into(), false, v)
}
fn radio(k: &str, t: &str, on: bool) -> SRow {
    row(5, k, t, "", "".into(), on, 0.0)
}
fn act(k: &str, t: &str, sub: &str, g: SharedString) -> SRow {
    row(6, k, t, sub, g, false, 0.0)
}
fn danger(k: &str, t: &str, sub: &str) -> SRow {
    row(7, k, t, sub, "".into(), false, 0.0)
}
fn note(t: &str) -> SRow {
    row(8, "", t, "", "".into(), false, 0.0)
}

/// Valori a scelta: la riga mostra il valore e un tocco passa al successivo
const OPTIONS: [(&str, &[&str]); 6] = [
    ("timeout", &["30 seconds", "1 minute", "2 minutes", "5 minutes", "10 minutes", "15 seconds"]),
    ("lockscreen", &["Show all notifications", "Hide silent notifications", "Don't show notifications"]),
    ("usb", &["Charging only", "File transfer", "USB tethering"]),
    ("screenlock", &["Swipe", "None"]),
    ("anim", &["1x", "0.5x", "Off", "1.5x"]),
    ("dns", &["Automatic", "Off"]),
];

fn opt(st: &State, k: &str) -> String {
    let list = OPTIONS.iter().find(|(n, _)| *n == k).map(|(_, v)| *v).unwrap_or(&[""]);
    st.store.data.prefs.get(k).cloned().unwrap_or_else(|| list[0].to_string())
}

fn pb(st: &State, k: &str, def: bool) -> bool {
    st.store.data.prefs.get(k).map(|v| v == "1").unwrap_or(def)
}

fn build(ui: &AppWindow, st: &State, page: &str) -> (String, Vec<SRow>) {
    let i = ui.global::<Icons>();
    let t = |id: &str| st.toggle_on(id);
    match page {
        "network" => ("Network & internet".into(), vec![
            nav("nav:wifi", "Internet", if t("wifi") { "Wi-Fi on" } else { "Wi-Fi off" }, i.get_wifi_f()),
            sw("t:dati", "Mobile data", if st.modem_ok { "Use data over the mobile network" } else { "Modem not available" }, t("dati")),
            sw("t:airplane", "Airplane mode", "Turns off all radios", t("airplane")),
            sw("t:hotspot", "Hotspot & tethering", "Share this phone's connection", t("hotspot")),
            sw("p:datasaver", "Data Saver", "Limits background data", pb(st, "datasaver", false)),
            val("opt:dns", "Private DNS", &opt(st, "dns")),
            h("SIM"),
            val("", "Carrier", &ui.global::<Settings>().get_modem_operator()),
            val("", "Signal", &ui.global::<Settings>().get_modem_signal()),
        ]),
        "devices" => {
            let mut v = vec![
                sw("t:bt", "Bluetooth", "Pair headphones, speakers and cars", t("bt")),
                h("Paired devices"),
            ];
            let paired = ui.global::<Settings>().get_bt_devices();
            use slint::Model;
            if paired.row_count() == 0 {
                v.push(note("No paired devices"));
            }
            for d in paired.iter() {
                v.push(nav("", &d, "Paired", i.get_bluetooth_f()));
            }
            v.extend([
                h("Connection preferences"),
                sw("p:nfc", "NFC", "Contactless with nearby devices", pb(st, "nfc", false)),
                val("opt:usb", "USB", &opt(st, "usb")),
                val("", "Cast", "Not available yet"),
            ]);
            ("Connected devices".into(), v)
        }
        "apps" => {
            let mut v = vec![h("Recently opened")];
            for r in st.recents.iter().take(3) {
                v.push(nav(&format!("app:{}", r.id), &r.name, if r.android { "Android app" } else { "System app" }, glyph(ui, &r.id)));
            }
            if st.recents.is_empty() {
                v.push(note("No apps opened yet"));
            }
            let n = catalog().len();
            v.push(nav("nav:apps_all", &format!("See all {n} apps"), "", i.get_apps()));
            v.extend([
                h("General"),
                val("", "Default browser", "Browser"),
                val("", "Default phone app", "Phone"),
                nav("nav:android", "Android apps", "Waydroid layer", i.get_android_f()),
            ]);
            ("Apps".into(), v)
        }
        "apps_all" => {
            let v = catalog()
                .into_iter()
                .map(|(id, name, android, _)| nav(&format!("app:{id}"), &name, if android { "Android app" } else { "System app" }, glyph(ui, &id)))
                .collect();
            ("All apps".into(), v)
        }
        "notifications" => ("Notifications".into(), vec![
            sw("t:dnd", "Do Not Disturb", "Silences calls and alerts", t("dnd")),
            sw("p:history", "Notification history", "Keep dismissed notifications", pb(st, "history", true)),
            sw("p:cooldown", "Notification cooldown", "Lower volume for quick bursts", pb(st, "cooldown", true)),
            h("Lock screen"),
            val("opt:lockscreen", "Notifications on lock screen", &opt(st, "lockscreen")),
            h("Advanced"),
            sw("p:flash", "Flash notifications", "Blink the screen for new alerts", pb(st, "flash", false)),
            sw("p:dots", "App icon badges", "Dots on icons with new items", pb(st, "dots", true)),
        ]),
        "display" => ("Display & touch".into(), vec![
            sl("sl:brightness", "Brightness", ui.global::<Sys>().get_brightness()),
            sw("p:adaptive", "Adaptive brightness", "Adjusts to your surroundings", pb(st, "adaptive", false)),
            h("Appearance"),
            sw("style:dark", "Dark theme", "Easier on the eyes at night", crate::ctl_style::is_dark(st.store.data.style.theme_mode)),
            nav("nav:style", "Wallpaper & style", "Colors, icons, app grid", i.get_palette_f()),
            sw("p:nightlight", "Night Light", "Tints the screen amber", pb(st, "nightlight", false)),
            h("Screen"),
            val("opt:timeout", "Screen timeout", &format!("After {} of inactivity", opt(st, "timeout"))),
            sw("t:rot", "Auto-rotate screen", "", t("rot")),
            sw("p:taptowake", "Tap to wake", "Double tap the screen to wake it", pb(st, "taptowake", true)),
        ]),
        "a11y" => ("Accessibility".into(), vec![
            h("Display"),
            sw("p:contrast", "High contrast text", "", pb(st, "contrast", false)),
            sw("p:invert", "Color inversion", "", pb(st, "invert", false)),
            sw("p:extradim", "Extra dim", "Dim the screen below minimum", pb(st, "extradim", false)),
            sw("p:noanim", "Remove animations", "", pb(st, "noanim", false)),
            h("Interaction"),
            sw("p:magnify", "Magnification", "Triple tap to zoom", pb(st, "magnify", false)),
            sw("p:powerend", "Power button ends call", "", pb(st, "powerend", true)),
            val("", "TalkBack", "Not available yet"),
        ]),
        "security" => ("Security & privacy".into(), vec![
            h("Device unlock"),
            val("opt:screenlock", "Screen lock", &opt(st, "screenlock")),
            sw("p:pinning", "App pinning", "Keep one app on screen", pb(st, "pinning", false)),
            h("Privacy"),
            sw("p:camaccess", "Camera access", "For all apps and services", pb(st, "camaccess", true)),
            sw("p:micaccess", "Microphone access", "For all apps and services", pb(st, "micaccess", true)),
            sw("p:clipboard", "Show clipboard access", "Message when apps read it", pb(st, "clipboard", true)),
            sw("p:passwords", "Show passwords", "Briefly show characters as you type", pb(st, "passwords", true)),
            h("Updates"),
            nav("nav:update", "System update", &format!("Cheip OS {}", crate::controller::VERSION), i.get_update_f()),
        ]),
        "location" => ("Location".into(), vec![
            sw("p:location", "Use location", "Apps with permission can find this phone", pb(st, "location", true)),
            h("Location services"),
            sw("p:wifiscan", "Wi-Fi scanning", "Improves location accuracy", pb(st, "wifiscan", true)),
            sw("p:btscan", "Bluetooth scanning", "Find nearby devices", pb(st, "btscan", false)),
            note("GPS needs the modem firmware. It works once the modem is supported on this phone."),
        ]),
        "system" => {
            let mut v = vec![
                val("", "Languages", "English (United States)"),
                nav("nav:gestures", "Gestures", "Navigation, quick actions", i.get_gesture()),
                nav("nav:datetime", "Date & time", &ui.global::<Settings>().get_timezone(), i.get_clock_f()),
                nav("nav:update", "System update", "", i.get_update_f()),
                nav("nav:reset", "Reset options", "", i.get_restart()),
            ];
            if pb(st, "dev", false) {
                v.push(nav("nav:developer", "Developer options", "", i.get_code()));
            }
            ("System".into(), v)
        }
        "gestures" => ("Gestures".into(), vec![
            h("System navigation"),
            radio("", "Gesture navigation", true),
            note("Swipe up from the bottom to go home. Swipe up and hold for recent apps. Swipe down from the top for quick settings."),
            h("Quick actions"),
            sw("p:powercam", "Quickly open camera", "Press power button twice", pb(st, "powercam", true)),
            sw("p:lift", "Lift to check phone", "", pb(st, "lift", false)),
        ]),
        "datetime" => ("Date & time".into(), vec![
            sw("p:autotime", "Set time automatically", "From the network", pb(st, "autotime", true)),
            val("", "Time zone", &ui.global::<Settings>().get_timezone()),
            sw("p:h24", "Use 24-hour format", "13:00", pb(st, "h24", true)),
        ]),
        "reset" => ("Reset options".into(), vec![
            act("act:resetnet", "Reset Wi-Fi, mobile & Bluetooth", "Forget saved networks and paired devices", i.get_wifi_off()),
            act("act:resetprefs", "Reset app preferences", "Settings go back to their defaults", i.get_tune()),
            danger("act:erase", "Erase all data", if ERASE_ARMED.with(|e| e.get()) { "Tap again to erase notes, messages, calls and alarms" } else { "Notes, messages, contacts, alarms" }),
        ]),
        "developer" => ("Developer options".into(), vec![
            sw("p:showtaps", "Show taps", "", pb(st, "showtaps", false)),
            sw("p:fps", "Show frame rate", "", pb(st, "fps", false)),
            val("opt:anim", "Animation scale", &opt(st, "anim")),
            sw("p:usbdebug", "SSH over USB", "Connect with ssh user@172.16.42.1", pb(st, "usbdebug", false)),
        ]),
        "about" => {
            let sysg = ui.global::<Sys>();
            ("About phone".into(), vec![
                val("", "Device name", &st.store.data.prefs.get("devname").cloned().unwrap_or_else(|| "Cheip phone".into())),
                val("", "Model", &sysg.get_model()),
                h("Software"),
                val("", "Cheip OS version", &sysg.get_version()),
                val("", "Kernel", &sysg.get_kernel()),
                val("act:build", "Build number", &format!("cheip-{}-mainline", crate::controller::VERSION)),
                h("Hardware"),
                val("", "Memory", &format!("{} MB", sysg.get_mem_total())),
                val("", "Storage", &ui.global::<Settings>().get_storage_total()),
            ])
        }
        _ => (page.to_string(), vec![]),
    }
}

/// Mostra in cima alla pila la pagina giusta (a mano o generica)
fn show(ui: &AppWindow, st: &State) {
    let top = STACK.with(|s| s.borrow().last().cloned());
    let g = ui.global::<Settings>();
    match top {
        None => g.set_page("".into()),
        Some(p) if CUSTOM.contains(&p.as_str()) => {
            g.set_page(p.clone().into());
            if st.live {
                match p.as_str() {
                    "wifi" => scan(ui, None),
                    "storage" => crate::ctl_files::push(ui, st),
                    _ => {}
                }
            }
        }
        Some(p) => {
            if st.live && p == "devices" {
                g.set_bt_devices(model(sys::bt_paired().into_iter().map(Into::into).collect()));
            }
            let (title, rows) = build(ui, st, &p);
            g.set_title(title.into());
            g.set_rows(model(rows));
            g.set_page("gen".into());
        }
    }
}

fn refresh(ui: &AppWindow) {
    if let Some(st) = shared() {
        show(ui, &st.borrow());
    }
}

/// Apre una pagina dall'esterno (per esempio Info app)
pub fn push_page(ui: &AppWindow, st: &State, page: &str) {
    STACK.with(|s| s.borrow_mut().push(page.to_string()));
    show(ui, st);
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let s = ui.global::<Settings>();
    s.set_timezone(if st.borrow().live { sys::timezone() } else { "Europe/Rome".into() }.into());

    s.on_open_page(with!(ui, st, |ui, s, page| {
        STACK.with(|k| *k.borrow_mut() = if page.is_empty() { vec![] } else { vec![page.to_string()] });
        ERASE_ARMED.with(|e| e.set(false));
        show(&ui, &s);
    }));
    s.on_back(with!(ui, st, |ui, s| {
        STACK.with(|k| {
            k.borrow_mut().pop();
        });
        show(&ui, &s);
    }));
    s.on_row_clicked(with!(ui, st, |ui, s, key| {
        let key = key.to_string();
        if let Some(p) = key.strip_prefix("nav:") {
            push_page(&ui, &s, p);
        } else if let Some(id) = key.strip_prefix("app:") {
            let Some((_, name, android, _)) = catalog().into_iter().find(|(a, _, _, _)| a == id) else { return };
            let set = ui.global::<Settings>();
            set.set_info_id(id.into());
            set.set_info_name(name.into());
            set.set_info_android(android);
            set.set_info_kind((if android { "Android app" } else { "System app" }).into());
            set.set_info_glyph(glyph(&ui, id));
            push_page(&ui, &s, "appinfo");
        } else if let Some(k) = key.strip_prefix("opt:") {
            let list = OPTIONS.iter().find(|(n, _)| *n == k).map(|(_, v)| *v).unwrap_or(&[]);
            if !list.is_empty() {
                let cur = opt(&s, k);
                let i = list.iter().position(|x| *x == cur).unwrap_or(0);
                s.store.data.prefs.insert(k.to_string(), list[(i + 1) % list.len()].to_string());
                s.store.save();
            }
            show(&ui, &s);
        } else if key == "act:build" {
            let n = BUILD_TAPS.with(|b| {
                b.set(b.get() + 1);
                b.get()
            });
            if pb(&s, "dev", false) {
                toast(&ui, "Developer options are already on");
            } else if n >= 7 {
                s.store.data.prefs.insert("dev".into(), "1".into());
                s.store.save();
                toast(&ui, "You are now a developer");
            } else if n >= 3 {
                toast(&ui, &format!("{} more taps to enable developer options", 7 - n));
            }
        } else if key == "act:resetnet" {
            toast(&ui, "Network settings reset");
        } else if key == "act:resetprefs" {
            let dev = s.store.data.prefs.get("dev").cloned();
            s.store.data.prefs.clear();
            if let Some(d) = dev {
                s.store.data.prefs.insert("dev".into(), d);
            }
            s.store.save();
            toast(&ui, "App preferences reset");
            show(&ui, &s);
        } else if key == "act:erase" {
            if ERASE_ARMED.with(|e| e.replace(true)) {
                ERASE_ARMED.with(|e| e.set(false));
                let d = &mut s.store.data;
                d.notes.clear();
                d.messages.clear();
                d.calls.clear();
                d.contacts.clear();
                d.events.clear();
                d.alarms.clear();
                s.store.save();
                crate::ctl_clock::push_alarms(&ui, &[]);
                toast(&ui, "All data erased");
            }
            show(&ui, &s);
        }
    }));
    s.on_row_toggled(|key, on| {
        let Some(ui) = crate::controller::ui_handle() else { return };
        let key = key.to_string();
        if let Some(id) = key.strip_prefix("t:") {
            ui.global::<Sys>().invoke_toggle(id.into());
        } else if key == "style:dark" {
            ui.global::<Style>().invoke_set_theme(if on { 0 } else { 1 });
        } else if let Some(k) = key.strip_prefix("p:") {
            if let Some(st) = shared() {
                let mut s = st.borrow_mut();
                s.store.data.prefs.insert(k.to_string(), if on { "1" } else { "0" }.into());
                s.store.save();
                if k == "h24" {
                    crate::controller::set_h24(on);
                }
            }
        }
        refresh(&ui);
    });
    s.on_row_slid(|key, v| {
        let Some(ui) = crate::controller::ui_handle() else { return };
        if key.as_str() == "sl:brightness" {
            let sysg = ui.global::<Sys>();
            sysg.set_brightness(v);
            sysg.invoke_brightness_changed(v);
        }
    });
    s.on_wifi_scan(with!(ui, st, |ui, s| {
        if s.live {
            scan(&ui, None);
        }
    }));
    s.on_wifi_connect(with!(ui, st, |ui, s, ssid, pass| {
        if !s.live {
            return;
        }
        let g = ui.global::<Settings>();
        g.set_wifi_busy(true);
        g.set_wifi_msg(format!("Connecting to {ssid}…").into());
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let res = sys::wifi_connect(&ssid, &pass);
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let msg = match res {
                    Ok(()) => format!("Connected to {ssid}"),
                    Err(e) => e,
                };
                scan(&ui, Some(msg));
            });
        });
    }));
}
