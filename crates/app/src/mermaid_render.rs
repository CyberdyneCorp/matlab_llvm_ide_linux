//! Cairo painter for the Markdown preview's mermaid diagrams. Core lays the
//! graph out into pixel-space [`Scene`] geometry; this only draws it (theme-aware
//! node fills, borders, edge arrows, and labels), matching the flowchart canvas's
//! visual style. Build a `DrawingArea` for a parsed scene with [`drawing_area`].

use gtk::cairo;
use gtk::prelude::*;
use gtk::DrawingArea;

use matforge_core::services::mermaid::{Scene, Shape};
use matforge_core::theme::Rgb;

const FONT: f64 = 13.0;

/// A `DrawingArea` sized to `scene`, painting it on demand (re-tints with theme).
pub fn drawing_area(scene: Scene) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_content_width(scene.width.ceil() as i32);
    area.set_content_height(scene.height.ceil() as i32);
    area.set_halign(gtk::Align::Start);
    area.add_css_class("mf-md-mermaid");
    area.set_draw_func(move |_a, ctx, _w, _h| draw(ctx, &scene));
    area
}

fn set_rgb(ctx: &cairo::Context, c: Rgb) {
    let (r, g, b) = c.to_unit();
    ctx.set_source_rgb(r, g, b);
}

fn draw(ctx: &cairo::Context, scene: &Scene) {
    let t = crate::theme_css::current();
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(FONT);

    // Edges first so node fills sit on top of the line ends.
    for e in &scene.edges {
        set_rgb(ctx, t.text_secondary);
        ctx.set_line_width(1.5);
        ctx.move_to(e.from.0, e.from.1);
        ctx.line_to(e.to.0, e.to.1);
        ctx.stroke().ok();
        arrowhead(ctx, e.from, e.to, t.text_secondary);

        if let Some(label) = &e.label {
            label_chip(ctx, label, e.label_pos, &t);
        }
    }

    for node in &scene.nodes {
        node_path(ctx, node);
        set_rgb(ctx, t.card);
        ctx.fill_preserve().ok();
        set_rgb(ctx, t.accent);
        ctx.set_line_width(1.6);
        ctx.stroke().ok();

        // Centered label.
        set_rgb(ctx, t.text_primary);
        if let Ok(ext) = ctx.text_extents(&node.label) {
            let cx = node.x + node.w / 2.0 - ext.width() / 2.0 - ext.x_bearing();
            let cy = node.y + node.h / 2.0 - ext.height() / 2.0 - ext.y_bearing();
            ctx.move_to(cx, cy);
            ctx.show_text(&node.label).ok();
        }
    }
}

/// Trace `node`'s outline onto the current path according to its shape.
fn node_path(ctx: &cairo::Context, node: &matforge_core::services::mermaid::SceneNode) {
    let (x, y, w, h) = (node.x, node.y, node.w, node.h);
    match node.shape {
        Shape::Rect => rounded_rect(ctx, x, y, w, h, 6.0),
        Shape::Round => rounded_rect(ctx, x, y, w, h, 12.0),
        Shape::Stadium => rounded_rect(ctx, x, y, w, h, h / 2.0),
        Shape::Circle => {
            let r = (w.max(h)) / 2.0;
            ctx.new_sub_path();
            ctx.arc(x + w / 2.0, y + h / 2.0, r, 0.0, std::f64::consts::TAU);
        }
        Shape::Diamond => {
            ctx.new_sub_path();
            ctx.move_to(x + w / 2.0, y);
            ctx.line_to(x + w, y + h / 2.0);
            ctx.line_to(x + w / 2.0, y + h);
            ctx.line_to(x, y + h / 2.0);
            ctx.close_path();
        }
        Shape::Hexagon => {
            let inset = (h / 2.0).min(w / 4.0);
            ctx.new_sub_path();
            ctx.move_to(x + inset, y);
            ctx.line_to(x + w - inset, y);
            ctx.line_to(x + w, y + h / 2.0);
            ctx.line_to(x + w - inset, y + h);
            ctx.line_to(x + inset, y + h);
            ctx.line_to(x, y + h / 2.0);
            ctx.close_path();
        }
    }
}

fn rounded_rect(ctx: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let deg = std::f64::consts::PI / 180.0;
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -90.0 * deg, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, 90.0 * deg);
    ctx.arc(x + r, y + h - r, r, 90.0 * deg, 180.0 * deg);
    ctx.arc(x + r, y + r, r, 180.0 * deg, 270.0 * deg);
    ctx.close_path();
}

/// A filled triangular arrowhead at `to`, pointing along `from`→`to`.
fn arrowhead(ctx: &cairo::Context, from: (f64, f64), to: (f64, f64), color: Rgb) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let size = 8.0;
    let (px, py) = (-uy, ux); // perpendicular
    let base = (to.0 - ux * size, to.1 - uy * size);
    set_rgb(ctx, color);
    ctx.move_to(to.0, to.1);
    ctx.line_to(base.0 + px * size * 0.5, base.1 + py * size * 0.5);
    ctx.line_to(base.0 - px * size * 0.5, base.1 - py * size * 0.5);
    ctx.close_path();
    ctx.fill().ok();
}

/// Draw an edge `label` in a small filled chip centered on `pos`.
fn label_chip(
    ctx: &cairo::Context,
    label: &str,
    pos: (f64, f64),
    t: &matforge_core::theme::ThemeTokens,
) {
    let Ok(ext) = ctx.text_extents(label) else { return };
    let pad = 3.0;
    let (bw, bh) = (ext.width() + 2.0 * pad, FONT + 2.0 * pad);
    let (bx, by) = (pos.0 - bw / 2.0, pos.1 - bh / 2.0);
    rounded_rect(ctx, bx, by, bw, bh, 3.0);
    set_rgb(ctx, t.panel);
    ctx.fill().ok();
    set_rgb(ctx, t.text_secondary);
    ctx.move_to(pos.0 - ext.width() / 2.0 - ext.x_bearing(), pos.1 + FONT / 2.0 - 2.0);
    ctx.show_text(label).ok();
}
