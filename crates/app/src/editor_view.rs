//! A code editor surface: a `GtkTextView` with live syntax highlighting and a
//! Cairo gutter (line numbers, breakpoint dots, the yellow ▶ execution marker).
//!
//! Layout: `HBox[ gutter | ScrolledWindow[textview] ]`. The `TextView` is the
//! *direct* child of the `ScrolledWindow` (it's a `GtkScrollable`, so this is
//! required for correct sizing/scrolling). The gutter is a sibling `DrawingArea`
//! to its left — it receives clicks directly (no overlap with the text) and
//! redraws on scroll, mapping buffer↔window coordinates via the text view. All
//! state lives in the view models; this only draws from them.

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, Box as GtkBox, DrawingArea, Orientation, ScrolledWindow, TextView, TextWindowType};

use matforge_core::services::highlighter::Language;

use crate::app_state::AppState;
use crate::highlight;

const GUTTER_WIDTH: i32 = 52;

thread_local! {
    /// The code view that last had focus — the target of in-editor find / goto.
    static ACTIVE_VIEW: RefCell<Option<TextView>> = const { RefCell::new(None) };
}

/// The currently-focused code editor's `TextView`, if any.
pub fn active_view() -> Option<TextView> {
    ACTIVE_VIEW.with(|c| c.borrow().clone())
}

fn set_active_view(view: &TextView) {
    ACTIVE_VIEW.with(|c| *c.borrow_mut() = Some(view.clone()));
}

