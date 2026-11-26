//! Aspetto: tema chiaro e scuro, sfondo, palette, forme e pacchetti icone.

use crate::controller::{push_apps, push_toggles, Shared, State};
use crate::with;
use crate::*;
use chrono::{Local, Timelike};
use slint::ComponentHandle;

pub fn is_dark(mode: i32) -> bool {
    match mode {
        0 => true,
        1 => false,
        _ => crate::style::auto_dark(Local::now().hour()),
    }
}

/// Applica allo schermo l'aspetto salvato
pub fn apply_from(ui: &AppWindow, st: &State) {
    let c = &st.store.data.style;
    crate::style::apply(ui, is_dark(c.theme_mode), c.wallpaper, c.palette, c.scheme);
    let g = ui.global::<Style>();
    g.set_theme_mode(c.theme_mode);
    g.set_wallpaper(c.wallpaper);
    g.set_palette(c.palette);
    g.set_scheme(c.scheme);
    g.set_icon_shape(c.icon_shape);
    g.set_icon_pack(c.icon_pack);
}

fn sync_dark_toggle(ui: &AppWindow, st: &mut State) {
    let dark = is_dark(st.store.data.style.theme_mode);
    if let Some(t) = st.toggles.iter_mut().find(|t| t.id == "dark") {
        t.on = dark;
    }
    push_toggles(ui, st);
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    {
        let mut s = st.borrow_mut();
        apply_from(ui, &s);
        sync_dark_toggle(ui, &mut s);
    }
    let g = ui.global::<Style>();
    macro_rules! setter {
        ($cb:ident, $field:ident, $apps:expr) => {
            g.$cb(with!(ui, st, |ui, s, v| {
                s.store.data.style.$field = v;
                if stringify!($field) == "wallpaper" {
                    s.store.data.style.palette = 0;
                }
                s.store.save();
                apply_from(&ui, &s);
                sync_dark_toggle(&ui, &mut s);
                if $apps {
                    push_apps(&ui, &s);
                }
            }));
        };
    }
    setter!(on_set_theme, theme_mode, false);
    setter!(on_set_wallpaper, wallpaper, false);
    setter!(on_set_palette, palette, false);
    setter!(on_set_scheme, scheme, false);
    setter!(on_set_icon_shape, icon_shape, false);
    setter!(on_set_icon_pack, icon_pack, true);
}
