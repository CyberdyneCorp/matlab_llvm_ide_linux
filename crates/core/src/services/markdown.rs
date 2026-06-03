//! A small Markdown → Pango-markup renderer for the editor's Markdown preview
//! pane. It covers the constructs that show up in this project's docs (headings,
//! emphasis, inline + fenced code, lists, blockquotes, links, rules, and pipe
//! tables) and produces a Pango markup string a `GtkLabel` can render. It is not
//! a CommonMark implementation — just enough to make a README read nicely.
//!
//! Pure and tested; the GTK side only feeds it buffer text and shows the result.

/// Render `src` (Markdown) to a Pango-markup string.
pub fn to_pango_markup(src: &str) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    let mut first_block = true;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // Fenced code block: ``` ... ``` (language tag, if any, is ignored).
        if let Some(fence) = fence_marker(trimmed) {
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(fence) {
                body.push(lines[i]);
                i += 1;
            }
            i += 1; // consume closing fence (or run off the end)
            block_gap(&mut out, &mut first_block);
            out.push_str("<tt>");
            out.push_str(&escape(&body.join("\n")));
            out.push_str("</tt>");
            continue;
        }

        // Blank line: paragraph separator (handled by block_gap on the next block).
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Horizontal rule.
        if is_hr(trimmed) {
            block_gap(&mut out, &mut first_block);
            out.push_str("<span foreground=\"#666666\">────────────────────</span>");
            i += 1;
            continue;
        }

        // ATX heading: #..###### .
        if let Some((level, text)) = heading(trimmed) {
            block_gap(&mut out, &mut first_block);
            let size = match level {
                1 => "x-large",
                2 => "large",
                3 => "medium",
                _ => "medium",
            };
            out.push_str(&format!(
                "<span size=\"{size}\" weight=\"bold\">{}</span>",
                inline(text)
            ));
            i += 1;
            continue;
        }

        // Blockquote: one or more leading '>' lines.
        if trimmed.starts_with('>') {
            block_gap(&mut out, &mut first_block);
            let mut quote = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let q = lines[i].trim_start().trim_start_matches('>').trim_start();
                quote.push(inline(q));
                i += 1;
            }
            out.push_str(&format!(
                "<span foreground=\"#888888\"><i>{}</i></span>",
                quote.join("\n")
            ));
            continue;
        }

        // Pipe table: a run of lines that all contain '|'. The separator row
        // (---|---) is dropped; the rest are shown monospaced and aligned.
        if trimmed.contains('|') && next_is_table_sep(&lines, i) {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                rows.push(lines[i]);
                i += 1;
            }
            block_gap(&mut out, &mut first_block);
            out.push_str(&render_table(&rows));
            continue;
        }

        // List: a run of -, *, +, or "N." items.
        if list_marker(trimmed).is_some() {
            block_gap(&mut out, &mut first_block);
            let mut items = Vec::new();
            while i < lines.len() {
                let lt = lines[i].trim_start();
                match list_marker(lt) {
                    Some((bullet, text)) => {
                        items.push(format!("{bullet}  {}", inline(text)));
                        i += 1;
                    }
                    None => break,
                }
            }
            out.push_str(&items.join("\n"));
            continue;
        }

        // Paragraph: gather consecutive non-blank, non-special lines.
        block_gap(&mut out, &mut first_block);
        let mut para = Vec::new();
        while i < lines.len() {
            let lt = lines[i].trim_start();
            if lt.is_empty()
                || heading(lt).is_some()
                || lt.starts_with('>')
                || is_hr(lt)
                || fence_marker(lt).is_some()
                || list_marker(lt).is_some()
            {
                break;
            }
            para.push(inline(lt));
            i += 1;
        }
        out.push_str(&para.join(" "));
    }

    out
}

/// Blank-line block separator (two newlines between blocks; none before the first).
fn block_gap(out: &mut String, first_block: &mut bool) {
    if *first_block {
        *first_block = false;
    } else {
        out.push_str("\n\n");
    }
}

fn fence_marker(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn is_hr(trimmed: &str) -> bool {
    let t = trimmed.replace(' ', "");
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

fn heading(trimmed: &str) -> Option<(usize, &str)> {
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if rest.starts_with(' ') || rest.is_empty() {
        Some((level, rest.trim()))
    } else {
        None
    }
}

/// If `trimmed` is a list item, return its rendered bullet and the item text.
fn list_marker(trimmed: &str) -> Option<(String, &str)> {
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(m) {
            return Some(("•".to_string(), rest));
        }
    }
    // Ordered: "<digits>. text"
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let after = &trimmed[digits.len()..];
        if let Some(rest) = after.strip_prefix(". ") {
            return Some((format!("{digits}."), rest));
        }
    }
    None
}

/// True when row `i` is a table header — i.e. the following row is a `---|---`
/// separator. This distinguishes real tables from prose that happens to use '|'.
fn next_is_table_sep(lines: &[&str], i: usize) -> bool {
    lines.get(i + 1).is_some_and(|l| {
        let t = l.trim();
        t.contains('|')
            && t.chars()
                .all(|c| matches!(c, '|' | '-' | ':' | ' '))
            && t.contains('-')
    })
}

