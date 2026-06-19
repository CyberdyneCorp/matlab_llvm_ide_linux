//! Cairo rendering for the mflowLink overlay scope: many signals on one set of
//! axes with a legend, grid + numeric ticks, a hover crosshair with a per-signal
//! value readout, and an in-progress box-zoom rubber band. All scaling math
//! lives in the tested `matforge_core::services::scope`; this only paints.

use gtk::cairo;

use matforge_core::services::scope::Bounds;
use matforge_core::theme::Rgb;

/// One signal to overlay: a name, a stable color, and its `(time, value)` points.
pub struct ScopeSeries<'a> {
    pub name: &'a str,
    pub color: (f64, f64, f64),
    pub points: &'a [(f64, f64)],
}

const MARGIN_L: f64 = 48.0;
const MARGIN_B: f64 = 22.0;
const MARGIN_T: f64 = 8.0;
const MARGIN_R: f64 = 10.0;

/// The plot rect `(x, y, w, h)` inside the widget, leaving room for axis labels.
/// Public so the window's gestures map pixels with the same geometry the
/// renderer uses.
pub fn plot_rect(w: f64, h: f64) -> (f64, f64, f64, f64) {
    let pw = (w - MARGIN_L - MARGIN_R).max(1.0);
    let ph = (h - MARGIN_T - MARGIN_B).max(1.0);
    (MARGIN_L, MARGIN_T, pw, ph)
}

fn set_rgb(ctx: &cairo::Context, c: Rgb) {
    let (r, g, b) = c.to_unit();
    ctx.set_source_rgb(r, g, b);
}

fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        "0".to_string()
    } else if v.abs() >= 1000.0 || v.abs() < 0.01 {
        format!("{v:.2e}")
    } else {
        format!("{v:.3}")
    }
}

/// Draw the overlay scope. `window` is the already-resolved data window
/// (autoscale or pinned); `hover` is the cursor pixel; `drag_box` is the pixel
/// rect of an in-progress box-zoom.
pub fn draw_overlay(
    ctx: &cairo::Context,
    w: f64,
    h: f64,
    series: &[ScopeSeries],
    window: Bounds,
    hover: Option<(f64, f64)>,
    drag_box: Option<(f64, f64, f64, f64)>,
) {
    let tokens = crate::theme_css::current();
    set_rgb(ctx, tokens.card);
    ctx.rectangle(0.0, 0.0, w, h);
    ctx.fill().ok();

    let (px, py, pw, ph) = plot_rect(w, h);
    let (x0, x1, y0, y1) = window;
    let to_px = |x: f64| px + (x - x0) / (x1 - x0) * pw;
    let to_py = |y: f64| py + (y1 - y) / (y1 - y0) * ph;

    draw_grid(ctx, &tokens, (px, py, pw, ph), window);
    draw_series(ctx, series, (px, py, pw, ph), &to_px, &to_py);
    draw_legend(ctx, &tokens, series, px, py);
    if let Some((hx, hy)) = hover {
        if hx >= px && hx <= px + pw && hy >= py && hy <= py + ph {
            draw_crosshair(ctx, &tokens, series, (px, py, pw, ph), window, (hx, hy));
        }
    }
    if let Some((bx0, by0, bx1, by1)) = drag_box {
        set_rgb(ctx, tokens.blue);
        ctx.set_line_width(1.0);
        ctx.rectangle(
            bx0.min(bx1),
            by0.min(by1),
            (bx1 - bx0).abs(),
            (by1 - by0).abs(),
        );
        ctx.stroke().ok();
    }
}

fn draw_grid(
    ctx: &cairo::Context,
    tokens: &matforge_core::theme::ThemeTokens,
    plot: (f64, f64, f64, f64),
    window: Bounds,
) {
    let (px, py, pw, ph) = plot;
    let (x0, x1, y0, y1) = window;
    ctx.set_font_size(9.0);
    const DIV: usize = 5;
    for i in 0..=DIV {
        let f = i as f64 / DIV as f64;
        // Vertical grid line + x tick label.
        let gx = px + f * pw;
        set_rgb(ctx, tokens.border);
        ctx.set_line_width(0.5);
        ctx.move_to(gx, py);
        ctx.line_to(gx, py + ph);
        ctx.stroke().ok();
        set_rgb(ctx, tokens.text_secondary);
        ctx.move_to(gx - 12.0, py + ph + 14.0);
        ctx.show_text(&fmt_num(x0 + f * (x1 - x0))).ok();
        // Horizontal grid line + y tick label.
        let gy = py + f * ph;
        set_rgb(ctx, tokens.border);
        ctx.move_to(px, gy);
        ctx.line_to(px + pw, gy);
        ctx.stroke().ok();
        set_rgb(ctx, tokens.text_secondary);
        ctx.move_to(2.0, gy + 3.0);
        ctx.show_text(&fmt_num(y1 - f * (y1 - y0))).ok();
    }
}

