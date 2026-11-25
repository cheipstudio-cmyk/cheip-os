//! App File: navigazione nella home, anteprime, nuova cartella, eliminazione.

use crate::controller::{model, Shared, State};
use crate::files::{self, Kind};
use crate::with;
use crate::*;
use slint::ComponentHandle;

fn info(e: &files::Entry, now: i64) -> String {
    let age = now - e.modified;
    let when = if age < 86_400 {
        "oggi".to_string()
    } else if age < 2 * 86_400 {
        "ieri".to_string()
    } else {
        chrono::DateTime::from_timestamp(e.modified, 0).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()
    };
    match e.kind {
        Kind::Dir => format!("Cartella · {when}"),
        _ => format!("{} · {when}", files::human_size(e.size)),
    }
}

pub fn push(ui: &AppWindow, st: &State) {
    let f = ui.global::<Files>();
    let home = files::home();
    let at_home = st.cwd == home;
    f.set_path_label(
        (if at_home { "Home".to_string() } else { st.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() }).into(),
    );
    f.set_can_up(!at_home);
    let now = chrono::Local::now().timestamp();
    f.set_entries(model(
        st.entries
            .iter()
            .map(|e| FileRow {
                name: e.name.clone().into(),
                info: info(e, now).into(),
                kind: match e.kind {
                    Kind::Dir => 0,
                    Kind::Text => 1,
                    Kind::Image => 2,
                    Kind::Other => 3,
                },
            })
            .collect(),
    ));
    if let Some((used, total)) = files::disk_usage(&home) {
        f.set_storage(format!("{} liberi su {}", files::human_size(total.saturating_sub(used)), files::human_size(total)).into());
        let s = ui.global::<Settings>();
        s.set_storage_used(files::human_size(used).into());
        s.set_storage_total(files::human_size(total).into());
        s.set_storage_ratio(if total > 0 { used as f32 / total as f32 } else { 0.0 });
    }
}

fn refresh(ui: &AppWindow, st: &mut State) {
    st.entries = files::list(&st.cwd, false);
    push(ui, st);
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let f = ui.global::<Files>();
    f.on_open(with!(ui, st, |ui, s, i| {
        let Some(e) = s.entries.get(i as usize).cloned() else { return };
        let f = ui.global::<Files>();
        match e.kind {
            Kind::Dir => {
                if files::allowed(&e.path) {
                    s.cwd = e.path;
                    refresh(&ui, &mut s);
                }
            }
            Kind::Text => {
                f.set_preview_name(e.name.into());
                f.set_preview_text(files::preview_text(&e.path).into());
                f.set_preview_kind(1);
                f.set_preview_open(true);
            }
            Kind::Image => {
                f.set_preview_name(e.name.into());
                match slint::Image::load_from_path(&e.path) {
                    Ok(img) => {
                        f.set_preview_image(img);
                        f.set_preview_kind(2);
                    }
                    Err(_) => f.set_preview_kind(3),
                }
                f.set_preview_open(true);
            }
            Kind::Other => {
                f.set_preview_name(e.name.into());
                f.set_preview_kind(3);
                f.set_preview_open(true);
            }
        }
    }));
    f.on_up(with!(ui, st, |ui, s| {
        if let Some(p) = s.cwd.parent().map(|p| p.to_path_buf()) {
            if files::allowed(&p) {
                s.cwd = p;
                refresh(&ui, &mut s);
            }
        }
    }));
    f.on_mkdir(with!(ui, st, |ui, s, name| {
        let cwd = s.cwd.clone();
        if s.live {
            files::mkdir(&cwd, &name);
        }
        refresh(&ui, &mut s);
    }));
    f.on_remove(with!(ui, st, |ui, s, i| {
        let Some(e) = s.entries.get(i as usize).cloned() else { return };
        if s.live {
            files::remove(&e.path);
        }
        refresh(&ui, &mut s);
    }));
    f.on_close_preview(with!(ui, st, |ui, _s| {
        let f = ui.global::<Files>();
        f.set_preview_open(false);
        f.set_preview_image(slint::Image::default());
        f.set_preview_text("".into());
    }));
}
