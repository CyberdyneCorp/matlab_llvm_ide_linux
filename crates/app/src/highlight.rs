//! Bridges the core [`highlighter`] token spans onto a `GtkTextBuffer` by
//! creating one `GtkTextTag` per [`TokenColor`] and applying them over the
//! computed ranges. All classification lives in core; this is pure GTK glue.

use gtk::prelude::*;
use gtk::TextBuffer;
use matforge_core::services::highlighter::{self, Language, TokenColor};

/// Create the per-color tags on the buffer's tag table (idempotent).
pub fn ensure_tags(buffer: &TextBuffer) {
    let table = buffer.tag_table();
    for color in TokenColor::ALL {
        if table.lookup(color.tag_name()).is_none() {
            let css = color.rgb().to_css();
            buffer.create_tag(Some(color.tag_name()), &[("foreground", &css)]);
        }
    }
}

/// Re-highlight the entire buffer for `language`.
pub fn apply(buffer: &TextBuffer, language: Language) {
    ensure_tags(buffer);
    let (start, end) = buffer.bounds();
    let text = buffer.text(&start, &end, false).to_string();

    for color in TokenColor::ALL {
        if let Some(tag) = buffer.tag_table().lookup(color.tag_name()) {
            buffer.remove_tag(&tag, &start, &end);
        }
    }
    for span in highlighter::highlight(&text, language) {
        let s = buffer.iter_at_offset(span.start as i32);
        let e = buffer.iter_at_offset(span.end as i32);
        buffer.apply_tag_by_name(span.color.tag_name(), &s, &e);
    }
}
