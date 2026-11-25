//! Renderizza le schermate della shell in PNG 720x1280 (risoluzione del Lumia 735)
//! con il renderer software, senza display. Uso: cheip-screens <cartella>

use cheip_shell::controller;
use cheip_shell::store::{Bookmark, CallLog, Contact, Data, Event, Message, Note, Store};
use cheip_shell::system::WifiNet;
use cheip_shell::*;
use chrono::{Local, NaiveDate, TimeZone};
use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, ModelRc, VecModel};
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
    out: String,
    frames: Vec<image::RgbImage>,
}

impl Ctx {
    fn advance(&self, ms: u64) {
        let steps = (ms / 16).max(1);
        for _ in 0..steps {
            self.clock.set(self.clock.get() + Duration::from_millis(ms / steps));
            slint::platform::update_timers_and_animations();
        }
    }

    fn shot(&mut self, name: &str) {
        self.window.request_redraw();
        let mut buf = vec![PremultipliedRgbaColor::default(); (W * H) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut buf, W as usize);
        });
        let mut img = image::RgbImage::new(W, H);
        for (i, p) in buf.iter().enumerate() {
            img.put_pixel(i as u32 % W, i as u32 / W, image::Rgb([p.red, p.green, p.blue]));
        }
        let path = format!("{}/{:02}-{name}.png", self.out, self.frames.len() + 1);
        img.save(&path).expect("salvataggio PNG");
        println!("{path}");
        self.frames.push(img);
    }
}

fn ts(d: u32, h: u32, m: u32) -> i64 {
    Local.with_ymd_and_hms(2026, 11, d, h, m, 0).single().map(|t| t.timestamp()).unwrap_or(0)
}

fn demo_store() -> Store {
    let n = |id, t: &str, b: &str, p, u| Note { id, title: t.into(), body: b.into(), pinned: p, updated: u };
    let e = |d: &str, t: &str, x: &str| Event { date: d.into(), time: t.into(), title: x.into() };
    let c = |n: &str, num: &str| Contact { name: n.into(), number: num.into() };
    let call = |num: &str, t, dir: &str, secs| CallLog { number: num.into(), ts: t, dir: dir.into(), secs };
    let m = |num: &str, t: &str, at, out, read| Message { number: num.into(), text: t.into(), ts: at, outgoing: out, read };
    Store::memory(Data {
        notes: vec![
            n(1, "Spesa", "Latte, pane, croccantini", true, 10),
            n(2, "Idee OS", "Gesture dal bordo, widget meteo in home", true, 9),
            n(3, "Da fare", "Shim di boot, test touch, Waydroid", false, 8),
            n(4, "Codice wifi", "Chiedere a casa", false, 7),
        ],
        events: vec![
            e("2026-11-03", "09:30", "Dentista"),
            e("2026-11-25", "18:00", "Spesa"),
            e("2026-11-25", "23:00", "Review design Cheip OS"),
            e("2026-11-27", "10:00", "Consegna build 0.2"),
        ],
        contacts: vec![
            c("Andrea", "+39 340 112 2334"),
            c("Giulia", "+39 347 556 7788"),
            c("Marco", "+39 333 123 4567"),
            c("Officina Rossi", "0331 123456"),
        ],
        calls: vec![
            call("+393331234567", ts(25, 21, 12), "out", 240),
            call("+393475567788", ts(25, 18, 40), "missed", 0),
            call("0331123456", ts(24, 10, 5), "in", 95),
            call("+393401122334", ts(22, 16, 30), "out", 600),
        ],
        messages: vec![
            m("+393331234567", "Domani si prova Cheip OS sul Lumia?", ts(25, 22, 30), false, true),
            m("+393331234567", "Sì, la 0.2 ha anche telefono e browser", ts(25, 22, 33), true, true),
            m("+393331234567", "Grande. Porto il 640 così testiamo anche quello", ts(25, 22, 47), false, false),
            m("+393475567788", "Ti ho chiamato, richiamami quando puoi", ts(25, 18, 41), false, false),
            m("3912", "Il tuo credito residuo è 8,40 euro", ts(23, 9, 0), false, true),
        ],
        bookmarks: vec![
            Bookmark { title: "Cheip Studio".into(), url: "https://cheipstudio.org".into() },
            Bookmark { title: "postmarketOS wiki".into(), url: "https://wiki.postmarketos.org".into() },
            Bookmark { title: "Meteo Sesto Calende".into(), url: "https://html.duckduckgo.com/html/?q=meteo%20sesto%20calende".into() },
        ],
    })
}

