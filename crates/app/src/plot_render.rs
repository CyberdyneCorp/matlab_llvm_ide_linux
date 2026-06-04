//! Cairo rendering for `PlotFigure`s: line / multi-line / scatter / bar /
//! area (spectrum) / histogram series, and direct blit of runtime-rendered PNG
//! figures. Pure drawing — reads a figure, paints a `cairo::Context`.

use gtk::cairo;
use gtk::prelude::*;

use matforge_core::models::{MatrixView, PlotFigure, PlotKind, PlotView, SurfaceCamera};
use matforge_core::theme::Rgb;

const MARGIN: f64 = 40.0;

/// True if `(px, py)` is inside the plotting rectangle (within the axes).
pub fn in_plot_area(w: f64, h: f64, px: f64, py: f64) -> bool {
    px >= MARGIN && px <= w - MARGIN && py >= MARGIN && py <= h - MARGIN
}

/// Invert the data→pixel mapping: the data coordinate under a canvas pixel.
pub fn data_at_pixel(view: PlotView, w: f64, h: f64, px: f64, py: f64) -> (f64, f64) {
    let plot_w = (w - 2.0 * MARGIN).max(1.0);
    let plot_h = (h - 2.0 * MARGIN).max(1.0);
    (
        view.x_min + ((px - MARGIN) / plot_w) * view.x_span(),
        view.y_min + (1.0 - (py - MARGIN) / plot_h) * view.y_span(),
    )
}

