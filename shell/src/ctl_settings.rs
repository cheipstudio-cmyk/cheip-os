//! Impostazioni: pagine Wi-Fi, Bluetooth, rete mobile, archiviazione, ecc.

use crate::controller::{model, Shared};
use crate::system as sys;
use crate::with;
use crate::*;
use slint::ComponentHandle;

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
            let label = msg.unwrap_or_else(|| if nets.is_empty() { "Nessuna rete trovata".into() } else { format!("{} reti", nets.len()) });
            s.set_wifi_msg(label.into());
            s.set_networks(model(wifi_rows(nets)));
        });
    });
}

/// Reti finte per screenshot e test
pub fn demo_networks(ui: &AppWindow, nets: Vec<sys::WifiNet>) {
    let s = ui.global::<Settings>();
    s.set_wifi_msg(format!("{} reti", nets.len()).into());
    s.set_networks(model(wifi_rows(nets)));
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let s = ui.global::<Settings>();
    s.set_timezone(if st.borrow().live { sys::timezone() } else { "Europe/Rome".into() }.into());

    s.on_open_page(with!(ui, st, |ui, s, page| {
        let g = ui.global::<Settings>();
        g.set_page(page.clone());
        if !s.live {
            return;
        }
        match page.as_str() {
            "wifi" => scan(&ui, None),
            "bt" => g.set_bt_devices(model(sys::bt_paired().into_iter().map(Into::into).collect())),
            "storage" => crate::ctl_files::push(&ui, &s),
            _ => {}
        }
    }));
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
        g.set_wifi_msg(format!("Connessione a {ssid}…").into());
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let res = sys::wifi_connect(&ssid, &pass);
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let msg = match res {
                    Ok(()) => format!("Connesso a {ssid}"),
                    Err(e) => e,
                };
                scan(&ui, Some(msg));
            });
        });
    }));
}