/// Build the editor surface for a tab. Returns the container widget.
pub fn build_code_view(
    app: &Rc<AppState>,
    tab_id: u64,
    contents: &str,
    language: Language,
) -> GtkBox {
    let view = TextView::new();
    view.set_monospace(true);
    view.add_css_class("mf-code");
    view.set_left_margin(8);
    let buffer = view.buffer();
    buffer.set_text(contents);
    highlight::ensure_tags(&buffer);
    highlight::apply(&buffer, language);
    // Caret decorations. `current-line` is created first (lowest priority) so the
    // debugger's `exec-line` background wins on the executing line; its colour and
    // `bracket-match`'s are refreshed per-theme on each cursor move.
    if buffer.tag_table().lookup("current-line").is_none() {
        buffer.create_tag(Some("current-line"), &[]);
    }
    if buffer.tag_table().lookup("exec-line").is_none() {
        buffer.create_tag(Some("exec-line"), &[("paragraph-background", &"#2c2a16")]);
    }
    if buffer.tag_table().lookup("bracket-match").is_none() {
        buffer.create_tag(Some("bracket-match"), &[]);
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
            update_caret_decorations(b);
        });
    }
    update_caret_decorations(&buffer);

    // TextView is the direct scrollable child of the ScrolledWindow.
    let scroll = ScrolledWindow::new();
    scroll.set_child(Some(&view));
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);

    // Gutter sits beside the scroller, filling the viewport height.
    let gutter = DrawingArea::new();
    gutter.set_width_request(GUTTER_WIDTH);
    gutter.set_vexpand(true);
    gutter.add_css_class("mf-gutter");
    {
        let view = view.clone();
        let app = app.clone();
        gutter.set_draw_func(move |_a, ctx, _w, h| draw_gutter(ctx, h, &view, &app, tab_id));
    }

    // Redraw the gutter on scroll, on edits, and when bp/exec state changes.
    {
        let gutter = gutter.clone();
        scroll.vadjustment().connect_value_changed(move |_| gutter.queue_draw());
    }
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

    // Jump to a requested line (e.g. clicking a PROBLEMS diagnostic).
    {
        let view = view.clone();
        let buffer = buffer.clone();
        app.vm.editor.goto_request.subscribe(move |req| {
            if let Some((tid, line)) = req {
                if *tid == tab_id {
                    if let Some(mut it) = buffer.iter_at_line(*line as i32 - 1) {
                        buffer.place_cursor(&it);
                        view.scroll_to_iter(&mut it, 0.2, false, 0.0, 0.0);
                        view.grab_focus();
                    }
                }
            }
        });
    }

    // Click the gutter to toggle a breakpoint at the clicked line. The gutter
    // shares the text view's vertical position, so the click y is a widget
    // window-y; convert it to a buffer y and resolve the line.
    let click = gtk::GestureClick::new();
    {
        let view = view.clone();
        let app = app.clone();
        let gutter2 = gutter.clone();
        click.connect_released(move |_g, _n, _x, y| {
            // The gutter shares the text view's vertical position; convert the
            // click y to a buffer y and resolve the line (x-independent, so a
            // click in the gutter margin still maps to the right line).
            let (_, by) = view.window_to_buffer_coords(TextWindowType::Widget, 0, y as i32);
            let (iter, _) = view.line_at_y(by);
            app.vm.editor.toggle_breakpoint(tab_id, iter.line() as usize + 1);
            app.refresh_breakpoints();
            gutter2.queue_draw();
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

    // Track the focused editor as the find / go-to-line target.
    set_active_view(&view);
    {
        let focus = gtk::EventControllerFocus::new();
        let view2 = view.clone();
        focus.connect_enter(move |_| set_active_view(&view2));
        view.add_controller(focus);
    }

    crate::e2e::set_active_gutter(&gutter);

    let hbox = GtkBox::new(Orientation::Horizontal, 0);
    hbox.append(&gutter);
    hbox.append(&scroll);

    // Markdown files get an Edit / Split / Preview sub-view with a live preview.
    if language == Language::Markdown {
        return build_markdown_container(&buffer, hbox);
    }
    hbox
}

/// Wrap a Markdown editor in a header (Edit · Split · Preview toggle) plus a
/// `Paned` whose right side is a live Pango-rendered preview of the buffer.
fn build_markdown_container(buffer: &gtk::TextBuffer, editor: GtkBox) -> GtkBox {
    // The preview is a column of block widgets (prose labels, code cards,
    // mermaid diagrams) rebuilt from scratch on every edit.
    let preview = GtkBox::new(Orientation::Vertical, 10);
    preview.set_margin_top(8);
    preview.set_margin_bottom(8);
    preview.set_margin_start(12);
    preview.set_margin_end(12);
    preview.add_css_class("mf-md-preview");

    let preview_scroll = ScrolledWindow::new();
    preview_scroll.set_child(Some(&preview));
    preview_scroll.set_hexpand(true);
    preview_scroll.set_vexpand(true);

    let render = {
        let preview = preview.clone();
        move |b: &gtk::TextBuffer| {
            let (s, e) = b.bounds();
            let text = b.text(&s, &e, false);
            render_blocks(&preview, &text);
        }
    };
    render(buffer);
    {
        let render = render.clone();
        buffer.connect_changed(move |b| render(b));
    }

    let paned = gtk::Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&editor));
    paned.set_end_child(Some(&preview_scroll));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    paned.set_wide_handle(true);
    paned.set_vexpand(true);
    paned.set_hexpand(true);

    // Header: a linked Edit / Split / Preview toggle.
    let edit_btn = gtk::ToggleButton::with_label("Edit");
    let split_btn = gtk::ToggleButton::with_label("Split");
    let preview_btn = gtk::ToggleButton::with_label("Preview");
    split_btn.set_group(Some(&edit_btn));
    preview_btn.set_group(Some(&edit_btn));
    split_btn.set_active(true);

    let apply_mode = {
        let editor = editor.clone();
        let preview_scroll = preview_scroll.clone();
        move |show_editor: bool, show_preview: bool| {
            editor.set_visible(show_editor);
            preview_scroll.set_visible(show_preview);
        }
    };
    {
        let apply = apply_mode.clone();
        edit_btn.connect_toggled(move |b| {
            if b.is_active() {
                apply(true, false);
            }
        });
    }
    {
        let apply = apply_mode.clone();
        split_btn.connect_toggled(move |b| {
            if b.is_active() {
                apply(true, true);
            }
        });
    }
    {
        let apply = apply_mode.clone();
        preview_btn.connect_toggled(move |b| {
            if b.is_active() {
                apply(false, true);
            }
        });
    }

    let toggles = GtkBox::new(Orientation::Horizontal, 0);
    toggles.add_css_class("linked");
    toggles.append(&edit_btn);
    toggles.append(&split_btn);
    toggles.append(&preview_btn);

    let header = GtkBox::new(Orientation::Horizontal, 0);
    header.add_css_class("mf-md-toolbar");
    header.set_halign(gtk::Align::End);
    header.set_margin_top(4);
    header.set_margin_bottom(4);
    header.set_margin_start(8);
    header.set_margin_end(8);
    header.append(&toggles);

    let container = GtkBox::new(Orientation::Vertical, 0);
    container.append(&header);
    container.append(&paned);
    container
}

