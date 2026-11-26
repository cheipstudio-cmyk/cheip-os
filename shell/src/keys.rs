//! Tasti fisici del telefono (volume e accensione) letti da evdev,
//! in un thread: i Lumia li espongono come gpio-keys o pm8941-pwrkey.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    VolumeUp,
    VolumeDown,
    Power,
}

#[cfg(feature = "native")]
pub fn listen<F: Fn(Key) + Send + Sync + Clone + 'static>(on_key: F) {
    use evdev::{EventSummary, KeyCode};
    for (_, dev) in evdev::enumerate() {
        let wanted = dev
            .supported_keys()
            .map(|k| k.contains(KeyCode::KEY_VOLUMEUP) || k.contains(KeyCode::KEY_VOLUMEDOWN) || k.contains(KeyCode::KEY_POWER))
            .unwrap_or(false);
        if !wanted {
            continue;
        }
        let on_key = on_key.clone();
        let mut dev = dev;
        std::thread::spawn(move || loop {
            let Ok(events) = dev.fetch_events() else { return };
            for ev in events {
                // Solo la pressione (1) e la ripetizione (2), non il rilascio
                if let EventSummary::Key(_, code, v) = ev.destructure() {
                    if v == 0 {
                        continue;
                    }
                    match code {
                        KeyCode::KEY_VOLUMEUP => on_key(Key::VolumeUp),
                        KeyCode::KEY_VOLUMEDOWN => on_key(Key::VolumeDown),
                        KeyCode::KEY_POWER if v == 1 => on_key(Key::Power),
                        _ => {}
                    }
                }
            }
        });
    }
}

#[cfg(not(feature = "native"))]
pub fn listen<F: Fn(Key) + Send + Sync + Clone + 'static>(_on_key: F) {}
