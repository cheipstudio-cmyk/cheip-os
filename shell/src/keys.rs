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
    use crate::evdev_lite::{enumerate, EV_KEY, KEY_POWER, KEY_VOLUMEDOWN, KEY_VOLUMEUP};
    for mut dev in enumerate() {
        if !(dev.has_key(KEY_VOLUMEUP) || dev.has_key(KEY_VOLUMEDOWN) || dev.has_key(KEY_POWER)) {
            continue;
        }
        let on_key = on_key.clone();
        std::thread::spawn(move || loop {
            let Ok(events) = dev.read_events() else { return };
            for ev in events {
                // Solo la pressione (1) e la ripetizione (2), non il rilascio
                if ev.kind != EV_KEY || ev.value == 0 {
                    continue;
                }
                match ev.code {
                    KEY_VOLUMEUP => on_key(Key::VolumeUp),
                    KEY_VOLUMEDOWN => on_key(Key::VolumeDown),
                    KEY_POWER if ev.value == 1 => on_key(Key::Power),
                    _ => {}
                }
            }
        });
    }
}

#[cfg(not(feature = "native"))]
pub fn listen<F: Fn(Key) + Send + Sync + Clone + 'static>(_on_key: F) {}
