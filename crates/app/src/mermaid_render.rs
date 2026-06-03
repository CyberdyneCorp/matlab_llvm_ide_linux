//! Cairo painter for the Markdown preview's mermaid diagrams. Core lays a
//! diagram out into pixel-space geometry; this only draws it (theme-aware fills,
//! borders, arrows, and labels). Flowcharts paint a [`Scene`] of nodes + edges
//! ([`drawing_area`]); sequence diagrams paint a [`SeqScene`] of lifelines +
//! messages ([`drawing_area_seq`]).

use gtk::cairo;
use gtk::prelude::*;
use gtk::DrawingArea;

use matforge_core::services::mermaid::{Arrow, ClassScene, Marker, Scene, SeqScene, Shape};
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

// ----- sequence diagrams -----------------------------------------------------

const SELF_LOOP_W: f64 = 44.0;

/// A `DrawingArea` sized to `scene`, painting a sequence diagram on demand.
pub fn drawing_area_seq(scene: SeqScene) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_content_width(scene.width.ceil() as i32);
    area.set_content_height(scene.height.ceil() as i32);
    area.set_halign(gtk::Align::Start);
    area.add_css_class("mf-md-mermaid");
    area.set_draw_func(move |_a, ctx, _w, _h| draw_seq(ctx, &scene));
    area
}

fn draw_seq(ctx: &cairo::Context, scene: &SeqScene) {
    let t = crate::theme_css::current();
    ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(FONT);

    // Dashed lifelines first, behind everything.
    set_rgb(ctx, t.border);
    ctx.set_line_width(1.0);
    ctx.set_dash(&[4.0, 3.0], 0.0);
    for &(x, y_top, y_bottom) in &scene.lifelines {
        ctx.move_to(x, y_top);
        ctx.line_to(x, y_bottom);
        ctx.stroke().ok();
    }
    ctx.set_dash(&[], 0.0);

    // Messages.
    for m in &scene.messages {
        set_rgb(ctx, t.text_secondary);
        ctx.set_line_width(1.5);
        if m.dashed {
            ctx.set_dash(&[5.0, 3.0], 0.0);
        }
        if m.self_loop {
            draw_self_loop(ctx, m, &t);
        } else {
            ctx.move_to(m.from.0, m.from.1);
            ctx.line_to(m.to.0, m.to.1);
            ctx.stroke().ok();
            ctx.set_dash(&[], 0.0);
            message_head(ctx, m.from, m.to, m.arrow, &t);
            if !m.label.is_empty() {
                let mid = ((m.from.0 + m.to.0) / 2.0, m.from.1 - 5.0);
                set_rgb(ctx, t.text_primary);
                centered_text(ctx, &m.label, mid.0, mid.1);
            }
        }
        ctx.set_dash(&[], 0.0);
    }

    // Boxes (participant headers/footers and notes) on top.
    for b in &scene.boxes {
        rounded_rect(ctx, b.x, b.y, b.w, b.h, 5.0);
        set_rgb(ctx, if b.note { t.panel_alt } else { t.card });
        ctx.fill_preserve().ok();
        set_rgb(ctx, if b.note { t.border } else { t.accent });
        ctx.set_line_width(1.5);
        ctx.stroke().ok();
        set_rgb(ctx, t.text_primary);
        centered_text(ctx, &b.label, b.x + b.w / 2.0, b.y + b.h / 2.0);
    }
}

/// Draw a self-message as a small loop to the right of the lifeline.
fn draw_self_loop(
    ctx: &cairo::Context,
    m: &matforge_core::services::mermaid::SeqMessage,
    t: &matforge_core::theme::ThemeTokens,
) {
    let x = m.from.0;
    let (y1, y2) = (m.from.1, m.to.1);
    let rx = x + SELF_LOOP_W;
    ctx.move_to(x, y1);
    ctx.line_to(rx, y1);
    ctx.line_to(rx, y2);
    ctx.line_to(x + 2.0, y2);
    ctx.stroke().ok();
    ctx.set_dash(&[], 0.0);
    message_head(ctx, (rx, y2), (x, y2), m.arrow, t);
    if !m.label.is_empty() {
        set_rgb(ctx, t.text_primary);
        if let Ok(ext) = ctx.text_extents(&m.label) {
            ctx.move_to(rx + 6.0, (y1 + y2) / 2.0 + FONT / 2.0 - 3.0 - ext.y_bearing() / 2.0);
            ctx.show_text(&m.label).ok();
        }
    }
}

