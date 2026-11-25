//! Renderizza le schermate della shell in PNG 720x1280 (risoluzione del Lumia 735)
//! con il renderer software, senza display. Uso: cheip-screens <cartella>

use cheip_shell::controller;
use cheip_shell::store::{Data, Event, Note, Store};
use cheip_shell::*;
use chrono::NaiveDate;
use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const W: u32 = 720;
const H: u32 = 1280;

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
    clock: Rc<Cell<Duration>>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.clock.get()
    }
}

struct Ctx {
    window: Rc<MinimalSoftwareWindow>,
    clock: Rc<Cell<Duration>>,
}

impl Ctx {
    fn advance(&self, ms: u64) {
        // A piccoli passi, così le animazioni vedono il tempo scorrere
        let steps = (ms / 16).max(1);
        for _ in 0..steps {
            self.clock.set(self.clock.get() + Duration::from_millis(ms / steps));
            slint::platform::update_timers_and_animations();
        }
    }

    fn shot(&self, path: &str) -> image::RgbImage {
        self.window.request_redraw();
        let mut buf = vec![PremultipliedRgbaColor::default(); (W * H) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut buf, W as usize);
        });
        let mut img = image::RgbImage::new(W, H);
        for (i, p) in buf.iter().enumerate() {
            img.put_pixel(i as u32 % W, i as u32 / W, image::Rgb([p.red, p.green, p.blue]));
        }
        img.save(path).expect("salvataggio PNG");
        println!("{path}");
        img
    }
}

fn demo_store() -> Store {
    let n = |id, t: &str, b: &str, p, u| Note { id, title: t.into(), body: b.into(), pinned: p, updated: u };
    let e = |d: &str, t: &str, x: &str| Event { date: d.into(), time: t.into(), title: x.into() };
    Store::memory(Data {
        notes: vec![
            n(1, "Spesa", "Latte, pane, croccantini", true, 10),
            n(2, "Idee OS", "Gesture dal bordo, widget meteo in home", true, 9),
            n(3, "Da fare", "Shim di boot, test touch, Waydroid", false, 8),
            n(4, "Codice wifi", "Chiedere a casa", false, 7),
            n(5, "Film", "Lista per il weekend", false, 6),
            n(6, "Auto", "Tagliando a dicembre", false, 5),
        ],
        events: vec![
            e("2026-11-03", "09:30", "Dentista"),
            e("2026-11-12", "14:00", "Call progetto"),
            e("2026-11-25", "18:00", "Spesa"),
            e("2026-11-25", "23:00", "Review design Cheip OS"),
            e("2026-11-27", "10:00", "Consegna build 0.1"),
        ],
    })
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "screens".into());
    std::fs::create_dir_all(&out).unwrap();

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let clock = Rc::new(Cell::new(Duration::ZERO));
    slint::platform::set_platform(Box::new(Headless { window: window.clone(), clock: clock.clone() })).unwrap();
    let ctx = Ctx { window: window.clone(), clock };

    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    window.set_size(slint::PhysicalSize::new(W, H));

    let now = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap().and_hms_opt(22, 51, 0).unwrap();
    let _st = controller::wire(&ui, demo_store(), now, false);

    // Dati demo come su un 735 reale
    let s = ui.global::<Sys>();
    s.set_battery(78);
    s.set_mem_used(612);
    s.set_mem_total(1024);
    s.set_mem_android(264);
    s.set_android_running(true);
    s.set_android_state("Attivo · si sospende dopo 2 min senza app".into());
    s.set_model("Nokia Lumia 735".into());
    s.set_kernel("6.12.9-msm8x26".into());
    s.set_update_available(true);
    s.set_update_version("0.1.1".into());
    s.set_update_state("Animazioni più rapide, fix calendario".into());
    s.set_notifications(ModelRc::new(VecModel::from(vec![
        NotifItem { app: "Telegram".into(), time: "22:47".into(), text: "Nuovo messaggio nel gruppo".into(), android: true },
        NotifItem { app: "Sistema".into(), time: "22:30".into(), text: "Aggiornamento Cheip OS 0.1.1 pronto".into(), android: false },
    ])));
    let shell = ui.global::<Shell>();
    let mut apps: Vec<AppEntry> = shell.get_apps().iter().collect();
    apps.push(AppEntry { id: "org.telegram.messenger".into(), name: "Telegram".into(), android: true });
    apps.push(AppEntry { id: "com.whatsapp".into(), name: "WhatsApp".into(), android: true });
    shell.set_apps(ModelRc::new(VecModel::from(apps)));

    let mut frames = Vec::new();
    ctx.advance(600);
    frames.push(ctx.shot(&format!("{out}/01-blocco.png")));

    shell.invoke_unlock();
    ctx.advance(800);
    frames.push(ctx.shot(&format!("{out}/02-home.png")));

    // Apertura Calendario: un fotogramma a metà animazione
    shell.invoke_open_app("calendar".into(), 196.0);
    ctx.advance(70);
    frames.push(ctx.shot(&format!("{out}/03-apertura-app.png")));
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/04-calendario.png")));

    shell.invoke_close_app();
    ctx.advance(700);
    shell.invoke_open_app("notes".into(), 240.0);
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/05-note.png")));

    shell.invoke_close_app();
    ctx.advance(700);
    shell.invoke_open_app("camera".into(), 284.0);
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/06-fotocamera.png")));

    shell.invoke_close_app();
    ctx.advance(700);
    shell.invoke_open_app("settings".into(), 328.0);
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/07-impostazioni.png")));

    shell.invoke_close_app();
    ctx.advance(700);
    shell.set_controls_open(true);
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/08-controlli.png")));

    shell.set_controls_open(false);
    ctx.advance(700);
    shell.set_screen(ScreenId::Switcher);
    ctx.advance(700);
    frames.push(ctx.shot(&format!("{out}/09-app-aperte.png")));

    // Tavola riassuntiva a metà risoluzione
    let (tw, th) = (W / 2, H / 2);
    let cols = 5u32;
    let rows = (frames.len() as u32 + cols - 1) / cols;
    let gap = 24u32;
    let mut sheet = image::RgbImage::from_pixel(cols * tw + (cols + 1) * gap, rows * th + (rows + 1) * gap, image::Rgb([24, 24, 24]));
    for (i, f) in frames.iter().enumerate() {
        let small = image::imageops::resize(f, tw, th, image::imageops::FilterType::Triangle);
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        image::imageops::overlay(&mut sheet, &small, (gap + c * (tw + gap)) as i64, (gap + r * (th + gap)) as i64);
    }
    sheet.save(format!("{out}/00-panoramica.png")).unwrap();
    println!("{out}/00-panoramica.png");
}
