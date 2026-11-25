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
            n(1, "Groceries", "Milk, bread, cat food", true, 10),
            n(2, "OS ideas", "Edge gestures, weather widget on home", true, 9),
            n(3, "To do", "Boot shim, touch test, Waydroid", false, 8),
            n(4, "Wi-Fi code", "Ask at home", false, 7),
        ],
        events: vec![
            e("2026-11-03", "09:30", "Dentist"),
            e("2026-11-25", "18:00", "Groceries"),
            e("2026-11-25", "23:00", "Cheip OS design review"),
            e("2026-11-27", "10:00", "Ship build 0.3"),
        ],
        contacts: vec![
            c("Andrea", "+39 340 112 2334"),
            c("Giulia", "+39 347 556 7788"),
            c("Marco", "+39 333 123 4567"),
            c("Rossi Garage", "0331 123456"),
        ],
        calls: vec![
            call("+393331234567", ts(25, 21, 12), "out", 240),
            call("+393475567788", ts(25, 18, 40), "missed", 0),
            call("0331123456", ts(24, 10, 5), "in", 95),
            call("+393401122334", ts(22, 16, 30), "out", 600),
        ],
        messages: vec![
            m("+393331234567", "Trying Cheip OS on the Lumia tomorrow?", ts(25, 22, 30), false, true),
            m("+393331234567", "Yes, 0.3 has phone, browser and a media player", ts(25, 22, 33), true, true),
            m("+393331234567", "Great. I'll bring the 640 so we can test that too", ts(25, 22, 47), false, false),
            m("+393475567788", "Called you, call me back when you can", ts(25, 18, 41), false, false),
            m("3912", "Your remaining credit is 8.40 euro", ts(23, 9, 0), false, true),
        ],
        bookmarks: vec![
            Bookmark { title: "Cheip Studio".into(), url: "https://cheipstudio.org".into() },
            Bookmark { title: "postmarketOS wiki".into(), url: "https://wiki.postmarketos.org".into() },
            Bookmark { title: "Weather Sesto Calende".into(), url: "https://html.duckduckgo.com/html/?q=weather%20sesto%20calende".into() },
        ],
    })
}

const PAGE: &str = r#"<html><head><title>Cheip OS 0.3 is here</title></head><body>
<nav><a href="/">Home</a></nav>
<h1>Cheip OS 0.3: media, files and an English UI</h1>
<p>The third release turns the Files app into a media hub: photo gallery, video player, music player with tags and covers, PDF and document viewers.</p>
<h2>A browser that fits in a few MB</h2>
<p>Instead of a full web engine, pages are reduced to native headings, text and links. On 1 GB of RAM that makes the difference.</p>
<ul><li>Tabs with per-tab history</li><li>Bookmarks</li><li>Built-in search</li></ul>
<p><a href="https://cheipstudio.org/cheip-os">Full release notes</a></p>
<blockquote><p>The real web engine arrives when the shell becomes a compositor.</p></blockquote>
</body></html>"#;

