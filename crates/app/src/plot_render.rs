//! Cairo rendering for `PlotFigure`s: line / multi-line / scatter / bar /
//! area (spectrum) / histogram series, and direct blit of runtime-rendered PNG
//! figures. Pure drawing — reads a figure, paints a `cairo::Context`.

use gtk::cairo;
use gtk::prelude::*;

use matforge_core::models::{MatrixView, PlotFigure, PlotKind};
use matforge_core::theme::Rgb;

const MARGIN: f64 = 40.0;

/// Draw `figure` filling a `w`×`h` Cairo surface.
pub fn draw_figure(ctx: &cairo::Context, w: f64, h: f64, figure: &PlotFigure) {
    fill(ctx, crate::theme_css::current().editor_bg, 0.0, 0.0, w, h);

    // Runtime PNG figure: decode via GDK (no cairo `png` feature needed) and
    // blit. GDK downloads in cairo's native ARGB32 layout.
    if let Some(png) = &figure.png_data {
        if blit_png(ctx, w, h, png) {
            return;
        }
        set_color(ctx, crate::theme_css::current().text_secondary);
        ctx.move_to(MARGIN, h / 2.0);
        ctx.show_text(&format!("[figure: {}]", figure.title)).ok();
        return;
    }

    if figure.ys.is_empty() {
        return;
    }

    // Data ranges.
    let xs: Vec<f64> = if figure.xs.len() == figure.ys.len() {
        figure.xs.clone()
    } else {
        (0..figure.ys.len()).map(|i| i as f64).collect()
    };
    let (x_min, x_max) = range(&xs);
    let (mut y_min, mut y_max) = range(&figure.ys);
    if !figure.ys2.is_empty() {
        let (a, b) = range(&figure.ys2);
        y_min = y_min.min(a);
        y_max = y_max.max(b);
    }
    if figure.kind == PlotKind::Bar || figure.kind == PlotKind::Histogram {
        y_min = y_min.min(0.0); // bars sit on the zero baseline
    }
    let plot_w = (w - 2.0 * MARGIN).max(1.0);
    let plot_h = (h - 2.0 * MARGIN).max(1.0);
    let map = |x: f64, y: f64| -> (f64, f64) {
        let px = MARGIN + norm(x, x_min, x_max) * plot_w;
        let py = MARGIN + (1.0 - norm(y, y_min, y_max)) * plot_h;
        (px, py)
    };

    draw_axes(ctx, w, h, x_min, x_max, y_min, y_max);

    match figure.kind {
        PlotKind::Scatter => draw_scatter(ctx, &xs, &figure.ys, &map, crate::theme_css::current().blue),
        PlotKind::Bar | PlotKind::Histogram => {
            draw_bars(ctx, &figure.ys, x_min, x_max, &map, plot_w)
        }
        PlotKind::Spectrum => draw_area(ctx, &xs, &figure.ys, &map, h),
        _ => {
            draw_line(ctx, &xs, &figure.ys, &map, crate::theme_css::current().blue);
            if !figure.ys2.is_empty() {
                let xs2: Vec<f64> = (0..figure.ys2.len()).map(|i| i as f64).collect();
                draw_line(ctx, &xs2, &figure.ys2, &map, crate::theme_css::current().green);
            }
        }
    }

    // Legend (only meaningful with a second series).
    if !figure.ys2.is_empty() {
        draw_legend(ctx, w, figure.source_variable.as_deref());
    }

    // Title.
    set_color(ctx, crate::theme_css::current().text_primary);
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    ctx.set_font_size(12.0);
    ctx.move_to(MARGIN, 20.0);
    ctx.show_text(&figure.title).ok();
}

/// Compact thumbnail render for the figure list: blits a runtime PNG (scaled to
/// fit) or draws a bare series chart — no title, axis labels, or legend.
pub fn draw_thumbnail(ctx: &cairo::Context, w: f64, h: f64, figure: &PlotFigure) {
    fill(ctx, crate::theme_css::current().editor_bg, 0.0, 0.0, w, h);
    if let Some(png) = &figure.png_data {
        blit_png(ctx, w, h, png);
        return;
    }
    if figure.ys.is_empty() {
        return;
    }
    let xs: Vec<f64> = if figure.xs.len() == figure.ys.len() {
        figure.xs.clone()
    } else {
        (0..figure.ys.len()).map(|i| i as f64).collect()
    };
    let (x_min, x_max) = range(&xs);
    let (mut y_min, mut y_max) = range(&figure.ys);
    if figure.kind == PlotKind::Bar || figure.kind == PlotKind::Histogram {
        y_min = y_min.min(0.0);
    }
    if (y_max - y_min).abs() < f64::EPSILON {
        y_max += 1.0;
    }
    let pad = 3.0;
    let plot_w = (w - 2.0 * pad).max(1.0);
    let plot_h = (h - 2.0 * pad).max(1.0);
    let map = |x: f64, y: f64| -> (f64, f64) {
        (pad + norm(x, x_min, x_max) * plot_w, pad + (1.0 - norm(y, y_min, y_max)) * plot_h)
    };
    match figure.kind {
        PlotKind::Scatter => draw_scatter(ctx, &xs, &figure.ys, &map, crate::theme_css::current().blue),
        PlotKind::Bar | PlotKind::Histogram => draw_bars(ctx, &figure.ys, x_min, x_max, &map, plot_w),
        _ => draw_line(ctx, &xs, &figure.ys, &map, crate::theme_css::current().blue),
    }
}

