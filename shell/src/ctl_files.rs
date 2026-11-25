//! App Files in stile Google Files: categorie, galleria, visore foto,
//! lettore video e audio, PDF e documenti, navigazione per cartelle.

use crate::controller::{model, Shared, State};
use crate::files::{self, Entry, Kind};
use crate::media::{self, Category};
use crate::player::{Event, Player};
use crate::with;
use crate::*;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub thumb: Option<PathBuf>,
    pub info: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Default)]
pub struct FilesState {
    pub all: Vec<Entry>,
    pub list: Vec<Entry>,
    pub view: String,
    pub meta: HashMap<PathBuf, Meta>,
    pub images: HashMap<PathBuf, Image>,
    pub viewer_list: Vec<Entry>,
    pub viewer_idx: usize,
    pub pdf: Option<(PathBuf, u32, u32)>,
    pub player: Option<Player>,
    pub video: bool,
    pub queue: Vec<Entry>,
    pub qidx: usize,
}

fn kind_id(k: &Kind) -> i32 {
    match k {
        Kind::Dir => 0,
        Kind::Text => 1,
        Kind::Image => 2,
        Kind::Other => 3,
        Kind::Video => 4,
        Kind::Audio => 5,
        Kind::Pdf => 6,
        Kind::Doc => 7,
    }
}

fn when(modified: i64) -> String {
    let age = chrono::Local::now().timestamp() - modified;
    if age < 86_400 {
        "Today".into()
    } else if age < 2 * 86_400 {
        "Yesterday".into()
    } else {
        chrono::DateTime::from_timestamp(modified, 0).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default()
    }
}

fn info(e: &Entry, meta: Option<&Meta>) -> String {
    if let Some(i) = meta.and_then(|m| m.info.clone()) {
        return i;
    }
    match e.kind {
        Kind::Dir => format!("Folder · {}", when(e.modified)),
        _ => format!("{} · {}", files::human_size(e.size), when(e.modified)),
    }
}

fn thumb_image(fx: &mut FilesState, p: &Path) -> Option<Image> {
    let t = fx.meta.get(p)?.thumb.clone()?;
    if let Some(img) = fx.images.get(&t) {
        return Some(img.clone());
    }
    let img = Image::load_from_path(&t).ok()?;
    fx.images.insert(t, img.clone());
    Some(img)
}

fn row(fx: &mut FilesState, e: &Entry) -> FileRow {
    let thumb = thumb_image(fx, &e.path);
    let meta = fx.meta.get(&e.path);
    let name = meta.and_then(|m| m.title.clone()).filter(|_| e.kind == Kind::Audio).unwrap_or_else(|| e.name.clone());
    FileRow {
        name: name.into(),
        info: info(e, meta).into(),
        kind: kind_id(&e.kind),
        has_thumb: thumb.is_some(),
        thumb: thumb.unwrap_or_default(),
    }
}

fn glyph(ui: &AppWindow, c: Category) -> slint::SharedString {
    let i = ui.global::<Icons>();
    match c {
        Category::Downloads => i.get_download(),
        Category::Images => i.get_picture(),
        Category::Videos => i.get_video(),
        Category::Audio => i.get_music(),
        Category::Documents => i.get_doc(),
    }
}

pub fn push_home(ui: &AppWindow, st: &State) {
    let home = files::home();
    let f = ui.global::<Files>();
    let cats = Category::ALL
        .into_iter()
        .map(|c| {
            let items = media::filter(&st.fx.all, c, &home);
            let n = items.len();
            CatRow {
                id: c.id().into(),
                label: c.label().into(),
                glyph: glyph(ui, c),
                detail: format!("{n} {} · {}", if n == 1 { "item" } else { "items" }, files::human_size(media::total_size(&items))).into(),
            }
        })
        .collect();
    f.set_categories(model(cats));
    if let Some((used, total)) = files::disk_usage(&home) {
        f.set_storage(format!("{} free of {}", files::human_size(total.saturating_sub(used)), files::human_size(total)).into());
        f.set_storage_ratio(if total > 0 { used as f32 / total as f32 } else { 0.0 });
        let s = ui.global::<Settings>();
        s.set_storage_used(files::human_size(used).into());
        s.set_storage_total(files::human_size(total).into());
        s.set_storage_ratio(if total > 0 { used as f32 / total as f32 } else { 0.0 });
    }
}

