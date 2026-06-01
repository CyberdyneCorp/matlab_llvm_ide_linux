//! A code editor surface: a `GtkTextView` with live syntax highlighting and a
//! Cairo gutter (line numbers, breakpoint dots, the yellow ▶ execution marker).
//! The gutter is a real sibling to the left of the text inside the scroller, so
//! it receives clicks directly — clicking it (or pressing F9) toggles a
//! breakpoint. All state lives in the view models; this only draws from them.

use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, Box as GtkBox, DrawingArea, Orientation, ScrolledWindow, TextView};

use matforge_core::services::highlighter::Language;
use matforge_core::theme::{code, palette};

use crate::app_state::AppState;
use crate::highlight;

const GUTTER_WIDTH: i32 = 52;

/// Build the editor surface for a tab. Returns the scrollable widget.
pub fn build_code_view(
    app: &Rc<AppState>,
    tab_id: u64,
    contents: &str,
    language: Language,
) -> ScrolledWindow {
    let view = TextView::new();
    view.set_monospace(true);
    view.add_css_class("mf-code");
    view.set_left_margin(6);
    view.set_hexpand(true);
    let buffer = view.buffer();
    buffer.set_text(contents);
    highlight::ensure_tags(&buffer);
    highlight::apply(&buffer, language);
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

    // Gutter — a sibling to the left of the text, scrolled together with it.
    let gutter = DrawingArea::new();
    gutter.set_width_request(GUTTER_WIDTH);
    gutter.add_css_class("mf-gutter");
    {
        let view = view.clone();
        let app = app.clone();
        gutter.set_draw_func(move |_a, ctx, _w, h| draw_gutter(ctx, h, &view, &app, tab_id));
    }

    let hbox = GtkBox::new(Orientation::Horizontal, 0);
    hbox.append(&gutter);
    hbox.append(&view);

    let scroll = ScrolledWindow::new();
    scroll.set_child(Some(&hbox));
    scroll.set_vexpand(true);

    // Redraw the gutter on edits and when breakpoints / the exec line change.
    {
        let gutter = gutter.clone();
        buffer.connect_changed(move |_| gutter.queue_draw());
    }
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

    // Click the gutter to toggle a breakpoint at the clicked line.
    let click = gtk::GestureClick::new();
    {
        let view = view.clone();
        let app = app.clone();
        let gutter2 = gutter.clone();
        click.connect_released(move |_g, _n, _x, y| {
            // The gutter shares the text's vertical coordinate space, so y is a
            // buffer y; sample the line at a small positive content x.
            if let Some(it) = view.iter_at_location(1, y as i32) {
                app.vm.editor.toggle_breakpoint(tab_id, it.line() as usize + 1);
                app.refresh_breakpoints();
                gutter2.queue_draw();
            }
        });
    }
    gutter.add_controller(click);

    // F9 toggles a breakpoint at the cursor line.
    let keys = gtk::EventControllerKey::new();
    {
        let app = app.clone();
        let buffer = buffer.clone();
        let gutter3 = gutter.clone();
        keys.connect_key_pressed(move |_c, keyval, _code, _state| {
            if keyval == gtk::gdk::Key::F9 {
                let it = buffer.iter_at_offset(buffer.cursor_position());
                app.vm.editor.toggle_breakpoint(tab_id, it.line() as usize + 1);
                app.refresh_breakpoints();
                gutter3.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    view.add_controller(keys);

    scroll
}

fn draw_gutter(ctx: &cairo::Context, height: i32, view: &TextView, app: &Rc<AppState>, tab_id: u64) {
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
        // The gutter scrolls with the text, so buffer y == gutter draw y.
        let (y, lh) = view.line_yrange(&iter);
        if y + lh < 0 || y > height {
            continue;
        }
        let yf = y as f64;
        let center = yf + lh as f64 / 2.0;
        let one_indexed = line as usize + 1;
        let is_exec = tab.execution_line == Some(one_indexed);
        let has_bp = tab.breakpoints.contains_key(&one_indexed);

        if has_bp {
            let (r, g, b) = palette::ACCENT_RED.to_unit();
            ctx.set_source_rgb(r, g, b);
            ctx.arc(9.0, center, 4.0, 0.0, 2.0 * PI);
            let _ = ctx.fill();
        }
        if is_exec {
            let (r, g, b) = palette::ACCENT_YELLOW.to_unit();
            ctx.set_source_rgb(r, g, b);
            ctx.move_to(18.0, center - 4.0);
            ctx.line_to(26.0, center);
            ctx.line_to(18.0, center + 4.0);
            ctx.close_path();
            let _ = ctx.fill();
        }
        let (nr, ng, nb) = if is_exec {
            palette::ACCENT_YELLOW.to_unit()
        } else {
            code::LINE_NUMBER.to_unit()
        };
        ctx.set_source_rgb(nr, ng, nb);
        let label = one_indexed.to_string();
        let ext = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        ctx.move_to(46.0 - ext, yf + 11.0);
        let _ = ctx.show_text(&label);
    }
}