fn draw_axes(ctx: &cairo::Context, w: f64, h: f64, x_min: f64, x_max: f64, y_min: f64, y_max: f64) {
    let t = crate::theme_css::current();
    let plot_h = h - 2.0 * MARGIN;
    const DIV: usize = 4;

    // Faint horizontal gridlines + y tick labels at each division.
    let (gr, gg, gb) = t.border.to_unit();
    ctx.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(9.0);
    for i in 0..=DIV {
        let frac = i as f64 / DIV as f64;
        let py = MARGIN + (1.0 - frac) * plot_h;
        ctx.set_source_rgba(gr, gg, gb, 0.4);
        ctx.set_line_width(0.5);
        ctx.move_to(MARGIN, py);
        ctx.line_to(w - MARGIN, py);
        ctx.stroke().ok();
        let val = y_min + (y_max - y_min) * frac;
        set_color(ctx, t.text_muted);
        ctx.move_to(3.0, py + 3.0);
        ctx.show_text(&format!("{val:.2}")).ok();
    }

    // Axis frame (solid L).
    set_color(ctx, t.border);
    ctx.set_line_width(1.0);
    ctx.move_to(MARGIN, MARGIN);
    ctx.line_to(MARGIN, h - MARGIN);
    ctx.line_to(w - MARGIN, h - MARGIN);
    ctx.stroke().ok();

    // x range (bottom corners).
    set_color(ctx, t.text_muted);
    ctx.move_to(MARGIN, h - MARGIN + 12.0);
    ctx.show_text(&format!("{x_min:.2}")).ok();
    let x_hi = format!("{x_max:.2}");
    let ext = ctx.text_extents(&x_hi).map(|e| e.width()).unwrap_or(0.0);
    ctx.move_to(w - MARGIN - ext, h - MARGIN + 12.0);
    ctx.show_text(&x_hi).ok();
}

/// Two-entry legend in the top-right (series 1 = blue, series 2 = green).
fn draw_legend(ctx: &cairo::Context, w: f64, source: Option<&str>) {
    let entries = [
        (crate::theme_css::current().blue, source.unwrap_or("series 1")),
        (crate::theme_css::current().green, "series 2"),
    ];
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(10.0);
    let x = w - MARGIN - 92.0;
    let mut y = MARGIN + 6.0;
    for (color, label) in entries {
        set_color(ctx, color);
        ctx.rectangle(x, y - 7.0, 12.0, 8.0);
        ctx.fill().ok();
        set_color(ctx, crate::theme_css::current().text_secondary);
        ctx.move_to(x + 16.0, y);
        ctx.show_text(label).ok();
        y += 14.0;
    }
}

fn draw_line(ctx: &cairo::Context, xs: &[f64], ys: &[f64], map: &impl Fn(f64, f64) -> (f64, f64), color: Rgb) {
    set_color(ctx, color);
    ctx.set_line_width(1.8);
    for (i, (&x, &y)) in xs.iter().zip(ys).enumerate() {
        let (px, py) = map(x, y);
        if i == 0 {
            ctx.move_to(px, py);
        } else {
            ctx.line_to(px, py);
        }
    }
    ctx.stroke().ok();
}

fn draw_scatter(ctx: &cairo::Context, xs: &[f64], ys: &[f64], map: &impl Fn(f64, f64) -> (f64, f64), color: Rgb) {
    set_color(ctx, color);
    for (&x, &y) in xs.iter().zip(ys) {
        let (px, py) = map(x, y);
        ctx.arc(px, py, 2.5, 0.0, std::f64::consts::TAU);
        ctx.fill().ok();
    }
}

fn draw_bars(ctx: &cairo::Context, ys: &[f64], _x_min: f64, _x_max: f64, map: &impl Fn(f64, f64) -> (f64, f64), plot_w: f64) {
    set_color(ctx, crate::theme_css::current().cyan);
    let n = ys.len().max(1);
    let bw = (plot_w / n as f64) * 0.7;
    for (i, &y) in ys.iter().enumerate() {
        let (px, py) = map(i as f64, y);
        let (_, base) = map(i as f64, 0.0);
        let top = py.min(base);
        let height = (py - base).abs();
        ctx.rectangle(px - bw / 2.0, top, bw, height);
        ctx.fill().ok();
    }
}