/// Rebuild `container`'s children from the Markdown blocks in `text`.
fn render_blocks(container: &GtkBox, text: &str) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    use matforge_core::services::markdown::Block;
    for block in matforge_core::services::markdown::parse(text) {
        match block {
            Block::Markup(markup) => {
                let label = gtk::Label::new(None);
                label.set_markup(&markup);
                label.set_use_markup(true);
                label.set_wrap(true);
                label.set_xalign(0.0);
                label.set_selectable(true);
                container.append(&label);
            }
            Block::Code { lang, body } => container.append(&code_card(&lang, &body)),
            Block::Mermaid(src) => container.append(&mermaid_block(&src)),
        }
    }
}

/// A syntax-highlighted code card with an optional language tag in its header.
fn code_card(lang: &str, body: &str) -> GtkBox {
    let card = GtkBox::new(Orientation::Vertical, 0);
    card.add_css_class("mf-md-code");
    card.set_hexpand(false);
    card.set_halign(gtk::Align::Start);

    if !lang.is_empty() {
        let tag = gtk::Label::new(Some(lang));
        tag.add_css_class("mf-md-code-lang");
        tag.set_xalign(0.0);
        card.append(&tag);
    }

    let code = gtk::Label::new(None);
    code.set_markup(&highlight_to_markup(body, lang));
    code.set_use_markup(true);
    code.set_xalign(0.0);
    code.set_selectable(true);
    code.add_css_class("mf-code");
    code.set_margin_top(6);
    code.set_margin_bottom(6);
    code.set_margin_start(10);
    code.set_margin_end(10);
    card.append(&code);
    card
}

/// A mermaid `DrawingArea` if the source parses, else a plain code-card fallback.
fn mermaid_block(src: &str) -> GtkBox {
    if let Some(graph) = matforge_core::services::mermaid::parse(src) {
        let scene = matforge_core::services::mermaid::layout(&graph);
        let area = crate::mermaid_render::drawing_area(scene);
        let wrap = GtkBox::new(Orientation::Vertical, 0);
        wrap.add_css_class("mf-md-mermaid-wrap");
        wrap.set_halign(gtk::Align::Start);
        wrap.append(&area);
        wrap
    } else {
        // Unsupported diagram type — show the source so nothing is lost.
        code_card("mermaid", src)
    }
}

/// Syntax-highlight `body` (per the fence's `lang`) into a Pango-markup string.
fn highlight_to_markup(body: &str, lang: &str) -> String {
    let language = Language::from_label(lang);
    let chars: Vec<char> = body.chars().collect();
    if language == Language::Plain {
        return pango_escape(body);
    }
    let tokens = matforge_core::services::highlighter::highlight(body, language);
    let mut out = String::new();
    let mut cursor = 0usize;
    let push_slice = |out: &mut String, slice: &[char]| {
        out.push_str(&pango_escape(&slice.iter().collect::<String>()));
    };
    for span in tokens {
        let (s, e) = (span.start.min(chars.len()), span.end.min(chars.len()));
        if s < cursor || s > e {
            continue;
        }
        push_slice(&mut out, &chars[cursor..s]);
        let color = token_color(span.color);
        out.push_str(&format!("<span foreground=\"{color}\">"));
        push_slice(&mut out, &chars[s..e]);
        out.push_str("</span>");
        cursor = e;
    }
    push_slice(&mut out, &chars[cursor..]);
    out
}

/// Map a highlighter `TokenColor` to a theme-aware hex color.
fn token_color(color: matforge_core::services::highlighter::TokenColor) -> String {
    use matforge_core::services::highlighter::TokenColor as T;
    let t = crate::theme_css::current();
    let rgb = match color {
        T::Keyword => t.syn_keyword,
        T::Control => t.syn_control,
        T::Number => t.syn_number,
        T::Str => t.syn_string,
        T::Comment => t.syn_comment,
        T::Function => t.syn_function,
        T::Identifier => t.syn_identifier,
        T::Operator => t.syn_operator,
        T::SsaGlobal => t.orange,
        T::SsaLocal => t.blue,
        T::Plain => t.syn_plain,
    };
    rgb.to_css()
}

