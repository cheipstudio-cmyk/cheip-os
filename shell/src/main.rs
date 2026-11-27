use cheip_shell::controller;
use cheip_shell::store::Store;
use cheip_shell::AppWindow;
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    // Sul telefono il backend arriva da SLINT_BACKEND (linuxkms-femtovg o linuxkms-software),
    // impostato dal servizio di avvio in base alla presenza della GPU.
    let ui = AppWindow::new()?;
    // Software rendering (no GPU driver): instant transitions, flat shadows
    if std::env::var("SLINT_BACKEND").map(|b| b.contains("software")).unwrap_or(false) {
        ui.global::<cheip_shell::Theme>().set_lite(true);
    }
    let store = Store::open(Store::data_dir());
    let _state = controller::wire(&ui, store, chrono::Local::now().naive_local(), true);
    let _timers = controller::start_timers(&ui, &_state);
    // Tasti volume e accensione dal kernel
    cheip_shell::keys::listen(|k| {
        let _ = slint::invoke_from_event_loop(move || controller::on_key(k));
    });
    ui.run()
}
