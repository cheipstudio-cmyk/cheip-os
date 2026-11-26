//! Note ed eventi salvati in JSON nella cartella dati dell'utente.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub id: i32,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    /// AAAA-MM-GG
    pub date: String,
    /// HH:MM
    pub time: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Contact {
    pub name: String,
    pub number: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CallLog {
    pub number: String,
    pub ts: i64,
    /// "out", "in", "missed"
    pub dir: String,
    pub secs: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub number: String,
    pub text: String,
    pub ts: i64,
    pub outgoing: bool,
    #[serde(default)]
    pub read: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Data {
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default)]
    pub calls: Vec<CallLog>,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Ordine delle icone nella griglia della home
    #[serde(default)]
    pub app_order: Vec<String>,
    /// App fisse nel dock, al massimo 4
    #[serde(default = "default_dock")]
    pub dock: Vec<String>,
    /// Aspetto: tema, sfondo, colori, icone
    #[serde(default)]
    pub style: StyleCfg,
    /// Impostazioni semplici chiave valore
    #[serde(default)]
    pub prefs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub alarms: Vec<Alarm>,
    #[serde(default = "default_cities")]
    pub cities: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct StyleCfg {
    /// 0 scuro, 1 chiaro, 2 automatico
    pub theme_mode: i32,
    pub wallpaper: i32,
    /// 0..2 colori dallo sfondo, poi colori base
    pub palette: i32,
    /// 0 tonal spot, 1 vibrante, 2 espressivo, 3 neutro, 4 monocromo
    pub scheme: i32,
    pub icon_shape: i32,
    pub icon_pack: i32,
}

impl Default for StyleCfg {
    fn default() -> Self {
        StyleCfg { theme_mode: 0, wallpaper: 1, palette: 0, scheme: 0, icon_shape: 2, icon_pack: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Alarm {
    pub id: i32,
    pub h: u32,
    pub m: u32,
    #[serde(default)]
    pub label: String,
    /// Bit 0 lunedi ... bit 6 domenica. 0 = una volta sola
    #[serde(default)]
    pub days: u8,
    pub on: bool,
}

pub fn default_cities() -> Vec<String> {
    ["New York", "London", "Tokyo"].iter().map(|s| s.to_string()).collect()
}

pub fn default_dock() -> Vec<String> {
    ["phone", "messages", "browser", "camera"].iter().map(|s| s.to_string()).collect()
}

pub const DOCK_MAX: usize = 4;

/// Sposta un elemento da `from` a `to` (indici nella lista visibile)
pub fn move_item(list: &mut Vec<String>, from: usize, to: usize) {
    if from >= list.len() || to >= list.len() || from == to {
        return;
    }
    let x = list.remove(from);
    list.insert(to, x);
}

/// Confronto numeri ignorando spazi e prefisso internazionale italiano
pub fn same_number(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        d.strip_prefix("0039").or_else(|| d.strip_prefix("39").filter(|r| r.len() >= 9)).unwrap_or(&d).to_string()
    };
    let (x, y) = (norm(a), norm(b));
    !x.is_empty() && x == y
}

pub struct Store {
    path: Option<PathBuf>,
    pub data: Data,
}

impl Store {
    pub fn data_dir() -> PathBuf {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(x).join("cheip");
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".local/share/cheip")
    }

    pub fn open(dir: PathBuf) -> Self {
        let path = dir.join("data.json");
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Store { path: Some(path), data }
    }

    /// Solo in memoria, per gli screenshot e i test
    pub fn memory(data: Data) -> Self {
        Store { path: None, data }
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        // Scrittura atomica: un crash a metà non corrompe le note
        let tmp = path.with_extension("json.tmp");
        if let Ok(s) = serde_json::to_string_pretty(&self.data) {
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, path);
            }
        }
    }

    pub fn upsert_note(&mut self, id: i32, title: &str, body: &str, now: i64) -> i32 {
        if let Some(n) = self.data.notes.iter_mut().find(|n| n.id == id) {
            n.title = title.to_string();
            n.body = body.to_string();
            n.updated = now;
            return id;
        }
        let new_id = self.data.notes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        self.data.notes.push(Note { id: new_id, title: title.to_string(), body: body.to_string(), pinned: false, updated: now });
        new_id
    }

    pub fn toggle_pin(&mut self, id: i32) {
        if let Some(n) = self.data.notes.iter_mut().find(|n| n.id == id) {
            n.pinned = !n.pinned;
        }
    }

    pub fn remove_note(&mut self, id: i32) {
        self.data.notes.retain(|n| n.id != id);
    }

    /// Fissate prima, poi le più recenti
    pub fn sorted_notes(&self, only_pinned: bool) -> Vec<Note> {
        let mut v: Vec<Note> = self.data.notes.iter().filter(|n| !only_pinned || n.pinned).cloned().collect();
        v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated.cmp(&a.updated)));
        v
    }

    pub fn add_event(&mut self, date: &str, time: &str, title: &str) {
        self.data.events.push(Event { date: date.into(), time: time.into(), title: title.into() });
    }

    pub fn events_on(&self, date: &str) -> Vec<Event> {
        let mut v: Vec<Event> = self.data.events.iter().filter(|e| e.date == date).cloned().collect();
        v.sort_by(|a, b| a.time.cmp(&b.time));
        v
    }

    pub fn has_events(&self, date: &str) -> bool {
        self.data.events.iter().any(|e| e.date == date)
    }

    pub fn contact_name(&self, number: &str) -> Option<String> {
        self.data.contacts.iter().find(|c| same_number(&c.number, number)).map(|c| c.name.clone())
    }

    pub fn add_contact(&mut self, name: &str, number: &str) {
        if let Some(c) = self.data.contacts.iter_mut().find(|c| same_number(&c.number, number)) {
            c.name = name.to_string();
            return;
        }
        self.data.contacts.push(Contact { name: name.into(), number: number.into() });
        self.data.contacts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    pub fn log_call(&mut self, number: &str, ts: i64, dir: &str, secs: i64) {
        self.data.calls.insert(0, CallLog { number: number.into(), ts, dir: dir.into(), secs });
        self.data.calls.truncate(200);
    }

    /// Aggiunge un messaggio se non c'è già (lo stesso SMS può essere letto due volte dal modem)
    pub fn add_message(&mut self, m: Message) -> bool {
        let dup = self.data.messages.iter().any(|x| x.ts == m.ts && x.text == m.text && same_number(&x.number, &m.number));
        if !dup {
            self.data.messages.push(m);
        }
        !dup
    }

    /// Conversazioni: (numero, ultimo messaggio, non letti), dalla più recente
    pub fn threads(&self) -> Vec<(String, Message, usize)> {
        let mut out: Vec<(String, Message, usize)> = Vec::new();
        for m in &self.data.messages {
            match out.iter_mut().find(|(n, _, _)| same_number(n, &m.number)) {
                Some(t) => {
                    if m.ts >= t.1.ts {
                        t.1 = m.clone();
                    }
                    if !m.outgoing && !m.read {
                        t.2 += 1;
                    }
                }
                None => out.push((m.number.clone(), m.clone(), usize::from(!m.outgoing && !m.read))),
            }
        }
        out.sort_by(|a, b| b.1.ts.cmp(&a.1.ts));
        out
    }

    pub fn conversation(&mut self, number: &str) -> Vec<Message> {
        for m in self.data.messages.iter_mut().filter(|m| same_number(&m.number, number)) {
            m.read = true;
        }
        let mut v: Vec<Message> = self.data.messages.iter().filter(|m| same_number(&m.number, number)).cloned().collect();
        v.sort_by_key(|m| m.ts);
        v
    }

    pub fn toggle_bookmark(&mut self, title: &str, url: &str) -> bool {
        if let Some(i) = self.data.bookmarks.iter().position(|b| b.url == url) {
            self.data.bookmarks.remove(i);
            false
        } else {
            self.data.bookmarks.insert(0, Bookmark { title: title.into(), url: url.into() });
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_roundtrip_and_order() {
        let mut s = Store::memory(Data::default());
        let a = s.upsert_note(-1, "Spesa", "Latte", 1);
        let b = s.upsert_note(-1, "Idee", "Gesture", 2);
        assert_eq!((a, b), (1, 2));
        s.toggle_pin(a);
        let v = s.sorted_notes(false);
        assert_eq!(v[0].id, a, "le fissate vanno in cima");
        s.upsert_note(b, "Idee OS", "Gesture", 3);
        assert_eq!(s.data.notes.iter().find(|n| n.id == b).unwrap().title, "Idee OS");
        s.remove_note(a);
        assert_eq!(s.sorted_notes(true).len(), 0);
    }

    #[test]
    fn events_sorted_by_time() {
        let mut s = Store::memory(Data::default());
        s.add_event("2026-11-25", "23:00", "Review");
        s.add_event("2026-11-25", "18:00", "Spesa");
        let v = s.events_on("2026-11-25");
        assert_eq!(v[0].title, "Spesa");
        assert!(s.has_events("2026-11-25"));
        assert!(!s.has_events("2026-11-26"));
    }

    #[test]
    fn numbers_and_threads() {
        assert!(same_number("+39 333 123 4567", "3331234567"));
        assert!(same_number("0039 333 1234567", "333-1234567"));
        assert!(!same_number("3331234567", "3331234568"));
        let mut s = Store::memory(Data::default());
        s.add_contact("Marco", "+39 333 1234567");
        assert_eq!(s.contact_name("3331234567").as_deref(), Some("Marco"));
        let m = |n: &str, t: &str, ts, o| Message { number: n.into(), text: t.into(), ts, outgoing: o, read: false };
        assert!(s.add_message(m("3331234567", "Ciao", 10, false)));
        assert!(!s.add_message(m("+393331234567", "Ciao", 10, false)), "duplicato");
        s.add_message(m("+393331234567", "Ehi", 20, true));
        s.add_message(m("3489999999", "Pacco in consegna", 15, false));
        let t = s.threads();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].1.text, "Ehi");
        assert_eq!(t[0].2, 1, "un non letto");
        assert_eq!(s.conversation("3331234567").len(), 2);
        assert_eq!(s.threads()[0].2, 0, "letti dopo l'apertura");
    }

    #[test]
    fn old_data_file_still_loads() {
        let d: Data = serde_json::from_str(r#"{"notes":[],"events":[]}"#).unwrap();
        assert!(d.contacts.is_empty());
        assert_eq!(d.dock.len(), 4, "dock predefinito");
    }

    #[test]
    fn reorder() {
        let mut v: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        move_item(&mut v, 0, 2);
        assert_eq!(v, ["b", "c", "a", "d"]);
        move_item(&mut v, 3, 0);
        assert_eq!(v, ["d", "b", "c", "a"]);
        move_item(&mut v, 9, 0);
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn save_and_reload() {
        let dir = std::env::temp_dir().join(format!("cheip-test-{}", std::process::id()));
        let mut s = Store::open(dir.clone());
        s.upsert_note(-1, "Prova", "Corpo", 5);
        s.save();
        let r = Store::open(dir.clone());
        assert_eq!(r.data.notes.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