fn draw_series(
    ctx: &cairo::Context,
    series: &[ScopeSeries],
    plot: (f64, f64, f64, f64),
    to_px: &dyn Fn(f64) -> f64,
    to_py: &dyn Fn(f64) -> f64,
) {
    let (px, py, pw, ph) = plot;
    ctx.rectangle(px, py, pw, ph);
    ctx.clip();
    ctx.set_line_width(1.4);
    for s in series {
        ctx.set_source_rgb(s.color.0, s.color.1, s.color.2);
        let mut started = false;
        for &(x, y) in s.points {
            if !x.is_finite() || !y.is_finite() {
                started = false;
                continue;
            }
            let (sx, sy) = (to_px(x), to_py(y));
            if started {
                ctx.line_to(sx, sy);
            } else {
                ctx.move_to(sx, sy);
                started = true;
            }
        }
        ctx.stroke().ok();
    }
    ctx.reset_clip();
}

fn draw_legend(
    ctx: &cairo::Context,
    tokens: &matforge_core::theme::ThemeTokens,
    series: &[ScopeSeries],
    px: f64,
    py: f64,
) {
    ctx.set_font_size(10.0);
    for (i, s) in series.iter().enumerate() {
        let ly = py + 6.0 + i as f64 * 14.0;
        ctx.set_source_rgb(s.color.0, s.color.1, s.color.2);
        ctx.rectangle(px + 8.0, ly, 9.0, 9.0);
        ctx.fill().ok();
        set_rgb(ctx, tokens.text_primary);
        ctx.move_to(px + 22.0, ly + 8.5);
        ctx.show_text(s.name).ok();
    }
}

fn draw_crosshair(
    ctx: &cairo::Context,
    tokens: &matforge_core::theme::ThemeTokens,
    series: &[ScopeSeries],
    plot: (f64, f64, f64, f64),
    window: Bounds,
    cursor: (f64, f64),
) {
    let (px, py, pw, ph) = plot;
    let (x0, x1, _, _) = window;
    let (hx, hy) = cursor;
    set_rgb(ctx, tokens.text_secondary);
    ctx.set_line_width(0.5);
    ctx.set_dash(&[3.0, 3.0], 0.0);
    ctx.move_to(hx, py);
    ctx.line_to(hx, py + ph);
    ctx.move_to(px, hy);
    ctx.line_to(px + pw, hy);
    ctx.stroke().ok();
    ctx.set_dash(&[], 0.0);

    // Readout: cursor time, then each signal's nearest-sample value.
    let cursor_x = x0 + (hx - px) / pw * (x1 - x0);
    let mut lines: Vec<(String, (f64, f64, f64))> = vec![(
        format!("t = {}", fmt_num(cursor_x)),
        tokens.text_primary.to_unit(),
    )];
    for s in series {
        if let Some(v) = nearest_value(s.points, cursor_x) {
            lines.push((format!("{} = {}", s.name, fmt_num(v)), s.color));
        }
    }
    let box_w = 132.0;
    let box_h = 6.0 + lines.len() as f64 * 13.0;
    let bx = (hx + 10.0).min(px + pw - box_w);
    let by = (hy + 10.0).min(py + ph - box_h);
    set_rgb(ctx, tokens.chrome);
    ctx.rectangle(bx, by, box_w, box_h);
    ctx.fill().ok();
    ctx.set_font_size(10.0);
    for (i, (text, color)) in lines.iter().enumerate() {
        ctx.set_source_rgb(color.0, color.1, color.2);
        ctx.move_to(bx + 6.0, by + 13.0 + i as f64 * 13.0);
        ctx.show_text(text).ok();
    }
}

/// Value of the sample whose time is closest to `x` (None for an empty series).
fn nearest_value(points: &[(f64, f64)], x: f64) -> Option<f64> {
    points
        .iter()
        .filter(|(t, v)| t.is_finite() && v.is_finite())
        .min_by(|a, b| {
            (a.0 - x)
                .abs()
                .partial_cmp(&(b.0 - x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|&(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use matforge_core::theme::ThemeTokens;

    /// Read the center-pixel `(r, g, b)` of a freshly rendered overlay.
    fn background_pixel(tokens: ThemeTokens) -> (u8, u8, u8) {
        crate::theme_css::set_current(tokens);
        let (w, h) = (80i32, 60i32);
        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).unwrap();
        {
            let ctx = cairo::Context::new(&surface).unwrap();
            // No series: the visible pixels are the card background fill.
            draw_overlay(
                &ctx,
                w as f64,
                h as f64,
                &[],
                (0.0, 1.0, 0.0, 1.0),
                None,
                None,
            );
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        // ARGB32 is little-endian premultiplied: bytes are [B, G, R, A].
        let off = (h as usize / 2) * stride + (w as usize / 2) * 4;
        (data[off + 2], data[off + 1], data[off])
    }

    #[test]
    fn background_uses_normalized_card_color_not_clamped_white() {
        // Regression: `set_rgb` must divide u8 channels by 255. Passing raw 0..255
        // to cairo's `set_source_rgb` clamps every channel to 1.0, painting the
        // whole scope white — background, grid, ticks and legend text vanish.
        let card = ThemeTokens::midnight().card;
        let (r, g, b) = background_pixel(ThemeTokens::midnight());
        assert_eq!(
            (r, g, b),
            (card.r, card.g, card.b),
            "scope background should be the card color, not clamped white",
        );
        assert_ne!((r, g, b), (255, 255, 255), "background clamped to white");
    }
}
