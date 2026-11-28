slint::include_modules!();

pub mod apk;
pub mod calc;
pub mod calendar;
pub mod controller;
pub mod ctl_comm;
pub mod ctl_files;
pub mod ctl_settings;
pub mod ctl_web;
#[cfg(feature = "native")]
pub mod evdev_lite;
pub mod feedback;
pub mod files;
pub mod keys;
pub mod media;
pub mod modem;
pub mod player;
pub mod store;
pub mod style;
pub mod ctl_style;
pub mod ctl_clock;
pub mod system;
pub mod update;
pub mod web;
pub mod demo;
#[cfg(target_arch = "wasm32")]
pub mod vfs;
#[cfg(target_arch = "wasm32")]
mod vfs_data;

/// Demo nel browser: stessa shell, dati finti, nessun accesso al sistema
#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn web_main() {
    use slint::ComponentHandle;
    let ui = AppWindow::new().expect("finestra");
    // La finestra occupa tutto lo schermo del telefono, in punti CSS
    // Tela grande quanto lo schermo in pixel reali: nitida sui display ad alta densità
    if let Some(w) = web_sys::window() {
        let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(360.0);
        let height = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(640.0);
        let dpr = w.device_pixel_ratio().max(1.0);
        ui.window().set_size(slint::PhysicalSize::new((width * dpr) as u32, (height * dpr) as u32));
    }
    let now = chrono::Local::now().naive_local();
    let st = controller::wire(&ui, demo::store(), now, false);
    demo::apply(&ui, &st);
    ctl_files::index(&ui, &st, false);
    std::mem::forget(controller::start_timers(&ui, &st));
    ui.run().expect("event loop");
}