/// Render pipe-table `rows` (including the header and `---` separator) as an
/// aligned, monospaced block with a bold header.
fn render_table(rows: &[&str]) -> String {
    let split = |row: &str| -> Vec<String> {
        row.trim()
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect()
    };
    // Skip the separator row (index 1) when laying out cells.
    let cell_rows: Vec<Vec<String>> = rows
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != 1)
        .map(|(_, r)| split(r))
        .collect();
    let cols = cell_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for r in &cell_rows {
        for (c, cell) in r.iter().enumerate() {
            widths[c] = widths[c].max(cell.chars().count());
        }
    }
    let mut out = String::from("<tt>");
    for (ri, r) in cell_rows.iter().enumerate() {
        let mut line = String::new();
        for c in 0..cols {
            let cell = r.get(c).map(String::as_str).unwrap_or("");
            let pad = widths[c].saturating_sub(cell.chars().count());
            line.push_str(cell);
            line.push_str(&" ".repeat(pad));
            if c + 1 < cols {
                line.push_str("  │  ");
            }
        }
        let escaped = escape(line.trim_end());
        if ri == 0 {
            out.push_str(&format!("<b>{escaped}</b>\n"));
        } else {
            out.push_str(&escaped);
            out.push('\n');
        }
    }
    let trimmed_end = out.trim_end().to_string();
    format!("{trimmed_end}</tt>")
}

/// Apply inline spans (code, bold, italic, links) to one line of text, escaping
/// any Pango/markup-special characters that aren't part of a span.
fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Inline code: `...`
        if c == '`' {
            if let Some(end) = find(&chars, i + 1, '`') {
                let code: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("<tt>{}</tt>", escape(&code)));
                i = end + 1;
                continue;
            }
        }
        // Bold: **...** or __...__
        if (c == '*' || c == '_') && i + 1 < chars.len() && chars[i + 1] == c {
            if let Some(end) = find_run(&chars, i + 2, c, 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                out.push_str(&format!("<b>{}</b>", inline(&inner)));
                i = end + 2;
                continue;
            }
        }
        // Italic: *...* or _..._
        if c == '*' || c == '_' {
            if let Some(end) = find(&chars, i + 1, c) {
                let inner: String = chars[i + 1..end].iter().collect();
                if !inner.is_empty() {
                    out.push_str(&format!("<i>{}</i>", inline(&inner)));
                    i = end + 1;
                    continue;
                }
            }
        }
        // Link: [text](url)
        if c == '[' {
            if let Some((label, url, next)) = parse_link(&chars, i) {
                out.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    escape(&url),
                    inline(&label)
                ));
                i = next;
                continue;
            }
        }
        out.push_str(&escape_char(c));
        i += 1;
    }
    out
}

/// Parse `[label](url)` starting at `open` ('['). Returns (label, url, next index).
fn parse_link(chars: &[char], open: usize) -> Option<(String, String, usize)> {
    let close = find(chars, open + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let paren = find(chars, close + 2, ')')?;
    let label: String = chars[open + 1..close].iter().collect();
    let url: String = chars[close + 2..paren].iter().collect();
    Some((label, url, paren + 1))
}

fn find(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == target)
}

/// Find a run of `n` consecutive `target` chars starting at or after `from`,
/// returning the index of the first char of the run.
fn find_run(chars: &[char], from: usize, target: char, n: usize) -> Option<usize> {
    let mut j = from;
    while j + n <= chars.len() {
        if (0..n).all(|k| chars[j + k] == target) {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Escape the three Pango/XML-special characters in a whole string.
fn escape(s: &str) -> String {
    s.chars().map(escape_char).collect()
}

fn escape_char(c: char) -> String {
    // Quotes are escaped too so the strings are safe inside attribute values
    // (e.g. a link `href="..."`); escaping them in text content is harmless.
    match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        '\'' => "&#39;".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_scale_and_bold() {
        let md = to_pango_markup("# Title");
        assert!(md.contains("weight=\"bold\""));
        assert!(md.contains("x-large"));
        assert!(md.contains("Title"));
    }

    #[test]
    fn emphasis_and_inline_code() {
        let md = to_pango_markup("a **bold** and *italic* and `code` here");
        assert!(md.contains("<b>bold</b>"));
        assert!(md.contains("<i>italic</i>"));
        assert!(md.contains("<tt>code</tt>"));
    }

    #[test]
    fn fenced_code_is_escaped_and_monospaced() {
        let md = to_pango_markup("```\nif a < b && c > d\n```");
        assert!(md.contains("<tt>"));
        assert!(md.contains("a &lt; b &amp;&amp; c &gt; d"));
        // No inline emphasis parsing inside a code fence.
        assert!(!md.contains("<i>"));
    }

    #[test]
    fn links_render_as_anchors() {
        let md = to_pango_markup("see [the docs](http://x/y) now");
        assert!(md.contains("<a href=\"http://x/y\">the docs</a>"));
    }

    #[test]
    fn bullet_and_ordered_lists() {
        let md = to_pango_markup("- one\n- two");
        assert!(md.contains("•  one"));
        assert!(md.contains("•  two"));
        let ol = to_pango_markup("1. first\n2. second");
        assert!(ol.contains("1.  first"));
        assert!(ol.contains("2.  second"));
    }

    #[test]
    fn pipe_table_has_bold_header_and_drops_separator() {
        let md = to_pango_markup("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(md.contains("<tt>"));
        assert!(md.contains("<b>"));
        assert!(md.contains('A') && md.contains('B'));
        assert!(md.contains('1') && md.contains('2'));
        // The --- separator row is not rendered literally.
        assert!(!md.contains("---"));
    }

    #[test]
    fn blockquote_is_italic() {
        let md = to_pango_markup("> quoted line");
        assert!(md.contains("<i>quoted line</i>"));
    }

    #[test]
    fn raw_special_chars_outside_spans_are_escaped() {
        let md = to_pango_markup("x < y & z > w");
        assert!(md.contains("x &lt; y &amp; z &gt; w"));
    }

    #[test]
    fn horizontal_rule() {
        let md = to_pango_markup("above\n\n---\n\nbelow");
        assert!(md.contains("above"));
        assert!(md.contains("below"));
        assert!(md.contains('─'));
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(to_pango_markup(""), "");
    }
}