fn push_list(ui: &AppWindow, st: &mut State) {
    let fx = &mut st.fx;
    let list = fx.list.clone();
    let rows: Vec<FileRow> = list.iter().map(|e| row(fx, e)).collect();
    let f = ui.global::<Files>();
    let n = rows.len();
    f.set_count_label(format!("{n} {}", if n == 1 { "item" } else { "items" }).into());
    f.set_entries(model(rows));
    let home = files::home();
    let at_home = st.cwd == home;
    f.set_path_label((if at_home { "Internal storage".to_string() } else { st.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() }).into());
    f.set_can_up(!at_home);
}

pub fn push(ui: &AppWindow, st: &State) {
    push_home(ui, st);
}

// ---------- miniature e metadati, fuori dal thread della UI ----------

/// Calcola miniature e metadati mancanti. Lento: va chiamata in un thread.
pub fn build_meta(items: &[Entry]) -> Vec<(PathBuf, Meta)> {
    items
        .iter()
        .filter_map(|e| {
            let dst = media::thumb_path(&e.path);
            let mut m = Meta::default();
            match e.kind {
                Kind::Image => {
                    if dst.exists() || media::image_thumb(&e.path, &dst, 240) {
                        m.thumb = Some(dst);
                    }
                }
                Kind::Video => {
                    let dur = media::video_thumb(&e.path, &dst, 320);
                    if dst.exists() {
                        m.thumb = Some(dst);
                    }
                    if let Some(d) = dur {
                        m.info = Some(media::fmt_duration(d));
                    }
                }
                Kind::Audio => {
                    let a = media::audio_meta(&e.path);
                    if let Some(c) = &a.cover {
                        if dst.exists() || media::cover_thumb(c, &dst, 240) {
                            m.thumb = Some(dst);
                        }
                    }
                    let who = if a.artist.is_empty() { "Unknown artist".to_string() } else { a.artist.clone() };
                    m.info = Some(if a.secs > 0 { format!("{who} · {}", media::fmt_duration(a.secs)) } else { who });
                    m.title = Some(a.title);
                    m.artist = Some(a.artist);
                }
                _ => return None,
            }
            Some((e.path.clone(), m))
        })
        .collect()
}

pub fn apply_meta(ui: &AppWindow, st: &mut State, res: Vec<(PathBuf, Meta)>) {
    for (p, m) in res {
        st.fx.meta.insert(p, m);
    }
    push_list(ui, st);
}

fn spawn_meta(ui: &AppWindow, items: Vec<Entry>) {
    let todo: Vec<Entry> = items.into_iter().filter(|e| matches!(e.kind, Kind::Image | Kind::Video | Kind::Audio)).take(400).collect();
    if todo.is_empty() {
        return;
    }
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        // A blocchi, così la griglia si riempie man mano
        for chunk in todo.chunks(12) {
            let res = build_meta(chunk);
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let (Some(ui), Some(st)) = (weak.upgrade(), crate::controller::shared()) else { return };
                apply_meta(&ui, &mut st.borrow_mut(), res);
            });
        }
    });
}

/// Indicizza la home: in un thread sul telefono, subito negli screenshot
pub fn index(ui: &AppWindow, st: &Shared, background: bool) {
    let home = files::home();
    if !background {
        let mut s = st.borrow_mut();
        s.fx.all = media::scan(&home);
        push_home(ui, &s);
        return;
    }
    ui.global::<Files>().set_scanning(true);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let all = media::scan(&home);
        let _ = slint::invoke_from_event_loop(move || {
            let (Some(ui), Some(st)) = (weak.upgrade(), crate::controller::shared()) else { return };
            let mut s = st.borrow_mut();
            s.fx.all = all;
            ui.global::<Files>().set_scanning(false);
            push_home(&ui, &s);
        });
    });
}

fn refresh_folder(ui: &AppWindow, st: &mut State) {
    st.entries = files::list(&st.cwd, false);
    st.fx.list = st.entries.clone();
    push_list(ui, st);
}

