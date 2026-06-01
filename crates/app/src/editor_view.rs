//! A code editor surface: a `GtkTextView` with live syntax highlighting plus a
//! Cairo gutter drawn over the left margin showing line numbers, breakpoint
//! dots, and the yellow ▶ execution-line marker. Clicking the gutter toggles a
//! breakpoint. All state lives in the view models; this draws from them.

use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, DrawingArea, Overlay, ScrolledWindow, TextView, TextWindowType};

use matforge_core::services::highlighter::Language;
use matforge_core::theme::{code, palette};

use crate::app_state::AppState;
use crate::highlight;

const GUTTER_WIDTH: i32 = 52;

/// Build the editor surface for a tab. Returns the scrollable overlay widget.
pub fn build_code_view(
    app: &Rc<AppState>,
    tab_id: u64,
    contents: &str,
    language: Language,
) -> Overlay {
    let view = TextView::new();
    view.set_monospace(true);
    view.add_css_class("mf-code");
    view.set_left_margin(GUTTER_WIDTH + 6);
    view.set_pixels_above_lines(1);
    let buffer = view.buffer();
    buffer.set_text(contents);
    highlight::ensure_tags(&buffer);
    highlight::apply(&buffer, language);
    // Tag used to shade the paused execution line.
    if buffer.tag_table().lookup("exec-line").is_none() {
        buffer.create_tag(Some("exec-line"), &[("paragraph-background", &"#2c2a16")]);
    }

    // Edits: re-highlight + sync content/dirty.
    {
        let app = app.clone();
        buffer.connect_changed(move |b| {
            highlight::apply(b, language);
            let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
            app.vm.editor.update_contents(tab_id, text);
        });
    }
    // Cursor → status bar.
    {
        let app = app.clone();
        buffer.connect_cursor_position_notify(move |b| {
            let it = b.iter_at_offset(b.cursor_position());
            app.vm.status_bar.set_cursor(it.line() as usize + 1, it.line_offset() as usize + 1);
        });
    }

    let scroll = ScrolledWindow::new();
    scroll.set_child(Some(&view));
    scroll.set_vexpand(true);

    // Gutter overlaid on the left margin.
    let gutter = DrawingArea::new();
    gutter.set_width_request(GUTTER_WIDTH);
    gutter.set_halign(gtk::Align::Start);
    gutter.add_css_class("mf-gutter");
    {
        let view = view.clone();
        let app = app.clone();
        gutter.set_draw_func(move |_area, ctx, _w, h| draw_gutter(ctx, h, &view, &app, tab_id));
    }

    // Redraw the gutter on scroll and on edits.
    {
        let gutter = gutter.clone();
        scroll.vadjustment().connect_value_changed(move |_| gutter.queue_draw());
    }
    {
        let gutter = gutter.clone();
        buffer.connect_changed(move |_| gutter.queue_draw());
    }
    // Redraw + re-shade when breakpoints / execution line change.
    {
        let gutter = gutter.clone();
        let view = view.clone();
        let buffer = buffer.clone();
        app.vm.editor.tabs.subscribe(move |tabs| {
            gutter.queue_draw();
            let (start, end) = buffer.bounds();
            buffer.remove_tag_by_name("exec-line", &start, &end);
            if let Some(tab) = tabs.iter().find(|t| t.id == tab_id) {
                if let Some(line) = tab.execution_line {
                    if let Some(mut it) = buffer.iter_at_line(line as i32 - 1) {
                        let mut eol = it;
                        if !eol.ends_line() {
                            eol.forward_to_line_end();
                        }
                        buffer.apply_tag_by_name("exec-line", &it, &eol);
                        view.scroll_to_iter(&mut it, 0.1, false, 0.0, 0.0);
                    }
                }
            }
        });
    }

    // Click the gutter to toggle a breakpoint.
    let click = gtk::GestureClick::new();
    {
        let view = view.clone();
        let app = app.clone();
        let gutter2 = gutter.clone();
        click.connect_released(move |_g, _n, x, y| {
            let (bx, by) = view.window_to_buffer_coords(TextWindowType::Widget, x as i32, y as i32);
            if let Some(it) = view.iter_at_location(bx, by) {
                app.vm.editor.toggle_breakpoint(tab_id, it.line() as usize + 1);
                app.refresh_breakpoints();
                gutter2.queue_draw();
            }
        });
    }
    gutter.add_controller(click);

    let overlay = Overlay::new();
    overlay.set_child(Some(&scroll));
    overlay.add_overlay(&gutter);
    overlay
}

fn draw_gutter(ctx: &cairo::Context, height: i32, view: &TextView, app: &Rc<AppState>, tab_id: u64) {
    // Background.
    let (br, bg, bb) = code::BACKGROUND.to_unit();
    ctx.set_source_rgb(br, bg, bb);
    let _ = ctx.paint();
    ctx.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(11.0);

    let buffer = view.buffer();
    let tab = app.vm.editor.tabs.with(|tabs| tabs.iter().find(|t| t.id == tab_id).cloned());
    let Some(tab) = tab else { return };
    let line_count = buffer.line_count();

    for line in 0..line_count {
        let Some(iter) = buffer.iter_at_line(line) else { continue };
        let (y, lh) = view.line_yrange(&iter);
        let (_, wy) = view.buffer_to_window_coords(TextWindowType::Widget, 0, y);
        if wy + lh < 0 || wy > height {
            continue;
        }
        let baseline = wy as f64 + 11.0;
        let one_indexed = line as usize + 1;
        let is_exec = tab.execution_line == Some(one_indexed);
        let has_bp = tab.breakpoints.contains_key(&one_indexed);

        // Breakpoint dot.
        if has_bp {
            let (r, g, b) = palette::ACCENT_RED.to_unit();
            ctx.set_source_rgb(r, g, b);
            ctx.arc(9.0, wy as f64 + lh as f64 / 2.0, 4.0, 0.0, 2.0 * PI);
            let _ = ctx.fill();
        }
        // Execution arrow.
        if is_exec {
            let (r, g, b) = palette::ACCENT_YELLOW.to_unit();
            ctx.set_source_rgb(r, g, b);
            let cy = wy as f64 + lh as f64 / 2.0;
            ctx.move_to(18.0, cy - 4.0);
            ctx.line_to(26.0, cy);
            ctx.line_to(18.0, cy + 4.0);
            ctx.close_path();
            let _ = ctx.fill();
        }
        // Line number (right-aligned at x≈46).
        let (nr, ng, nb) = if is_exec {
            palette::ACCENT_YELLOW.to_unit()
        } else {
            code::LINE_NUMBER.to_unit()
        };
        ctx.set_source_rgb(nr, ng, nb);
        let label = one_indexed.to_string();
        let ext = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        ctx.move_to(46.0 - ext, baseline);
        let _ = ctx.show_text(&label);
    }
}