const PAGE: &str = r#"<html><head><title>Cheip OS 0.2 è qui</title></head><body>
<nav><a href="/">Home</a></nav>
<h1>Cheip OS 0.2: telefono, messaggi e browser</h1>
<p>La seconda versione porta le app che mancavano per usare il Lumia ogni giorno: Telefono con rubrica e registro, Messaggi, File e un browser leggero.</p>
<h2>Un browser da pochi MB</h2>
<p>Invece di un motore web completo, le pagine vengono ridotte a titoli, testo e link nativi. Su 1 GB di RAM fa la differenza.</p>
<ul><li>Schede e cronologia per scheda</li><li>Preferiti</li><li>Ricerca integrata</li></ul>
<p><a href="https://cheipstudio.org/cheip-os">Note di rilascio complete</a></p>
<blockquote><p>Il motore web vero arriva quando la shell diventa compositor.</p></blockquote>
</body></html>"#;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "screens".into());
    std::fs::create_dir_all(&out).unwrap();
    for e in std::fs::read_dir(&out).unwrap().flatten() {
        if e.path().extension().map(|x| x == "png").unwrap_or(false) {
            let _ = std::fs::remove_file(e.path());
        }
    }

    // Home finta per l'app File
    let home = std::env::temp_dir().join("cheip-demo-home");
    let _ = std::fs::remove_dir_all(&home);
    for d in ["Documenti", "Download", "Foto", "Musica"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    std::fs::write(home.join("lista-spesa.txt"), "Latte\nPane\n").unwrap();
    std::fs::write(home.join("note-boot.md"), "# Boot\nlk2nd poi kernel mainline").unwrap();
    std::fs::write(home.join("sfondo.png"), vec![0u8; 734_003]).unwrap();
    std::env::set_var("HOME", &home);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let clock = Rc::new(Cell::new(Duration::ZERO));
    slint::platform::set_platform(Box::new(Headless { window: window.clone(), clock: clock.clone() })).unwrap();
    let mut ctx = Ctx { window: window.clone(), clock, out: out.clone(), frames: vec![] };

    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    window.set_size(slint::PhysicalSize::new(W, H));

    let now = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap().and_hms_opt(22, 51, 0).unwrap();
    let st = controller::wire(&ui, demo_store(), now, false);

    // Dati demo come su un 735 reale
    {
        let mut s = st.borrow_mut();
        s.modem_ok = true;
        s.cwd = home.clone();
        s.entries = cheip_shell::files::list(&home, false);
        s.notify("Marco", "Grande. Porto il 640 così testiamo anche quello", false);
        s.notify("Sistema", "Cheip OS 0.2.1 pronto da installare", false);
        controller::push_all(&ui, &s);
    }
    let s = ui.global::<Sys>();
    s.set_battery(78);
    s.set_mem_used(588);
    s.set_mem_total(1024);
    s.set_mem_android(212);
    s.set_android_running(true);
    s.set_android_state("Attivo, si sospende dopo 2 minuti senza app".into());
    s.set_model("Nokia Lumia 735".into());
    s.set_kernel("6.12.9-msm8x26".into());
    s.set_update_available(true);
    s.set_update_version("0.2.1".into());
    s.set_update_state("Browser più veloce, fix SMS".into());
    s.set_volume(0.6);
    let set = ui.global::<Settings>();
    set.set_modem_operator("TIM".into());
    set.set_modem_signal("67%".into());
    set.set_modem_state("registered".into());
    set.set_storage_used("2,9 GB".into());
    set.set_storage_total("5,6 GB".into());
    set.set_storage_ratio(0.52);
    ui.global::<Files>().set_storage("2,7 GB liberi su 5,6 GB".into());
    cheip_shell::ctl_settings::demo_networks(
        &ui,
        vec![
            WifiNet { ssid: "Casa Casale".into(), signal: 82, secure: true, active: true },
            WifiNet { ssid: "FASTWEB-7A21".into(), signal: 61, secure: true, active: false },
            WifiNet { ssid: "Biblioteca Sesto".into(), signal: 44, secure: false, active: false },
            WifiNet { ssid: "TIM-90312".into(), signal: 30, secure: true, active: false },
        ],
    );

    // App Android finte nella griglia
    let shell = ui.global::<Shell>();
    let mut apps: Vec<AppEntry> = slint::Model::iter(&shell.get_apps()).collect();
    for (id, name) in [("org.telegram.messenger", "Telegram"), ("com.whatsapp", "WhatsApp"), ("com.spotify.music", "Spotify")] {
        apps.push(AppEntry { id: id.into(), name: name.into(), initial: name[..1].into(), android: true, ..Default::default() });
    }
    shell.set_apps(ModelRc::new(VecModel::from(apps)));

    let open = |id: &str| shell.invoke_open_app(id.into(), 152.0, 300.0, 56.0);

    ctx.advance(600);
    ctx.shot("blocco");

    shell.invoke_unlock();
    ctx.advance(800);
    ctx.shot("home");

    shell.set_searching(true);
    ctx.advance(300);
    ctx.shot("ricerca");
    shell.set_searching(false);

    shell.invoke_open_app("phone".into(), 152.0, 530.0, 48.0);
    ctx.advance(80);
    ctx.shot("apertura-app");
    ctx.advance(700);
    ui.global::<Phone>().set_dial("3331234".into());
    ctx.advance(400);
    ctx.shot("telefono");
    ui.global::<Phone>().set_dial("".into());
    ui.global::<Phone>().set_tab(1);
    ctx.advance(200);
    ctx.shot("telefono-recenti");
    ui.global::<Phone>().invoke_call("+393331234567".into());
    ctx.advance(700);
    ctx.shot("chiamata");
    ui.global::<Phone>().invoke_hangup();
    ctx.advance(700);

    shell.invoke_close_app();
    ctx.advance(600);
    open("messages");
    ctx.advance(700);
    ctx.shot("messaggi");
    ui.global::<Msg>().invoke_open_thread("+393331234567".into());
    ctx.advance(300);
    ctx.shot("conversazione");
    ui.global::<Msg>().invoke_close_thread();

    shell.invoke_close_app();
    ctx.advance(600);
    open("browser");
    ctx.advance(700);
    ctx.shot("browser-inizio");
    {
        let page = cheip_shell::web::parse(PAGE, "https://cheipstudio.org/blog/cheip-os-0-2");
        let mut s = st.borrow_mut();
        cheip_shell::ctl_web::show_page(&ui, &mut s, page);
    }
    ctx.advance(300);
    ctx.shot("browser-pagina");

    shell.invoke_close_app();
    ctx.advance(600);
    open("files");
    ctx.advance(700);
    ctx.shot("file");

    shell.invoke_close_app();
    ctx.advance(600);
    open("settings");
    ctx.advance(700);
    ctx.shot("impostazioni");
    set.set_page("wifi".into());
    ctx.advance(300);
    ctx.shot("wifi");
    set.set_page("".into());

    shell.invoke_close_app();
    ctx.advance(600);
    open("calendar");
    ctx.advance(700);
    ctx.shot("calendario");

    shell.invoke_close_app();
    ctx.advance(600);
    shell.set_controls_open(true);
    ctx.advance(700);
    ctx.shot("controlli");
    shell.set_controls_open(false);
    ctx.advance(700);
    shell.set_screen(ScreenId::Switcher);
    ctx.advance(700);
    ctx.shot("app-aperte");

    // Tavola riassuntiva
    let (tw, th) = (W / 2, H / 2);
    let cols = 6u32;
    let rows = (ctx.frames.len() as u32).div_ceil(cols);
    let gap = 24u32;
    let mut sheet = image::RgbImage::from_pixel(cols * tw + (cols + 1) * gap, rows * th + (rows + 1) * gap, image::Rgb([24, 24, 24]));
    for (i, f) in ctx.frames.iter().enumerate() {
        let small = image::imageops::resize(f, tw, th, image::imageops::FilterType::Triangle);
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        image::imageops::overlay(&mut sheet, &small, (gap + c * (tw + gap)) as i64, (gap + r * (th + gap)) as i64);
    }
    sheet.save(format!("{out}/00-panoramica.png")).unwrap();
    println!("{out}/00-panoramica.png");
}
