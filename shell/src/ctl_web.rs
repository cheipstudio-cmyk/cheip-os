//! Browser in modalità lettura: schede, cronologia per scheda, preferiti.

use crate::controller::{model, Shared, State, Tab};
use crate::web::{self, Block, Page};
use crate::with;
use crate::*;
use slint::ComponentHandle;

fn blocks(p: &Page) -> Vec<WebBlock> {
    p.blocks
        .iter()
        .map(|b| {
            let (kind, text, href) = match b {
                Block::Heading(t, 1) => (0, t.clone(), String::new()),
                Block::Heading(t, 2) => (1, t.clone(), String::new()),
                Block::Heading(t, _) => (2, t.clone(), String::new()),
                Block::Para(t) => (3, t.clone(), String::new()),
                Block::Item(t) => (4, t.clone(), String::new()),
                Block::Link(t, h) => (5, t.clone(), h.clone()),
                Block::Code(t) => (6, t.clone(), String::new()),
                Block::Quote(t) => (7, t.clone(), String::new()),
            };
            WebBlock { kind, text: text.into(), href: href.into() }
        })
        .collect()
}

pub fn push(ui: &AppWindow, st: &State) {
    let w = ui.global::<Web>();
    let tab = &st.tabs[st.tab];
    let url = tab.history.last().cloned().unwrap_or_default();
    w.set_url(url.clone().into());
    w.set_title(tab.page.as_ref().map(|p| p.title.clone()).unwrap_or_default().into());
    w.set_loading(tab.loading);
    w.set_error(tab.error.clone().into());
    w.set_blocks(model(tab.page.as_ref().map(blocks).unwrap_or_default()));
    w.set_can_back(tab.history.len() > 1);
    w.set_current_tab(st.tab as i32);
    w.set_tabs(model(
        st.tabs
            .iter()
            .map(|t| TabRow {
                initial: crate::controller::initial(&t.page.as_ref().map(|p| p.title.clone()).unwrap_or_default()),
                title: t.page.as_ref().map(|p| p.title.clone()).unwrap_or_default().into(),
                url: t.history.last().cloned().unwrap_or_default().into(),
            })
            .collect(),
    ));
    w.set_bookmarked(!url.is_empty() && st.store.data.bookmarks.iter().any(|b| b.url == url));
    w.set_bookmarks(model(
        st.store.data.bookmarks.iter().map(|b| TabRow { initial: crate::controller::initial(&b.title), title: b.title.clone().into(), url: b.url.clone().into() }).collect(),
    ));
    w.set_scroll_reset(st.scroll_gen);
}

/// Mostra una pagina già pronta nella scheda corrente (screenshot e test)
pub fn show_page(ui: &AppWindow, st: &mut State, page: Page) {
    let t = &mut st.tabs[st.tab];
    t.history.push(page.url.clone());
    t.page = Some(page);
    t.loading = false;
    st.scroll_gen += 1;
    push(ui, st);
}

fn load(ui: &AppWindow, shared: &Shared, url: String) {
    // Demo e screenshot: niente rete, una pagina di esempio con l'indirizzo richiesto
    if !shared.borrow().live {
        let mut s = shared.borrow_mut();
        let mut page = web::parse(crate::demo::PAGE, &url);
        page.url = url;
        let i = s.tab;
        s.tabs[i].page = Some(page);
        s.tabs[i].loading = false;
        s.tabs[i].error.clear();
        s.scroll_gen += 1;
        push(ui, &s);
        return;
    }
    let tab_id = {
        let mut s = shared.borrow_mut();
        let i = s.tab;
        let t = &mut s.tabs[i];
        t.loading = true;
        t.error.clear();
        let id = t.id;
        push(ui, &s);
        id
    };
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let res = web::fetch(&url);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(shared) = crate::controller::shared() else { return };
            let mut s = shared.borrow_mut();
            let Some(t) = s.tabs.iter_mut().find(|t| t.id == tab_id) else { return };
            t.loading = false;
            match res {
                Ok(p) => {
                    // I redirect cambiano l'indirizzo: aggiorniamo la cronologia
                    if let Some(last) = t.history.last_mut() {
                        *last = p.url.clone();
                    }
                    t.page = Some(p);
                }
                Err(e) => {
                    t.page = None;
                    t.error = e;
                }
            }
            s.scroll_gen += 1;
            push(&ui, &s);
        });
    });
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let w = ui.global::<Web>();
    let shared = st.clone();
    let weak = ui.as_weak();
    w.on_go(move |input| {
        let Some(ui) = weak.upgrade() else { return };
        if input.trim().is_empty() {
            return;
        }
        let url = web::resolve_input(&input);
        {
            let mut s = shared.borrow_mut();
            let i = s.tab;
            s.tabs[i].history.push(url.clone());
        }
        load(&ui, &shared, url);
    });
    let shared = st.clone();
    let weak = ui.as_weak();
    w.on_back(move || {
        let Some(ui) = weak.upgrade() else { return };
        let url = {
            let mut s = shared.borrow_mut();
            let i = s.tab;
            let h = &mut s.tabs[i].history;
            if h.len() < 2 {
                return;
            }
            h.pop();
            h.last().cloned().unwrap_or_default()
        };
        load(&ui, &shared, url);
    });
    let shared = st.clone();
    let weak = ui.as_weak();
    w.on_reload(move || {
        let Some(ui) = weak.upgrade() else { return };
        let url = {
            let s = shared.borrow();
            s.tabs[s.tab].history.last().cloned()
        };
        if let Some(url) = url {
            load(&ui, &shared, url);
        }
    });
    w.on_new_tab(with!(ui, st, |ui, s| {
        s.tabs.push(Tab::default());
        s.tab = s.tabs.len() - 1;
        s.scroll_gen += 1;
        push(&ui, &s);
    }));
    w.on_select_tab(with!(ui, st, |ui, s, i| {
        if (i as usize) < s.tabs.len() {
            s.tab = i as usize;
            s.scroll_gen += 1;
            push(&ui, &s);
        }
    }));
    w.on_close_tab(with!(ui, st, |ui, s, i| {
        let i = i as usize;
        if i >= s.tabs.len() {
            return;
        }
        s.tabs.remove(i);
        if s.tabs.is_empty() {
            s.tabs.push(Tab::default());
        }
        if s.tab >= s.tabs.len() || s.tab > i {
            s.tab = s.tab.saturating_sub(1).min(s.tabs.len() - 1);
        }
        push(&ui, &s);
    }));
    w.on_toggle_bookmark(with!(ui, st, |ui, s| {
        let t = &s.tabs[s.tab];
        let Some(url) = t.history.last().cloned() else { return };
        let title = t.page.as_ref().map(|p| p.title.clone()).unwrap_or_else(|| url.clone());
        s.store.toggle_bookmark(&title, &url);
        s.store.save();
        push(&ui, &s);
    }));
}