/// Draw the arrowhead for a message ending at `to`, per its [`Arrow`] kind.
fn message_head(
    ctx: &cairo::Context,
    from: (f64, f64),
    to: (f64, f64),
    arrow: Arrow,
    t: &matforge_core::theme::ThemeTokens,
) {
    match arrow {
        Arrow::Head => arrowhead(ctx, from, to, t.text_secondary),
        Arrow::Open => open_head(ctx, from, to, t.text_secondary),
        Arrow::Cross => cross_head(ctx, to, t.red),
        Arrow::None => {}
    }
}

/// An open `V`-shaped arrowhead (async messages).
fn open_head(ctx: &cairo::Context, from: (f64, f64), to: (f64, f64), color: Rgb) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let s = 8.0;
    set_rgb(ctx, color);
    ctx.set_line_width(1.5);
    ctx.move_to(to.0 - ux * s + px * s * 0.6, to.1 - uy * s + py * s * 0.6);
    ctx.line_to(to.0, to.1);
    ctx.line_to(to.0 - ux * s - px * s * 0.6, to.1 - uy * s - py * s * 0.6);
    ctx.stroke().ok();
}

/// A small cross at the message end (lost / rejected message).
fn cross_head(ctx: &cairo::Context, to: (f64, f64), color: Rgb) {
    let s = 4.0;
    set_rgb(ctx, color);
    ctx.set_line_width(1.6);
    ctx.move_to(to.0 - s, to.1 - s);
    ctx.line_to(to.0 + s, to.1 + s);
    ctx.move_to(to.0 + s, to.1 - s);
    ctx.line_to(to.0 - s, to.1 + s);
    ctx.stroke().ok();
}

/// Draw `label` centered on `(cx, cy)`.
fn centered_text(ctx: &cairo::Context, label: &str, cx: f64, cy: f64) {
    if let Ok(ext) = ctx.text_extents(label) {
        ctx.move_to(cx - ext.width() / 2.0 - ext.x_bearing(), cy - ext.height() / 2.0 - ext.y_bearing());
        ctx.show_text(label).ok();
    }
}

// ----- class diagrams --------------------------------------------------------

const CLASS_LINE_H: f64 = 19.0;
const CLASS_TITLE_H: f64 = 28.0;

/// A `DrawingArea` sized to `scene`, painting a class diagram on demand.
pub fn drawing_area_class(scene: ClassScene) -> DrawingArea {
    let area = DrawingArea::new();
    area.set_content_width(scene.width.ceil() as i32);
    area.set_content_height(scene.height.ceil() as i32);
    area.set_halign(gtk::Align::Start);
    area.add_css_class("mf-md-mermaid");
    area.set_draw_func(move |_a, ctx, _w, _h| draw_class(ctx, &scene));
    area
}