/// Draw `figure` filling a `w`×`h` Cairo surface. `view` overrides the auto-fit
/// data window (zoom/pan); `hover` is the cursor pixel for the crosshair readout.
pub fn draw_figure(
    ctx: &cairo::Context,
    w: f64,
    h: f64,
    figure: &PlotFigure,
    view: Option<PlotView>,
    hover: Option<(f64, f64)>,
    reveal: Option<usize>,
) {
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

    let Some(view) = view.or_else(|| figure.auto_view()) else {
        return;
    };
    let xs: Vec<f64> = if figure.xs.len() == figure.ys.len() {
        figure.xs.clone()
    } else {
        (0..figure.ys.len()).map(|i| i as f64).collect()
    };
    // Trace animation: reveal only the first `n` points of each series.
    let n = reveal.map(|r| r.clamp(1, figure.ys.len())).unwrap_or(figure.ys.len());
    let (xs_r, ys_r) = (&xs[..n.min(xs.len())], &figure.ys[..n.min(figure.ys.len())]);
    let plot_w = (w - 2.0 * MARGIN).max(1.0);
    let plot_h = (h - 2.0 * MARGIN).max(1.0);
    let map = |x: f64, y: f64| -> (f64, f64) {
        let px = MARGIN + norm(x, view.x_min, view.x_max) * plot_w;
        let py = MARGIN + (1.0 - norm(y, view.y_min, view.y_max)) * plot_h;
        (px, py)
    };

    draw_axes(ctx, w, h, view.x_min, view.x_max, view.y_min, view.y_max);

    // Clip series to the plotting rectangle so zoom/pan never spills past the axes.
    ctx.save().ok();
    ctx.rectangle(MARGIN, MARGIN, plot_w, plot_h);
    ctx.clip();
    match figure.kind {
        PlotKind::Scatter => draw_scatter(ctx, xs_r, ys_r, &map, crate::theme_css::current().blue),
        PlotKind::Bar | PlotKind::Histogram => {
            draw_bars(ctx, ys_r, view.x_min, view.x_max, &map, plot_w)
        }
        PlotKind::Spectrum => draw_area(ctx, xs_r, ys_r, &map, h),
        _ => {
            draw_line(ctx, xs_r, ys_r, &map, crate::theme_css::current().blue);
            if !figure.ys2.is_empty() {
                let m = n.min(figure.ys2.len());
                let xs2: Vec<f64> = (0..m).map(|i| i as f64).collect();
                draw_line(ctx, &xs2, &figure.ys2[..m], &map, crate::theme_css::current().green);
            }
        }
    }
    ctx.restore().ok();

    // Hover crosshair + nearest-point readout.
    if let Some((hx, hy)) = hover {
        if in_plot_area(w, h, hx, hy) {
            draw_hover(ctx, w, h, figure, view, &map, hx);
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

/// Draw the hover crosshair at the data point nearest the cursor's x, with a
/// small readout chip showing its `(x, y)` value.
fn draw_hover(
    ctx: &cairo::Context,
    w: f64,
    h: f64,
    figure: &PlotFigure,
    view: PlotView,
    map: &dyn Fn(f64, f64) -> (f64, f64),
    hx: f64,
) {
    let (data_x, _) = data_at_pixel(view, w, h, hx, MARGIN);
    let Some((nx, ny)) = figure.nearest(data_x) else { return };
    let (mx, my) = map(nx, ny);
    if !in_plot_area(w, h, mx, my) {
        return;
    }

    // Crosshair lines across the plot rectangle.
    set_color(ctx, crate::theme_css::current().text_muted);
    ctx.set_line_width(0.8);
    ctx.set_dash(&[3.0, 3.0], 0.0);
    ctx.move_to(mx, MARGIN);
    ctx.line_to(mx, h - MARGIN);
    ctx.move_to(MARGIN, my);
    ctx.line_to(w - MARGIN, my);
    ctx.stroke().ok();
    ctx.set_dash(&[], 0.0);

    // Point marker.
    set_color(ctx, crate::theme_css::current().amber);
    ctx.arc(mx, my, 3.5, 0.0, std::f64::consts::TAU);
    ctx.fill().ok();

    // Readout chip.
    let label = format!("x {}   y {}", fmt_num(nx), fmt_num(ny));
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(11.0);
    let tw = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
    let (bw, bh) = (tw + 12.0, 18.0);
    let bx = (mx + 8.0).min(w - MARGIN - bw);
    let by = (my - bh - 6.0).max(MARGIN + 2.0);
    fill(ctx, crate::theme_css::current().card, bx, by, bw, bh);
    set_color(ctx, crate::theme_css::current().border);
    ctx.set_line_width(1.0);
    ctx.rectangle(bx, by, bw, bh);
    ctx.stroke().ok();
    set_color(ctx, crate::theme_css::current().text_primary);
    ctx.move_to(bx + 6.0, by + 13.0);
    ctx.show_text(&label).ok();
}

/// Compact numeric formatting for the readout (trims trailing zeros).
fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    let s = if a >= 1000.0 || a < 0.001 {
        format!("{v:.3e}")
    } else {
        format!("{v:.4}")
    };
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Render a 3-D surface from `figure.zs` as an orbitable, height-shaded mesh.
/// The grid is projected through `cam` and quads are drawn back-to-front
/// (painter's algorithm). Large grids are downsampled to keep the mesh light.
pub fn draw_surface(ctx: &cairo::Context, w: f64, h: f64, figure: &PlotFigure, cam: SurfaceCamera) {
    let t = crate::theme_css::current();
    fill(ctx, t.editor_bg, 0.0, 0.0, w, h);
    let grid = &figure.zs;
    let rows = grid.len();
    let cols = grid.iter().map(|r| r.len()).min().unwrap_or(0);
    if rows < 2 || cols < 2 {
        set_color(ctx, t.text_secondary);
        ctx.move_to(MARGIN, h / 2.0);
        ctx.show_text("[surface needs a 2-D matrix]").ok();
        return;
    }

    // Downsample so the mesh stays readable and cheap (~48 cells per axis).
    const MAX: usize = 48;
    let rstep = rows.div_ceil(MAX).max(1);
    let cstep = cols.div_ceil(MAX).max(1);
    let rs: Vec<usize> = (0..rows).step_by(rstep).chain([rows - 1]).collect();
    let cs: Vec<usize> = (0..cols).step_by(cstep).chain([cols - 1]).collect();
    let rs: Vec<usize> = dedup_sorted(rs);
    let cs: Vec<usize> = dedup_sorted(cs);

    // Height range for normalization + colour.
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for row in grid {
        for &v in row {
            if v.is_finite() {
                zmin = zmin.min(v);
                zmax = zmax.max(v);
            }
        }
    }
    if !zmin.is_finite() {
        return;
    }
    let zspan = if (zmax - zmin).abs() < 1e-12 { 1.0 } else { zmax - zmin };

    let (cx, cy) = (w / 2.0, h / 2.0 + 8.0);
    let radius = (w.min(h) * 0.42).max(1.0);
    let nr = rs.len();
    let nc = cs.len();
    // Project every sampled node to a screen pixel + keep its height.
    let project = |ri: usize, ci: usize| -> (f64, f64, f64) {
        let z = grid[rs[ri]][cs[ci]];
        let nx = ci as f64 / (nc - 1) as f64 - 0.5;
        let ny = ri as f64 / (nr - 1) as f64 - 0.5;
        let nz = (z - zmin) / zspan - 0.5;
        let (sx, sy, depth) = cam.project(nx, ny, nz);
        (cx + sx * radius, cy - sy * radius, depth)
    };

    // Build quads with an averaged depth + height, then sort far → near.
    let (cold, hot) = (t.blue, t.red);
    let mut quads: Vec<(f64, [(f64, f64); 4], f64)> = Vec::with_capacity((nr - 1) * (nc - 1));
    for ri in 0..nr - 1 {
        for ci in 0..nc - 1 {
            let p = [
                project(ri, ci),
                project(ri, ci + 1),
                project(ri + 1, ci + 1),
                project(ri + 1, ci),
            ];
            let depth = p.iter().map(|q| q.2).sum::<f64>() / 4.0;
            let zavg = (grid[rs[ri]][cs[ci]] + grid[rs[ri]][cs[ci + 1]]
                + grid[rs[ri + 1]][cs[ci]] + grid[rs[ri + 1]][cs[ci + 1]])
                / 4.0;
            let pts = [(p[0].0, p[0].1), (p[1].0, p[1].1), (p[2].0, p[2].1), (p[3].0, p[3].1)];
            quads.push((depth, pts, (zavg - zmin) / zspan));
        }
    }
    quads.sort_by(|a, b| b.0.total_cmp(&a.0));

    let (er, eg, eb) = t.border.to_unit();
    for (_, pts, ht) in &quads {
        ctx.move_to(pts[0].0, pts[0].1);
        for q in &pts[1..] {
            ctx.line_to(q.0, q.1);
        }
        ctx.close_path();
        set_color(ctx, cold.blend(hot, *ht));
        ctx.fill_preserve().ok();
        ctx.set_source_rgba(er, eg, eb, 0.6);
        ctx.set_line_width(0.6);
        ctx.stroke().ok();
    }

    // Title + orientation hint.
    set_color(ctx, t.text_primary);
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    ctx.set_font_size(12.0);
    ctx.move_to(MARGIN, 20.0);
    ctx.show_text(&figure.title).ok();
    set_color(ctx, t.text_muted);
    ctx.set_font_size(10.0);
    ctx.move_to(MARGIN, h - 12.0);
    ctx.show_text("drag to orbit · scroll to zoom · double-click to reset").ok();
}

/// Drop consecutive duplicates from an ascending index list.
fn dedup_sorted(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v.dedup();
    v
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

/// Paint a single animation frame (a runtime PNG) filling a `w`×`h` surface.
/// Used by the Plots playback scrubber to show a chosen frame rather than the
/// figure's latest (`png_data`).
pub fn draw_png_frame(ctx: &cairo::Context, w: f64, h: f64, png: &[u8]) {
    fill(ctx, crate::theme_css::current().editor_bg, 0.0, 0.0, w, h);
    if !blit_png(ctx, w, h, png) {
        set_color(ctx, crate::theme_css::current().text_secondary);
        ctx.move_to(MARGIN, h / 2.0);
        ctx.show_text("[frame]").ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The forward data→pixel map (mirrors the closure in `draw_figure`).
    fn forward(view: PlotView, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
        let plot_w = w - 2.0 * MARGIN;
        let plot_h = h - 2.0 * MARGIN;
        (
            MARGIN + norm(x, view.x_min, view.x_max) * plot_w,
            MARGIN + (1.0 - norm(y, view.y_min, view.y_max)) * plot_h,
        )
    }

    #[test]
    fn data_at_pixel_inverts_forward_map() {
        let view = PlotView { x_min: -2.0, x_max: 6.0, y_min: 1.0, y_max: 9.0 };
        let (w, h) = (800.0, 500.0);
        for &(dx, dy) in &[(-2.0, 1.0), (0.0, 5.0), (3.5, 7.25), (6.0, 9.0)] {
            let (px, py) = forward(view, w, h, dx, dy);
            let (rx, ry) = data_at_pixel(view, w, h, px, py);
            assert!((rx - dx).abs() < 1e-9, "x {dx} -> {px} -> {rx}");
            assert!((ry - dy).abs() < 1e-9, "y {dy} -> {py} -> {ry}");
        }
        // The plot-area test rejects the axis margins.
        assert!(in_plot_area(w, h, w / 2.0, h / 2.0));
        assert!(!in_plot_area(w, h, MARGIN / 2.0, h / 2.0));
    }

    #[test]
    fn fmt_num_is_compact() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(2.0), "2");
        assert_eq!(fmt_num(1234.0), "1.234e3");
    }
}