fn open_category(ui: &AppWindow, st: &mut State, id: &str) {
    let f = ui.global::<Files>();
    if id == "folder" {
        st.fx.view = "folder".into();
        f.set_view("folder".into());
        refresh_folder(ui, st);
        spawn_meta(ui, st.fx.list.clone());
        return;
    }
    let Some(cat) = Category::from_id(id) else { return };
    st.fx.view = id.into();
    st.fx.list = media::filter(&st.fx.all, cat, &files::home());
    f.set_title(cat.label().into());
    f.set_view(id.into());
    push_list(ui, st);
    if st.live {
        spawn_meta(ui, st.fx.list.clone());
    }
}

// ---------- visori e lettore ----------

fn show_image(ui: &AppWindow, st: &mut State) {
    let Some(e) = st.fx.viewer_list.get(st.fx.viewer_idx).cloned() else { return };
    let f = ui.global::<Files>();
    f.set_viewer_title(e.name.clone().into());
    f.set_viewer_pos(format!("{} / {}", st.fx.viewer_idx + 1, st.fx.viewer_list.len()).into());
    // Immagine intera ridotta a 720 px: una foto da 13 MP non deve entrare tutta in RAM
    let img = image::open(&e.path).ok().map(|i| i.thumbnail(720, 1280).to_rgba8()).map(|i| {
        let (w, h) = (i.width(), i.height());
        Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(i.as_raw(), w, h))
    });
    f.set_viewer_image(img.unwrap_or_default());
    f.set_viewer("image".into());
}

fn show_pdf_page(ui: &AppWindow, st: &mut State) {
    let Some((p, page, pages)) = st.fx.pdf.clone() else { return };
    let f = ui.global::<Files>();
    f.set_viewer_pos(format!("Page {page} / {pages}").into());
    let dst = media::cache_dir().join(format!("pdf-{}-{page}.png", media::thumb_path(&p).file_stem().unwrap_or_default().to_string_lossy()));
    if dst.exists() || media::pdf_render(&p, page, 720, &dst) {
        if let Ok(img) = Image::load_from_path(&dst) {
            f.set_viewer_image(img);
        }
    }
}

fn ensure_player(ui: &AppWindow, st: &mut State) -> bool {
    if st.fx.player.is_some() {
        return true;
    }
    let weak = ui.as_weak();
    let weak = std::sync::Mutex::new(weak);
    match Player::new(720, move |w, h, data| {
        let weak = weak.lock().map(|g| g.clone()).ok();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.and_then(|w| w.upgrade()) else { return };
            let f = ui.global::<Files>();
            if f.get_viewer() == "video" {
                f.set_viewer_image(Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&data, w, h)));
            }
        });
    }) {
        Ok(p) => {
            st.fx.player = Some(p);
            true
        }
        Err(e) => {
            ui.global::<Files>().set_status(e.into());
            false
        }
    }
}

fn play_track(ui: &AppWindow, st: &mut State) {
    let Some(e) = st.fx.queue.get(st.fx.qidx).cloned() else { return };
    if !ensure_player(ui, st) {
        return;
    }
    st.fx.video = false;
    if let Some(p) = st.fx.player.as_mut() {
        let _ = p.load(&e.path);
    }
    let meta = st.fx.meta.get(&e.path).cloned().unwrap_or_default();
    let (title, artist, cover) = match &meta.title {
        Some(t) => (t.clone(), meta.artist.clone().unwrap_or_default(), thumb_image(&mut st.fx, &e.path)),
        None => {
            // Metadati non ancora pronti: li leggiamo ora, è un file solo
            let res = build_meta(std::slice::from_ref(&e));
            for (p, m) in res {
                st.fx.meta.insert(p, m);
            }
            let m = st.fx.meta.get(&e.path).cloned().unwrap_or_default();
            (m.title.unwrap_or(e.name.clone()), m.artist.unwrap_or_default(), thumb_image(&mut st.fx, &e.path))
        }
    };
    let f = ui.global::<Files>();
    f.set_track_title(title.into());
    f.set_track_artist((if artist.is_empty() { "Unknown artist".to_string() } else { artist }).into());
    f.set_has_cover(cover.is_some());
    f.set_track_cover(cover.unwrap_or_default());
    f.set_audio_active(true);
    f.set_playing(true);
    f.set_progress(0.0);
}