/// Escape the Pango/XML-special characters in `s`.
fn pango_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Refresh the caret decorations on cursor moves: a subtle current-line
/// background (theme `hover`) plus a `bracket-match` accent on the bracket pair
/// when the caret sits next to one. Both tags are re-coloured for the active
/// theme each call, so they follow theme switches without rebuilding the buffer.
fn update_caret_decorations(buffer: &gtk::TextBuffer) {
    let tokens = crate::theme_css::current();
    let table = buffer.tag_table();
    let (bounds_start, bounds_end) = buffer.bounds();

    // Current line: tint the caret's paragraph. `paragraph-background` colours
    // the whole line regardless of how much of it the tag covers.
    if let Some(cl) = table.lookup("current-line") {
        cl.set_property("paragraph-background", tokens.hover.to_css());
        buffer.remove_tag(&cl, &bounds_start, &bounds_end);
        let cursor = buffer.iter_at_offset(buffer.cursor_position());
        if let Some(ls) = buffer.iter_at_line(cursor.line()) {
            let mut le = ls;
            if !le.ends_line() {
                le.forward_to_line_end();
            }
            buffer.apply_tag(&cl, &ls, &le);
        }
    }

    // Bracket match: highlight the pair when the caret abuts a bracket.
    if let Some(bm) = table.lookup("bracket-match") {
        bm.set_property("foreground", tokens.accent.to_css());
        bm.set_property("weight", 700i32);
        buffer.remove_tag(&bm, &bounds_start, &bounds_end);
        let text: Vec<char> = buffer
            .text(&bounds_start, &bounds_end, false)
            .chars()
            .collect();
        let pos = buffer.cursor_position() as usize;
        // Prefer the bracket just before the caret, then the one just after.
        for probe in [pos.checked_sub(1), Some(pos)].into_iter().flatten() {
            if let Some(m) = matforge_core::services::brackets::matching_bracket(&text, probe) {
                highlight_char(buffer, &bm, probe);
                highlight_char(buffer, &bm, m);
                break;
            }
        }
    }
}

/// Apply `tag` to the single character at `offset`.
fn highlight_char(buffer: &gtk::TextBuffer, tag: &gtk::TextTag, offset: usize) {
    let start = buffer.iter_at_offset(offset as i32);
    let mut end = start;
    end.forward_char();
    buffer.apply_tag(tag, &start, &end);
}

fn draw_gutter(ctx: &cairo::Context, height: i32, view: &TextView, app: &Rc<AppState>, tab_id: u64) {
    let (br, bg, bb) = crate::theme_css::current().editor_bg.to_unit();
    ctx.set_source_rgb(br, bg, bb);
    let _ = ctx.paint();
    ctx.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(11.0 * crate::theme_css::code_scale());

    let buffer = view.buffer();
    let tab = app.vm.editor.tabs.with(|tabs| tabs.iter().find(|t| t.id == tab_id).cloned());
    let Some(tab) = tab else { return };
    let line_count = buffer.line_count();

    for line in 0..line_count {
        let Some(iter) = buffer.iter_at_line(line) else { continue };
        let (by, lh) = view.line_yrange(&iter);
        // Map the buffer y to an on-screen (widget window) y, accounting for scroll.
        let (_, wy) = view.buffer_to_window_coords(TextWindowType::Widget, 0, by);
        if wy + lh < 0 || wy > height {
            continue; // off-screen
        }
        let yf = wy as f64;
        let center = yf + lh as f64 / 2.0;
        let one_indexed = line as usize + 1;
        let is_exec = tab.execution_line == Some(one_indexed);
        let has_bp = tab.breakpoints.contains_key(&one_indexed);

        if has_bp {
            let (r, g, b) = crate::theme_css::current().red.to_unit();
            ctx.set_source_rgb(r, g, b);
            ctx.arc(9.0, center, 4.0, 0.0, 2.0 * PI);
            let _ = ctx.fill();
        }
        if is_exec {
            let (r, g, b) = crate::theme_css::current().yellow.to_unit();
            ctx.set_source_rgb(r, g, b);
            ctx.move_to(18.0, center - 4.0);
            ctx.line_to(26.0, center);
            ctx.line_to(18.0, center + 4.0);
            ctx.close_path();
            let _ = ctx.fill();
        }
        let (nr, ng, nb) = if is_exec {
            crate::theme_css::current().yellow.to_unit()
        } else {
            crate::theme_css::current().syn_line_number.to_unit()
        };
        ctx.set_source_rgb(nr, ng, nb);
        let label = one_indexed.to_string();
        let ext = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        ctx.move_to(46.0 - ext, yf + 11.0);
        let _ = ctx.show_text(&label);
    }
}
