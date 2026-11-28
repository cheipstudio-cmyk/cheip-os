//! Colori dinamici Material 3 (spec 2025, quella di Material 3 Expressive)
//! calcolati con material-color-utilities a partire dallo sfondo o da un colore base.

use crate::{AppWindow, Style, Swatch, Theme};
use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Platform, SpecVersion};
use material_colors::hct::Hct;
use material_colors::quantize::{Quantizer, QuantizerCelebi};
use material_colors::scheme::variant::{SchemeExpressive, SchemeMonochrome, SchemeNeutral, SchemeTonalSpot, SchemeVibrant};
use material_colors::score::Score;
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;

/// Colori base offerti oltre a quelli presi dallo sfondo
pub const BASIC: [(&str, u32); 6] = [
    ("Blue", 0x4285f4),
    ("Green", 0x2e9e5b),
    ("Purple", 0x8e6cef),
    ("Pink", 0xe8508a),
    ("Orange", 0xf08a24),
    ("Lime", 0xa8d930),
];

thread_local! {
    /// Colori estratti da ogni sfondo, calcolati una volta
    static SEEDS: RefCell<Vec<Vec<Rgb>>> = RefCell::new(Vec::new());
    /// Sfondi chiari nella parte alta (servono icone e testo scuri)
    static LIGHT: RefCell<Vec<bool>> = RefCell::new(Vec::new());
}

/// Luminosita media della parte alta dell'immagine
fn is_light(img: &slint::Image) -> bool {
    let Some(buf) = img.to_rgba8() else { return false };
    let (w, h) = (buf.width() as usize, buf.height() as usize);
    let px = buf.as_slice();
    let (mut sum, mut n) = (0f64, 0f64);
    for y in (0..h * 2 / 5).step_by(8) {
        for x in (0..w).step_by(8) {
            let p = px[y * w + x];
            sum += 0.2126 * p.r as f64 + 0.7152 * p.g as f64 + 0.0722 * p.b as f64;
            n += 1.0;
        }
    }
    n > 0.0 && sum / n > 150.0
}

pub fn wallpaper_light(ui: &AppWindow, wp: i32) -> bool {
    if let Some((_, l)) = BUILTIN.get(wp as usize) {
        return *l;
    }
    LIGHT.with(|l| {
        if l.borrow().is_empty() {
            let imgs = ui.global::<Style>().get_wallpapers();
            *l.borrow_mut() = (0..imgs.row_count()).map(|i| is_light(&imgs.row_data(i).unwrap())).collect();
        }
        l.borrow().get(wp as usize).copied().unwrap_or(false)
    })
}

fn rgb(c: u32) -> Rgb {
    Rgb::new(((c >> 16) & 255) as u8, ((c >> 8) & 255) as u8, (c & 255) as u8)
}

fn col(c: Rgb) -> Color {
    Color::from_rgb_u8(c.red, c.green, c.blue)
}

/// Colori dominanti di un'immagine secondo l'algoritmo dei Pixel
fn seeds_of(img: &slint::Image) -> Vec<Rgb> {
    let Some(buf) = img.to_rgba8() else { return vec![rgb(0x4285f4)] };
    let (w, h) = (buf.width() as usize, buf.height() as usize);
    let px = buf.as_slice();
    let step = ((w * h) / 12000).max(1);
    let mut pixels = Vec::with_capacity(w * h / step + 1);
    let mut i = 0;
    while i < w * h {
        let p = px[i];
        pixels.push(Rgb::new(p.r, p.g, p.b));
        i += step;
    }
    let q = QuantizerCelebi::quantize(&pixels, 128);
    let mut v = Score::score(&q.color_to_count, Some(3), Some(rgb(0x4285f4)), None);
    v.truncate(3);
    v
}

fn ensure_seeds(ui: &AppWindow) {
    SEEDS.with(|s| {
        if !s.borrow().is_empty() {
            return;
        }
        let imgs = ui.global::<Style>().get_wallpapers();
        let v: Vec<Vec<Rgb>> = (0..imgs.row_count()).map(|i| seeds_of(&imgs.row_data(i).unwrap())).collect();
        *s.borrow_mut() = v;
    });
}

