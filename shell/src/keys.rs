//! Tasti fisici del telefono (volume e accensione) letti da evdev,
//! in un thread: i Lumia li espongono come gpio-keys o pm8941-pwrkey.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    VolumeUp,
    VolumeDown,
    Power,
    PowerLong,
}

#[cfg(feature = "native")]
pub fn listen<F: Fn(Key) + Send + Sync + Clone + 'static>(on_key: F) {
    use crate::evdev_lite::{enumerate, EV_KEY, KEY_POWER, KEY_VOLUMEDOWN, KEY_VOLUMEUP};
    for mut dev in enumerate() {
        if !(dev.has_key(KEY_VOLUMEUP) || dev.has_key(KEY_VOLUMEDOWN) || dev.has_key(KEY_POWER)) {
            continue;
        }
        let on_key = on_key.clone();
        // Power: short press on release, long press after 600 ms held
        let held = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        std::thread::spawn(move || loop {
            let Ok(events) = dev.read_events() else { return };
            for ev in events {
                if ev.kind != EV_KEY {
                    continue;
                }
                if ev.code == KEY_POWER {
                    use std::sync::atomic::Ordering::SeqCst;
                    if ev.value == 1 {
                        let id = held.fetch_add(1, SeqCst) + 1;
                        let (held, on_key) = (held.clone(), on_key.clone());
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(600));
                            // Still the same press and not released: long press
                            if held.compare_exchange(id, id | 0x8000_0000, SeqCst, SeqCst).is_ok() {
                                on_key(Key::PowerLong);
                            }
                        });
                    } else if ev.value == 0 {
                        let v = held.load(SeqCst);
                        if v & 0x8000_0000 == 0 {
                            // Released before 600 ms: short press. Bump so the timer sees a new id
                            held.store((v + 1) & 0x7fff_ffff, SeqCst);
                            on_key(Key::Power);
                        } else {
                            held.store((v + 1) & 0x7fff_ffff, SeqCst);
                        }
                    }
                    continue;
                }
                // Solo la pressione (1) e la ripetizione (2), non il rilascio
                if ev.value == 0 {
                    continue;
                }
                match ev.code {
                    KEY_VOLUMEUP => on_key(Key::VolumeUp),
                    KEY_VOLUMEDOWN => on_key(Key::VolumeDown),
                    _ => {}
                }
            }
        });
    }
}

#[cfg(not(feature = "native"))]
pub fn listen<F: Fn(Key) + Send + Sync + Clone + 'static>(_on_key: F) {}