fn draw_class(ctx: &cairo::Context, scene: &ClassScene) {
    let t = crate::theme_css::current();

    // Relationships first, behind the boxes.
    for e in &scene.edges {
        set_rgb(ctx, t.text_secondary);
        ctx.set_line_width(1.4);
        if e.dashed {
            ctx.set_dash(&[5.0, 3.0], 0.0);
        }
        ctx.move_to(e.from.0, e.from.1);
        ctx.line_to(e.to.0, e.to.1);
        ctx.stroke().ok();
        ctx.set_dash(&[], 0.0);
        marker(ctx, e.to, e.from, e.left, &t); // marker at the `from` end
        marker(ctx, e.from, e.to, e.right, &t); // marker at the `to` end
        if let Some(label) = &e.label {
            let mid = ((e.from.0 + e.to.0) / 2.0, (e.from.1 + e.to.1) / 2.0);
            label_chip(ctx, label, mid, &t);
        }
    }

    for b in &scene.boxes {
        // Box body + border.
        rounded_rect(ctx, b.x, b.y, b.w, b.h, 4.0);
        set_rgb(ctx, t.card);
        ctx.fill_preserve().ok();
        set_rgb(ctx, t.accent);
        ctx.set_line_width(1.6);
        ctx.stroke().ok();

        // Title bar.
        set_rgb(ctx, t.panel_alt);
        ctx.rectangle(b.x + 1.0, b.y + 1.0, b.w - 2.0, CLASS_TITLE_H - 1.0);
        ctx.fill().ok();
        ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        ctx.set_font_size(FONT);
        set_rgb(ctx, t.text_primary);
        centered_text(ctx, &b.name, b.x + b.w / 2.0, b.y + CLASS_TITLE_H / 2.0);

        // Member compartments (fields then methods), left-aligned monospace.
        ctx.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        ctx.set_font_size(FONT - 1.0);
        let mut y = b.y + CLASS_TITLE_H;
        set_rgb(ctx, t.border);
        ctx.set_line_width(1.0);
        ctx.move_to(b.x, y);
        ctx.line_to(b.x + b.w, y);
        ctx.stroke().ok();
        y += 6.0;
        for (i, line) in b.fields.iter().chain(b.methods.iter()).enumerate() {
            // Divider between the field and method compartments.
            if i == b.fields.len() && !b.fields.is_empty() && !b.methods.is_empty() {
                set_rgb(ctx, t.border);
                ctx.move_to(b.x, y - 3.0);
                ctx.line_to(b.x + b.w, y - 3.0);
                ctx.stroke().ok();
            }
            set_rgb(ctx, t.text_secondary);
            ctx.move_to(b.x + 10.0, y + FONT - 4.0);
            ctx.show_text(line).ok();
            y += CLASS_LINE_H;
        }
    }
}

/// Draw a UML relationship `marker` at `tip`, oriented along `from`→`tip`.
fn marker(
    ctx: &cairo::Context,
    from: (f64, f64),
    tip: (f64, f64),
    m: Marker,
    t: &matforge_core::theme::ThemeTokens,
) {
    if m == Marker::None {
        return;
    }
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    match m {
        Marker::Arrow => open_head(ctx, from, tip, t.text_secondary),
        Marker::Triangle => {
            let s = 11.0;
            let w = 6.0;
            let base = (tip.0 - ux * s, tip.1 - uy * s);
            ctx.move_to(tip.0, tip.1);
            ctx.line_to(base.0 + px * w, base.1 + py * w);
            ctx.line_to(base.0 - px * w, base.1 - py * w);
            ctx.close_path();
            set_rgb(ctx, t.card);
            ctx.fill_preserve().ok();
            set_rgb(ctx, t.text_secondary);
            ctx.set_line_width(1.4);
            ctx.stroke().ok();
        }
        Marker::Diamond | Marker::DiamondHollow => {
            let s = 12.0;
            let w = 6.0;
            let base = (tip.0 - ux * s, tip.1 - uy * s);
            let mid = (tip.0 - ux * s / 2.0, tip.1 - uy * s / 2.0);
            ctx.move_to(tip.0, tip.1);
            ctx.line_to(mid.0 + px * w, mid.1 + py * w);
            ctx.line_to(base.0, base.1);
            ctx.line_to(mid.0 - px * w, mid.1 - py * w);
            ctx.close_path();
            if m == Marker::Diamond {
                set_rgb(ctx, t.text_secondary);
                ctx.fill().ok();
            } else {
                set_rgb(ctx, t.card);
                ctx.fill_preserve().ok();
                set_rgb(ctx, t.text_secondary);
                ctx.set_line_width(1.4);
                ctx.stroke().ok();
            }
        }
        Marker::None => {}
    }
}