fn sh(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Home finta con media veri: immagini, video, musica con tag, PDF, DOCX
fn demo_home(home: &std::path::Path) {
    use std::io::Write;
    let _ = std::fs::remove_dir_all(home);
    for d in ["Documents", "Downloads", "Pictures", "Movies", "Music"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    // Immagini astratte generate: niente foto vere
    let palette = [(198, 255, 61), (255, 122, 26), (61, 169, 255), (255, 79, 163), (177, 140, 255), (40, 40, 40), (255, 210, 60), (60, 220, 170), (230, 230, 230)];
    for (i, (r, g, b)) in palette.iter().enumerate() {
        let img = image::RgbImage::from_fn(600, 600, |x, y| {
            let t = ((x as f32 / 600.0) + (y as f32 / 600.0)) / 2.0;
            let ring = (((x as f32 - 300.0).powi(2) + (y as f32 - 300.0).powi(2)).sqrt() / (40.0 + i as f32 * 8.0)) as u32 % 2;
            let k = if ring == 0 { 1.0 } else { 0.55 };
            image::Rgb([(*r as f32 * k * (1.0 - t * 0.5)) as u8, (*g as f32 * k * (1.0 - t * 0.5)) as u8, (*b as f32 * k * (1.0 - t * 0.5)) as u8])
        });
        img.save(home.join(format!("Pictures/IMG_2026112{}_{:02}.jpg", i % 5, i))).unwrap();
    }
    let movies = home.join("Movies");
    for (name, src, d) in [("Sunset test.mp4", "mandelbrot=size=640x360:rate=15", "4"), ("Bars.mp4", "smptehdbars=size=640x360:rate=15", "75"), ("Life.mp4", "life=size=640x360:rate=15:mold=10:ratio=0.2", "12"), ("Pattern.mp4", "testsrc2=size=640x360:rate=15", "203")] {
        let out = movies.join(name);
        sh("ffmpeg", &["-y", "-f", "lavfi", "-i", src, "-t", d, "-c:v", "mpeg4", "-q:v", "8", out.to_str().unwrap()]);
    }
    let cover = home.join("cover.png");
    image::RgbImage::from_fn(400, 400, |x, y| {
        let v = ((x / 50 + y / 50) % 2) as u8;
        if v == 0 { image::Rgb([198, 255, 61]) } else { image::Rgb([20, 20, 20]) }
    })
    .save(&cover)
    .unwrap();
    let music = home.join("Music");
    for (i, (title, artist, secs)) in [("Night Drive", "Second Dream", "214"), ("Lake Maggiore", "Second Dream", "187"), ("Overtake", "Cheip", "163"), ("Garage Loop", "Cheip", "241")].iter().enumerate() {
        let out = music.join(format!("{:02} {title}.flac", i + 1));
        let mut args = vec!["-y", "-f", "lavfi", "-i", "anullsrc=r=22050:cl=mono"];
        if i < 2 {
            args.extend(["-i", cover.to_str().unwrap(), "-map", "0:a", "-map", "1:v", "-disposition:v", "attached_pic"]);
        }
        let t = format!("title={title}");
        let a = format!("artist={artist}");
        args.extend(["-t", secs, "-metadata", &t, "-metadata", &a, out.to_str().unwrap()]);
        sh("ffmpeg", &args);
    }
    let _ = std::fs::remove_file(&cover);
    // PDF con testo, creato con MuPDF
    let page = home.join("page.txt");
    std::fs::write(&page, "%%MediaBox 0 0 360 500\n%%Font F1 Helvetica\nBT /F1 26 Tf 30 430 Td (Cheip OS) Tj ET\nBT /F1 13 Tf 30 400 Td (Installation guide for MSM8x26 Lumias) Tj ET\nBT /F1 11 Tf 30 360 Td (1. Back up the eMMC in mass storage mode) Tj ET\nBT /F1 11 Tf 30 340 Td (2. Boot menu, BootShim, lk2nd) Tj ET\nBT /F1 11 Tf 30 320 Td (3. Write the Cheip OS image) Tj ET\n").unwrap();
    sh("mutool", &["create", "-o", home.join("Documents/Cheip OS install guide.pdf").to_str().unwrap(), page.to_str().unwrap(), page.to_str().unwrap()]);
    sh("mutool", &["create", "-o", home.join("Downloads/lk2nd-notes.pdf").to_str().unwrap(), page.to_str().unwrap()]);
    let _ = std::fs::remove_file(&page);
    let docx = home.join("Documents/Client brief.docx");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&docx).unwrap());
    z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
    z.write_all(b"<w:document><w:body><w:p><w:r><w:t>Client brief</w:t></w:r></w:p><w:p><w:r><w:t>Build a launcher with round minimal icons, ChromeOS style.</w:t></w:r></w:p><w:p><w:r><w:t>Files app with gallery, video and music player.</w:t></w:r></w:p></w:body></w:document>").unwrap();
    z.finish().unwrap();
    std::fs::write(home.join("Documents/groceries.txt"), "Milk\nBread\nCat food\n").unwrap();
    std::fs::write(home.join("Downloads/firmware.zip"), vec![0u8; 3_400_000]).unwrap();
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "screens".into());
    std::fs::create_dir_all(&out).unwrap();
    for e in std::fs::read_dir(&out).unwrap().flatten() {
        if e.path().extension().map(|x| x == "png").unwrap_or(false) {
            let _ = std::fs::remove_file(e.path());
        }
    }

    let home = std::env::temp_dir().join("cheip-demo-home");
    demo_home(&home);
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CACHE_HOME", home.join(".cache"));
    std::env::set_var("CHEIP_AUDIO_SINK", "fakesink");

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
        s.notify("Marco", "Great. I'll bring the 640 so we can test that too", false);
        s.notify("System", "Cheip OS 0.3.1 ready to install", false);
        controller::push_all(&ui, &s);
    }
    let s = ui.global::<Sys>();
    s.set_battery(78);
    s.set_mem_used(588);
    s.set_mem_total(1024);
    s.set_mem_android(212);
    s.set_android_running(true);
    s.set_android_state("Running, suspends after 2 minutes with no apps".into());
    s.set_model("Nokia Lumia 735".into());
    s.set_kernel("6.12.9-msm8x26".into());
    s.set_update_available(true);
    s.set_update_version("0.3.1".into());
    s.set_update_state("Faster browser, SMS fixes".into());
    s.set_volume(0.6);
    let set = ui.global::<Settings>();
    set.set_modem_operator("TIM".into());
    set.set_modem_signal("67%".into());
    set.set_modem_state("registered".into());
    set.set_storage_used("2.9 GB".into());
    set.set_storage_total("5.6 GB".into());
    set.set_storage_ratio(0.52);
    cheip_shell::ctl_settings::demo_networks(
        &ui,
        vec![
            WifiNet { ssid: "Casa Casale".into(), signal: 82, secure: true, active: true },
            WifiNet { ssid: "FASTWEB-7A21".into(), signal: 61, secure: true, active: false },
            WifiNet { ssid: "Sesto Library".into(), signal: 44, secure: false, active: false },
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
    cheip_shell::ctl_files::index(&ui, &st, false);
    open("files");
    ctx.advance(700);
    ctx.shot("files");
    let files = ui.global::<Files>();
    let meta = |cat: &str| {
        files.invoke_open_category(cat.into());
        let list = st.borrow().fx.list.clone();
        let res = cheip_shell::ctl_files::build_meta(&list);
        let mut s = st.borrow_mut();
        cheip_shell::ctl_files::apply_meta(&ui, &mut s, res);
    };
    meta("images");
    ctx.advance(300);
    ctx.shot("gallery");
    files.invoke_open(3);
    ctx.advance(300);
    ctx.shot("photo");
    files.invoke_close_viewer();
    meta("videos");
    ctx.advance(300);
    ctx.shot("videos");
    files.invoke_open(0);
    ctx.advance(1200);
    files.set_pos_label("1:12".into());
    files.set_dur_label("3:23".into());
    files.set_progress(0.35);
    ctx.shot("video-player");
    files.invoke_close_viewer();
    meta("audio");
    files.invoke_open(0);
    ctx.advance(1500);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    ctx.advance(600);
    files.set_pos_label("1:12".into());
    files.set_dur_label("3:07".into());
    files.set_progress(0.38);
    ctx.shot("music-player");
    files.set_audio_full(false);
    ctx.advance(300);
    ctx.shot("music");
    files.invoke_stop_audio();
    meta("documents");
    ctx.advance(300);
    ctx.shot("documents");
    let pdf_idx = st.borrow().fx.list.iter().position(|e| e.name.ends_with(".pdf")).unwrap_or(0) as i32;
    files.invoke_open(pdf_idx);
    ctx.advance(300);
    ctx.shot("pdf");
    files.invoke_close_viewer();
    let doc_idx = st.borrow().fx.list.iter().position(|e| e.name.ends_with(".docx")).unwrap_or(0) as i32;
    files.invoke_open(doc_idx);
    ctx.advance(300);
    ctx.shot("docx");
    files.invoke_close_viewer();
    files.invoke_open_home();

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
    let cols = 7u32;
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