fn stop_player(ui: &AppWindow, st: &mut State) {
    if let Some(p) = st.fx.player.as_mut() {
        p.stop();
    }
    let f = ui.global::<Files>();
    f.set_playing(false);
    f.set_audio_active(false);
    f.set_audio_full(false);
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let live = st.borrow().live;
    if live {
        index(ui, st, true);
    }
    let f = ui.global::<Files>();
    f.on_open_home(with!(ui, st, |ui, s| {
        s.fx.view = "home".into();
        ui.global::<Files>().set_view("home".into());
        push_home(&ui, &s);
    }));
    f.on_open_storage(with!(ui, st, |ui, s| {
        s.cwd = files::home();
        open_category(&ui, &mut s, "folder");
    }));
    f.on_open_category(with!(ui, st, |ui, s, id| { open_category(&ui, &mut s, &id); }));
    f.on_open(with!(ui, st, |ui, s, i| {
        let Some(e) = s.fx.list.get(i as usize).cloned() else { return };
        let f = ui.global::<Files>();
        match e.kind {
            Kind::Dir => {
                if files::allowed(&e.path) {
                    s.cwd = e.path;
                    refresh_folder(&ui, &mut s);
                    spawn_meta(&ui, s.fx.list.clone());
                }
            }
            Kind::Image => {
                s.fx.viewer_list = s.fx.list.iter().filter(|x| x.kind == Kind::Image).cloned().collect();
                s.fx.viewer_idx = s.fx.viewer_list.iter().position(|x| x.path == e.path).unwrap_or(0);
                show_image(&ui, &mut s);
            }
            Kind::Video => {
                // Un solo lettore: il video ferma la musica
                if !ensure_player(&ui, &mut s) {
                    return;
                }
                f.set_audio_active(false);
                f.set_audio_full(false);
                s.fx.video = true;
                if let Some(p) = s.fx.player.as_mut() {
                    let _ = p.load(&e.path);
                }
                let thumb = thumb_image(&mut s.fx, &e.path);
                f.set_viewer_image(thumb.unwrap_or_default());
                f.set_viewer_title(e.name.into());
                f.set_viewer_pos("".into());
                f.set_playing(true);
                f.set_viewer("video".into());
            }
            Kind::Audio => {
                s.fx.queue = s.fx.list.iter().filter(|x| x.kind == Kind::Audio).cloned().collect();
                s.fx.qidx = s.fx.queue.iter().position(|x| x.path == e.path).unwrap_or(0);
                play_track(&ui, &mut s);
                f.set_audio_full(true);
            }
            Kind::Pdf => {
                let pages = media::pdf_pages(&e.path).unwrap_or(0);
                f.set_viewer_title(e.name.clone().into());
                if pages == 0 {
                    f.set_viewer("none".into());
                    return;
                }
                s.fx.pdf = Some((e.path.clone(), 1, pages));
                f.set_viewer_image(Image::default());
                f.set_viewer("pdf".into());
                show_pdf_page(&ui, &mut s);
            }
            Kind::Doc | Kind::Text => {
                let text = if e.kind == Kind::Doc { media::office_text(&e.path).unwrap_or_default() } else { files::preview_text(&e.path) };
                f.set_viewer_title(e.name.into());
                f.set_viewer_pos("".into());
                f.set_viewer_text(text.into());
                f.set_viewer("text".into());
            }
            Kind::Other => {
                f.set_viewer_title(e.name.into());
                f.set_viewer_pos("".into());
                f.set_viewer("none".into());
            }
        }
    }));
    f.on_next(with!(ui, st, |ui, s| {
        let v = ui.global::<Files>().get_viewer();
        if v == "image" && s.fx.viewer_idx + 1 < s.fx.viewer_list.len() {
            s.fx.viewer_idx += 1;
            show_image(&ui, &mut s);
        } else if v == "pdf" {
            if let Some((_, page, pages)) = s.fx.pdf.as_mut() {
                if *page < *pages {
                    *page += 1;
                }
            }
            show_pdf_page(&ui, &mut s);
        }
    }));
    f.on_prev(with!(ui, st, |ui, s| {
        let v = ui.global::<Files>().get_viewer();
        if v == "image" && s.fx.viewer_idx > 0 {
            s.fx.viewer_idx -= 1;
            show_image(&ui, &mut s);
        } else if v == "pdf" {
            if let Some((_, page, _)) = s.fx.pdf.as_mut() {
                if *page > 1 {
                    *page -= 1;
                }
            }
            show_pdf_page(&ui, &mut s);
        }
    }));
    f.on_close_viewer(with!(ui, st, |ui, s| {
        let f = ui.global::<Files>();
        if f.get_viewer() == "video" {
            if let Some(p) = s.fx.player.as_mut() {
                p.stop();
            }
            f.set_playing(false);
        }
        f.set_viewer("".into());
        f.set_viewer_image(Image::default());
        f.set_viewer_text("".into());
        s.fx.pdf = None;
    }));
    f.on_play_pause(with!(ui, st, |ui, s| {
        if let Some(p) = s.fx.player.as_mut() {
            let play = !p.is_playing();
            p.set_playing(play);
            ui.global::<Files>().set_playing(play);
        }
    }));
    f.on_seek(with!(ui, st, |_ui, s, v| {
        if let Some(p) = s.fx.player.as_ref() {
            p.seek_fraction(v);
        }
    }));
    f.on_track_next(with!(ui, st, |ui, s| {
        if s.fx.qidx + 1 < s.fx.queue.len() {
            s.fx.qidx += 1;
            play_track(&ui, &mut s);
        }
    }));
    f.on_track_prev(with!(ui, st, |ui, s| {
        // Come tutti i lettori: dopo 3 secondi "indietro" riparte dall'inizio
        let pos = s.fx.player.as_ref().map(|p| p.position()).unwrap_or(0);
        if pos > 3 || s.fx.qidx == 0 {
            if let Some(p) = s.fx.player.as_ref() {
                p.seek_fraction(0.0);
            }
        } else {
            s.fx.qidx -= 1;
            play_track(&ui, &mut s);
        }
    }));
    f.on_stop_audio(with!(ui, st, |ui, s| { stop_player(&ui, &mut s); }));
    f.on_up(with!(ui, st, |ui, s| {
        if let Some(p) = s.cwd.parent().map(|p| p.to_path_buf()) {
            if files::allowed(&p) {
                s.cwd = p;
                refresh_folder(&ui, &mut s);
            }
        }
    }));
    f.on_mkdir(with!(ui, st, |ui, s, name| {
        let cwd = s.cwd.clone();
        if s.live {
            files::mkdir(&cwd, &name);
        }
        refresh_folder(&ui, &mut s);
    }));
    f.on_remove(with!(ui, st, |ui, s, i| {
        let Some(e) = s.fx.list.get(i as usize).cloned() else { return };
        if s.live && files::remove(&e.path) {
            s.fx.all.retain(|x| !x.path.starts_with(&e.path));
        }
        s.fx.list.retain(|x| x.path != e.path);
        if s.fx.view == "folder" {
            refresh_folder(&ui, &mut s);
        } else {
            push_list(&ui, &mut s);
        }
    }));
}