/// Colori e luminosita degli sfondi inclusi, calcolati in anticipo con lo
/// stesso algoritmo (test print_wallpaper_seeds): niente lavoro all'avvio.
const BUILTIN: [(&[u32], bool); 5] = [
    (&[0xd0557c, 0x7c2e60], true),
    (&[0x12695e, 0x0e3036, 0x07141c], false),
    (&[0x8c2f1f, 0xfcc78e, 0xfff9df], true),
    (&[0x28204e, 0x0e5d87, 0x8b4374], false),
    (&[0x696d78, 0x60656b, 0x16161b], false),
];

fn wallpaper_seeds(ui: &AppWindow, wp: i32) -> Vec<Rgb> {
    if let Some((s, _)) = BUILTIN.get(wp as usize) {
        return s.iter().map(|c| rgb(*c)).collect();
    }
    ensure_seeds(ui);
    SEEDS.with(|s| s.borrow().get(wp as usize).cloned().unwrap_or_else(|| vec![rgb(0x4285f4)]))
}

/// Colore sorgente scelto: i primi sono dallo sfondo, poi i colori base
pub fn source(ui: &AppWindow, wp: i32, palette: i32) -> Rgb {
    let ws = wallpaper_seeds(ui, wp);
    let p = palette.max(0) as usize;
    if p < ws.len() {
        ws[p]
    } else {
        BASIC.get(p - ws.len()).map(|(_, c)| rgb(*c)).unwrap_or(ws[0])
    }
}

pub fn scheme(src: Rgb, kind: i32, dark: bool) -> DynamicScheme {
    let hct = Hct::new(src);
    let v = SpecVersion::Spec2025;
    let p = Platform::Phone;
    match kind {
        1 => SchemeVibrant::with_spec(hct, dark, None, v, p).scheme,
        2 => SchemeExpressive::with_spec(hct, dark, None, v, p).scheme,
        3 => SchemeNeutral::with_spec(hct, dark, None, v, p).scheme,
        4 => SchemeMonochrome::with_spec(hct, dark, None, v, p).scheme,
        _ => SchemeTonalSpot::with_spec(hct, dark, None, v, p).scheme,
    }
}

/// Applica tema chiaro o scuro con i colori calcolati
pub fn apply(ui: &AppWindow, dark: bool, wp: i32, palette: i32, kind: i32) {
    let s = scheme(source(ui, wp, palette), kind, dark);
    let t = ui.global::<Theme>();
    t.set_dark(dark);
    t.set_primary(col(s.primary()));
    t.set_on_primary(col(s.on_primary()));
    t.set_primary_container(col(s.primary_container()));
    t.set_on_primary_container(col(s.on_primary_container()));
    t.set_secondary(col(s.secondary()));
    t.set_on_secondary(col(s.on_secondary()));
    t.set_secondary_container(col(s.secondary_container()));
    t.set_on_secondary_container(col(s.on_secondary_container()));
    t.set_tertiary(col(s.tertiary()));
    t.set_on_tertiary(col(s.on_tertiary()));
    t.set_tertiary_container(col(s.tertiary_container()));
    t.set_on_tertiary_container(col(s.on_tertiary_container()));
    t.set_error(col(s.error()));
    t.set_on_error(col(s.on_error()));
    t.set_error_container(col(s.error_container()));
    t.set_on_error_container(col(s.on_error_container()));
    t.set_surface(col(s.surface()));
    t.set_surface_dim(col(s.surface_dim()));
    t.set_surface_bright(col(s.surface_bright()));
    t.set_container_lowest(col(s.surface_container_lowest()));
    t.set_container_low(col(s.surface_container_low()));
    t.set_container(col(s.surface_container()));
    t.set_container_high(col(s.surface_container_high()));
    t.set_container_highest(col(s.surface_container_highest()));
    t.set_on_surface(col(s.on_surface()));
    t.set_on_surface_variant(col(s.on_surface_variant()));
    t.set_outline(col(s.outline()));
    t.set_outline_variant(col(s.outline_variant()));
    t.set_inverse_surface(col(s.inverse_surface()));
    t.set_inverse_on_surface(col(s.inverse_on_surface()));
    t.set_inverse_primary(col(s.inverse_primary()));
    t.set_scrim(col(s.scrim()));
    swatches(ui, dark, wp, palette, kind);
    // Testo e orologio sopra lo sfondo
    let light = wallpaper_light(ui, wp);
    let on_wp = scheme(source(ui, wp, palette), kind, !light);
    let g = ui.global::<Style>();
    g.set_wp_light(light);
    g.set_wp_ink(if light { Color::from_rgb_u8(0x1b, 0x1b, 0x1f) } else { Color::from_rgb_u8(255, 255, 255) });
    g.set_wp_accent(col(if light { on_wp.on_primary_fixed_variant() } else { on_wp.primary_fixed() }));
}

