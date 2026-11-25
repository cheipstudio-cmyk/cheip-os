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

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Data {
    pub notes: Vec<Note>,
    pub events: Vec<Event>,
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