/// Avanzamento del lettore e fine traccia, 4 volte al secondo
pub fn timer(ui: &AppWindow, st: &Shared) -> slint::Timer {
    let t = slint::Timer::default();
    t.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        with!(ui, st, |ui, s| {
            let Some(p) = s.fx.player.as_ref() else { return };
            let f = ui.global::<Files>();
            if !f.get_audio_active() && f.get_viewer() != "video" {
                return;
            }
            let (pos, dur) = (p.position(), p.duration());
            f.set_pos_label(media::fmt_duration(pos).into());
            f.set_dur_label(media::fmt_duration(dur).into());
            f.set_progress(if dur > 0 { pos as f32 / dur as f32 } else { 0.0 });
            match p.poll() {
                Event::Eos => {
                    if s.fx.video {
                        if let Some(p) = s.fx.player.as_mut() {
                            p.set_playing(false);
                        }
                        f.set_playing(false);
                    } else if s.fx.qidx + 1 < s.fx.queue.len() {
                        s.fx.qidx += 1;
                        play_track(&ui, &mut s);
                    } else {
                        stop_player(&ui, &mut s);
                    }
                }
                Event::Error(e) => {
                    f.set_status(e.into());
                    f.set_playing(false);
                }
                Event::None => {}
            }
        }),
    );
    t
}
