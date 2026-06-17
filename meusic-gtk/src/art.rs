//! Cover-art helpers: decode bytes into a GDK texture (for display) and extract
//! a small palette of dominant, vivified colors (for the adaptive gradient +
//! accent). The palette algorithm is ported from the React app's colors.ts.

use relm4::gtk;
use gtk::gdk;
use gtk::gdk_pixbuf::{InterpType, Pixbuf};
use gtk::prelude::*;
use gtk::{gio, glib};

pub type Rgb = (u8, u8, u8);

/// Neutral fallback palette when a track has no art.
pub fn default_palette() -> Vec<Rgb> {
    vec![vivify((60, 50, 90))]
}

/// Decode encoded image bytes into a GDK texture for display. Main-thread only
/// (GDK objects are not Send).
pub fn texture_from_bytes(bytes: &[u8]) -> Option<gdk::Texture> {
    gdk::Texture::from_bytes(&glib::Bytes::from(bytes)).ok()
}

fn decode_scaled(bytes: &[u8], size: i32) -> Option<Pixbuf> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from(bytes));
    let pb = Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()?;
    pb.scale_simple(size, size, InterpType::Bilinear)
}

/// Extract up to `count` dominant, vivid colors from cover-art bytes: downscale
/// to 64×64, bucket by a coarse 4-bit quantization, rank by population weighted
/// toward saturation, de-duplicate near colors, and vivify.
pub fn palette_from_bytes(bytes: &[u8], count: usize) -> Vec<Rgb> {
    let Some(pb) = decode_scaled(bytes, 64) else {
        return default_palette();
    };
    let channels = pb.n_channels() as usize;
    let rowstride = pb.rowstride() as usize;
    let width = pb.width() as usize;
    let height = pb.height() as usize;
    let pixels = pb.read_pixel_bytes();
    let data: &[u8] = &pixels;

    // bucket key -> (sum_r, sum_g, sum_b, n)
    let mut buckets: std::collections::HashMap<(u8, u8, u8), (u64, u64, u64, u64)> =
        std::collections::HashMap::new();
    for y in 0..height {
        for x in 0..width {
            let o = y * rowstride + x * channels;
            if o + 2 >= data.len() {
                continue;
            }
            if channels == 4 && data[o + 3] < 125 {
                continue; // skip transparent
            }
            let (r, g, b) = (data[o], data[o + 1], data[o + 2]);
            let key = (r >> 4, g >> 4, b >> 4);
            let e = buckets.entry(key).or_insert((0, 0, 0, 0));
            e.0 += r as u64;
            e.1 += g as u64;
            e.2 += b as u64;
            e.3 += 1;
        }
    }
    if buckets.is_empty() {
        return default_palette();
    }

    let mut scored: Vec<(Rgb, f64)> = buckets
        .values()
        .map(|&(sr, sg, sb, n)| {
            let nf = n as f64;
            let r = sr as f64 / nf;
            let g = sg as f64 / nf;
            let b = sb as f64 / nf;
            let score = nf * (0.4 + saturation(r, g, b));
            ((r.round() as u8, g.round() as u8, b.round() as u8), score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut picked: Vec<Rgb> = Vec::new();
    for (c, _) in scored {
        if picked.iter().all(|p| color_dist(*p, c) > 48.0) {
            picked.push(c);
        }
        if picked.len() >= count {
            break;
        }
    }
    if picked.is_empty() {
        return default_palette();
    }
    picked.into_iter().map(vivify).collect()
}

fn saturation(r: f64, g: f64, b: f64) -> f64 {
    let max = r.max(g).max(b) / 255.0;
    let min = r.min(g).min(b) / 255.0;
    if max == 0.0 {
        0.0
    } else {
        (max - min) / max
    }
}

fn color_dist(a: Rgb, b: Rgb) -> f64 {
    let dr = a.0 as f64 - b.0 as f64;
    let dg = a.1 as f64 - b.1 as f64;
    let db = a.2 as f64 - b.2 as f64;
    (dr * dr + dg * dg + db * db).sqrt()
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let mut h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    (h, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

/// Render a station's chip (color circle + initials) to a temp PNG and return a
/// `file://` URL — used as the MPRIS artUrl so the GNOME popup shows the station
/// chip instead of a generic icon.
pub fn station_art_file(name: &str) -> Option<String> {
    use gtk::cairo;
    use std::f64::consts::TAU;
    use std::hash::{Hash, Hasher};

    let size = 320;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let s = size as f64;
        let (r, g, b) = station_color(name);
        cr.set_source_rgb(0.055, 0.055, 0.075);
        let _ = cr.paint();
        cr.set_source_rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        cr.arc(s / 2.0, s / 2.0, s * 0.42, 0.0, TAU);
        let _ = cr.fill();
        let initials = crate::stations::initials(name);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size(s * 0.32);
        if let Ok(ext) = cr.text_extents(&initials) {
            cr.move_to(
                s / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
                s / 2.0 - ext.height() / 2.0 - ext.y_bearing(),
            );
            let _ = cr.show_text(&initials);
        }
    }
    surface.flush();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    let dir = std::env::temp_dir().join("meusic-art");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("station-{:016x}.png", hasher.finish()));
    // Save via gdk-pixbuf (the cairo `png` feature isn't enabled).
    let pixbuf = gdk::pixbuf_get_from_surface(&surface, 0, 0, size, size)?;
    pixbuf.savev(&file, "png", &[]).ok()?;
    Some(crate::library::file_uri(&file))
}

/// Deterministic vivid color for a radio station (no cover art) — same name
/// always yields the same hue, mid saturation/lightness for legibility.
pub fn station_color(name: &str) -> Rgb {
    let mut h: u32 = 0;
    for ch in name.chars() {
        h = h.wrapping_mul(31).wrapping_add(ch as u32);
    }
    hsl_to_rgb((h % 360) as f64, 0.55, 0.5)
}

/// Push a color toward a richer, more luminous version so even muted covers
/// yield a visible gradient (boost saturation, clamp lightness 0.4–0.6).
pub fn vivify(c: Rgb) -> Rgb {
    let (h, s, l) = rgb_to_hsl(c.0 as f64, c.1 as f64, c.2 as f64);
    let ns = (s * 1.45 + 0.12).min(1.0);
    let nl = l.clamp(0.4, 0.6);
    hsl_to_rgb(h, ns, nl)
}