fn draw_area(ctx: &cairo::Context, xs: &[f64], ys: &[f64], map: &impl Fn(f64, f64) -> (f64, f64), h: f64) {
    ctx.set_source_rgba(
        rgb_unit(crate::theme_css::current().magenta).0,
        rgb_unit(crate::theme_css::current().magenta).1,
        rgb_unit(crate::theme_css::current().magenta).2,
        0.35,
    );
    let base = h - MARGIN;
    for (i, (&x, &y)) in xs.iter().zip(ys).enumerate() {
        let (px, py) = map(x, y);
        if i == 0 {
            ctx.move_to(px, base);
            ctx.line_to(px, py);
        } else {
            ctx.line_to(px, py);
        }
    }
    if let Some(&lastx) = xs.last() {
        let (px, _) = map(lastx, 0.0);
        ctx.line_to(px, base);
    }
    ctx.close_path();
    ctx.fill().ok();
    draw_line(ctx, xs, ys, map, crate::theme_css::current().magenta);
}

/// Paint the empty-plots placeholder (dark background + hint) so the panel
/// never shows a bare white surface.
pub fn draw_empty(ctx: &cairo::Context, w: f64, h: f64) {
    fill(ctx, crate::theme_css::current().editor_bg, 0.0, 0.0, w, h);
    set_color(ctx, crate::theme_css::current().text_muted);
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(12.0);
    let msg = "No figures yet.";
    let ext = ctx.text_extents(msg).map(|e| e.width()).unwrap_or(0.0);
    ctx.move_to((w - ext) / 2.0, h / 2.0 - 6.0);
    ctx.show_text(msg).ok();
    let hint = "Click + or plot a workspace variable.";
    let ext2 = ctx.text_extents(hint).map(|e| e.width()).unwrap_or(0.0);
    ctx.move_to((w - ext2) / 2.0, h / 2.0 + 12.0);
    ctx.show_text(hint).ok();
}

/// Render a matrix as a cold→hot heatmap (blue → red) for the Matrix Viewer.
pub fn draw_heatmap(ctx: &cairo::Context, w: f64, h: f64, m: &MatrixView) {
    fill(ctx, crate::theme_css::current().editor_bg, 0.0, 0.0, w, h);
    if m.rows == 0 || m.cols == 0 {
        return;
    }
    let Some((lo, hi)) = m.value_range() else { return };
    let bar_w = 14.0;
    let grid_w = (w - bar_w - 12.0).max(1.0);
    let cw = grid_w / m.cols as f64;
    let ch = (h / m.rows as f64).max(1.0);
    let (cold, hot) = (crate::theme_css::current().blue, crate::theme_css::current().red);
    for (r, row) in m.cells.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            let t = if (hi - lo).abs() < 1e-12 { 0.5 } else { (v - lo) / (hi - lo) };
            set_color(ctx, cold.blend(hot, t));
            ctx.rectangle(c as f64 * cw, r as f64 * ch, (cw - 1.0).max(1.0), (ch - 1.0).max(1.0));
            ctx.fill().ok();
        }
    }
    // Colorbar.
    let steps = 32;
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        set_color(ctx, cold.blend(hot, 1.0 - t));
        ctx.rectangle(w - bar_w, t * h, bar_w, h / steps as f64 + 1.0);
        ctx.fill().ok();
    }
}

/// Decode PNG bytes with GDK and paint them centered + scaled to fit.
fn blit_png(ctx: &cairo::Context, w: f64, h: f64, png: &[u8]) -> bool {
    let bytes = gtk::glib::Bytes::from(png);
    let Ok(texture) = gtk::gdk::Texture::from_bytes(&bytes) else { return false };
    let (iw, ih) = (texture.width(), texture.height());
    if iw <= 0 || ih <= 0 {
        return false;
    }
    let stride = iw * 4;
    let mut data = vec![0u8; (ih * stride) as usize];
    texture.download(&mut data, stride as usize);
    let Ok(surface) =
        cairo::ImageSurface::create_for_data(data, cairo::Format::ARgb32, iw, ih, stride)
    else {
        return false;
    };
    let scale = (w / iw as f64).min(h / ih as f64).min(1.0);
    ctx.save().ok();
    ctx.translate((w - iw as f64 * scale) / 2.0, (h - ih as f64 * scale) / 2.0);
    ctx.scale(scale, scale);
    if ctx.set_source_surface(&surface, 0.0, 0.0).is_ok() {
        ctx.paint().ok();
    }
    ctx.restore().ok();
    true
}

fn range(v: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &x in v.iter().filter(|x| x.is_finite()) {
        lo = lo.min(x);
        hi = hi.max(x);
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if (hi - lo).abs() < 1e-12 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    }
}

fn norm(v: f64, lo: f64, hi: f64) -> f64 {
    if (hi - lo).abs() < 1e-12 {
        0.5
    } else {
        (v - lo) / (hi - lo)
    }
}

fn fill(ctx: &cairo::Context, c: Rgb, x: f64, y: f64, w: f64, h: f64) {
    set_color(ctx, c);
    ctx.rectangle(x, y, w, h);
    ctx.fill().ok();
}

fn set_color(ctx: &cairo::Context, c: Rgb) {
    let (r, g, b) = rgb_unit(c);
    ctx.set_source_rgb(r, g, b);
}

fn rgb_unit(c: Rgb) -> (f64, f64, f64) {
    c.to_unit()
}