/// Anteprime dei colori disponibili per la pagina Sfondo e stile
fn swatches(ui: &AppWindow, dark: bool, wp: i32, palette: i32, kind: i32) {
    let ws = wallpaper_seeds(ui, wp);
    let mut v = Vec::new();
    let all: Vec<(String, Rgb)> = ws
        .iter()
        .enumerate()
        .map(|(i, c)| (format!("Wallpaper {}", i + 1), *c))
        .chain(BASIC.iter().map(|(n, c)| (n.to_string(), rgb(*c))))
        .collect();
    for (i, (label, c)) in all.into_iter().enumerate() {
        let s = scheme(c, kind, dark);
        v.push(Swatch {
            id: i as i32,
            label: label.into(),
            primary: col(s.primary()),
            secondary: col(s.secondary_container()),
            tertiary: col(s.tertiary()),
            selected: i as i32 == palette,
        });
    }
    ui.global::<Style>().set_swatches(ModelRc::new(VecModel::from(v)));
}

/// Colori delle app per il pacchetto icone "Colors"
pub fn app_tone(id: &str) -> (Color, Color) {
    let c = |x: u32| Color::from_rgb_u8((x >> 16) as u8, (x >> 8) as u8, x as u8);
    let (bg, fg) = match id {
        "phone" => (0x1e8e3e, 0xffffff),
        "messages" => (0x1a73e8, 0xffffff),
        "browser" => (0xea4335, 0xffffff),
        "camera" => (0x3c4043, 0xffffff),
        "files" => (0x4285f4, 0xffffff),
        "calendar" => (0x3367d6, 0xffffff),
        "notes" => (0xfbbc04, 0x3c2f00),
        "clock" => (0x202124, 0x8ab4f8),
        "settings" => (0x5f6368, 0xffffff),
        "log" => (0x263238, 0x80cbc4),
        "calculator" => (0x4a3b00, 0xffe08a),
        _ => {
            let h = id.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            let palette = [0x7b1fa2, 0x00897b, 0xef6c00, 0x3949ab, 0xc2185b, 0x2e7d32];
            (palette[(h % palette.len() as u32) as usize], 0xffffff)
        }
    };
    (c(bg), c(fg))
}

/// Tema scuro automatico: scuro dalle 19 alle 7
pub fn auto_dark(hour: u32) -> bool {
    !(7..19).contains(&hour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_have_contrast() {
        for kind in 0..5 {
            for dark in [false, true] {
                let s = scheme(rgb(0xe8508a), kind, dark);
                let a = Hct::new(s.on_primary()).get_tone();
                let b = Hct::new(s.primary()).get_tone();
                assert!((a - b).abs() > 35.0, "kind {kind} dark {dark}: {a} {b}");
            }
        }
    }
}

#[cfg(all(test, feature = "screens"))]
mod gen {
    #[test]
    #[ignore]
    fn print_wallpaper_seeds() {
        for n in ["petal", "lagoon", "dune", "bloom", "graphite"] {
            let img = slint::Image::load_from_path(std::path::Path::new(&format!("ui/wallpapers/{n}.jpg"))).unwrap();
            let s: Vec<String> = super::seeds_of(&img).iter().map(|c| format!("0x{:02x}{:02x}{:02x}", c.red, c.green, c.blue)).collect();
            println!("SEED {n} [{}] light={}", s.join(", "), super::is_light(&img));
        }
    }
}
