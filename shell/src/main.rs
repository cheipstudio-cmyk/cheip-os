use cheip_shell::controller;
use cheip_shell::store::Store;
use cheip_shell::AppWindow;
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    // Sul telefono il backend arriva da SLINT_BACKEND (linuxkms-femtovg o linuxkms-software),
    // impostato dal servizio di avvio in base alla presenza della GPU.
    let ui = AppWindow::new()?;
    let store = Store::open(Store::data_dir());
    let _state = controller::wire(&ui, store, chrono::Local::now().naive_local(), true);
    let _timers = controller::start_timers(&ui);
    ui.run()
}
