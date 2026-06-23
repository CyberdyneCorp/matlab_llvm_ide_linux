//! Incremental parser for `matlabc -simulate` output. The simulator streams a
//! CSV trace — a header row naming the time column plus one column per logged
//! signal, then one row of floating-point samples per solver step:
//!
//! ```text
//! t,src,scope
//! 0.000000000e+00,0.000000000e+00,0.000000000e+00
//! 1.000000000e-02,6.279051946e-02,6.260230918e-04
//! ```
//!
//! [`SimTrace`] consumes this line-by-line so a live mflowLink scope can plot
//! samples as they arrive. Non-CSV lines (model banners, diagnostics) are
//! ignored.

/// A growing simulation trace: column names (`[0]` is the time column) and the
/// sample rows collected so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimTrace {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<f64>>,
}

impl SimTrace {
    pub fn new() -> SimTrace {
        SimTrace::default()
    }

    /// Feed one output line. Returns `true` when it added a sample row (so a
    /// view can redraw only on real progress). The first CSV-shaped,
    /// non-numeric line is taken as the header.
    pub fn feed_line(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() || !line.contains(',') {
            return false;
        }
        let fields: Vec<String> = split_csv(line);
        let all_numeric = fields.iter().all(|f| f.parse::<f64>().is_ok());

        if self.columns.is_empty() {
            if all_numeric {
                // Data before a header — synthesize generic column names.
                self.columns = std::iter::once("t".to_string())
                    .chain((1..fields.len()).map(|i| format!("y{i}")))
                    .collect();
                return self.push_row(&fields);
            }
            self.columns = fields;
            return false;
        }

        if all_numeric && fields.len() == self.columns.len() {
            self.push_row(&fields)
        } else {
            false
        }
    }

    fn push_row(&mut self, fields: &[String]) -> bool {
        let row: Vec<f64> = fields
            .iter()
            .map(|f| f.parse::<f64>().unwrap_or(f64::NAN))
            .collect();
        self.rows.push(row);
        true
    }

    /// Number of logged signals (columns excluding the time column).
    pub fn signal_count(&self) -> usize {
        self.columns.len().saturating_sub(1)
    }

    /// Name of signal `i` (0-based among signals, i.e. column `i + 1`).
    pub fn signal_name(&self, i: usize) -> Option<&str> {
        self.columns.get(i + 1).map(String::as_str)
    }

    /// The time column (column 0) across all rows.
    pub fn time(&self) -> Vec<f64> {
        self.rows
            .iter()
            .map(|r| r.first().copied().unwrap_or(f64::NAN))
            .collect()
    }

    /// `(time, values)` for signal `i` (0-based among signals).
    pub fn series(&self, i: usize) -> (Vec<f64>, Vec<f64>) {
        let col = i + 1;
        let xs = self.time();
        let ys = self
            .rows
            .iter()
            .map(|r| r.get(col).copied().unwrap_or(f64::NAN))
            .collect();
        (xs, ys)
    }

    /// Latest sampled time, if any.
    pub fn last_time(&self) -> Option<f64> {
        self.rows.last().and_then(|r| r.first().copied())
    }

    /// The collected trace as CSV — the header row then one row per sample.
    /// Mirrors the simulator's column layout, so the export re-imports cleanly
    /// through [`feed_line`](Self::feed_line).
    pub fn to_csv(&self) -> String {
        self.to_csv_window(None)
    }

    /// CSV restricted to rows whose time (column 0) lies within `window`
    /// (inclusive) — i.e. the *visible* trace when a scope is zoomed. `None`
    /// exports the whole trace.
    pub fn to_csv_window(&self, window: Option<(f64, f64)>) -> String {
        let mut out = String::new();
        out.push_str(&self.columns.join(","));
        out.push('\n');
        for row in &self.rows {
            if let Some((x0, x1)) = window {
                let t = row.first().copied().unwrap_or(f64::NAN);
                if !(x0..=x1).contains(&t) {
                    continue;
                }
            }
            for (i, v) in row.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("{v}"));
            }
            out.push('\n');
        }
        out
    }
}

/// Split a CSV line into fields, handling both encodings the simulator has used
/// for image-pixel column names like `sBox[1,1]` (whose subscript carries a
/// comma). RFC 4180 quoting (current) wraps such a cell — `t,"sBox[1,1]"` — so
/// its commas are literal and `""` is an escaped `"`; the legacy bare form left
/// the comma unquoted — `t,sBox[1,1]` — where commas inside `[...]` are not
/// separators. Data rows carry neither quotes nor brackets, so they split on
/// plain commas.
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32; // `[...]` nesting, for the legacy bare form
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(ch);
            }
            continue;
        }
        match ch {
            '"' if cur.trim().is_empty() => {
                in_quotes = true;
                cur.clear();
            }
            '[' => {
                depth += 1;
                cur.push(ch);
            }
            ']' => {
                depth = (depth - 1).max(0);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                fields.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(ch),
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_then_rows() {
        let mut t = SimTrace::new();
        assert!(!t.feed_line("t,src,scope"));
        assert_eq!(t.columns, ["t", "src", "scope"]);
        assert!(t.feed_line("0.0,1.0,2.0"));
        assert!(t.feed_line("0.1,3.0,4.0"));
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.signal_count(), 2);
        assert_eq!(t.signal_name(0), Some("src"));
        assert_eq!(t.signal_name(1), Some("scope"));
    }

    #[test]
    fn image_pixel_columns_with_commas_parse_as_single_signals() {
        // Image blocks log pixels as `base[i,j]` columns whose subscript carries
        // a comma; a naive comma split would shatter the header and reject every
        // data row (0 samples). Regression for the empty image-model scope. The
        // legacy *bare* (unquoted) header form.
        let mut t = SimTrace::new();
        assert!(!t.feed_line("t,sBox[1,1],sBox[1,2],sThr[2,1]"));
        assert_eq!(t.columns, ["t", "sBox[1,1]", "sBox[1,2]", "sThr[2,1]"]);
        assert_eq!(t.signal_count(), 3);
        assert!(t.feed_line("0.0,5.0,6.0,1.0"));
        assert_eq!(t.rows.len(), 1, "the data row is accepted, not dropped");
        assert_eq!(t.signal_name(0), Some("sBox[1,1]"));
        let (_, ys) = t.series(0);
        assert_eq!(ys, vec![5.0]);
    }

    #[test]
    fn rfc4180_quoted_header_cells_are_unquoted() {
        // The simulator now RFC-4180-quotes only the cells that need it
        // (matlab_llvm#392/#394): `t,"sBox[1,1]",scope`. Quotes are stripped, the
        // embedded comma is literal, and `""` is an escaped quote.
        let mut t = SimTrace::new();
        assert!(!t.feed_line(r#"t,"sBox[1,1]","s,""x""",scope"#));
        assert_eq!(t.columns, ["t", "sBox[1,1]", r#"s,"x""#, "scope"]);
        assert_eq!(t.signal_count(), 3);
        assert!(t.feed_line("0.0,5.0,6.0,7.0"));
        assert_eq!(t.signal_name(0), Some("sBox[1,1]"));
        assert_eq!(t.series(2).1, vec![7.0]);
    }

    #[test]
    fn series_extracts_columns() {
        let mut t = SimTrace::new();
        t.feed_line("t,a,b");
        t.feed_line("0.0,10.0,20.0");
        t.feed_line("1.0,11.0,21.0");
        let (xs, ys) = t.series(0);
        assert_eq!(xs, vec![0.0, 1.0]);
        assert_eq!(ys, vec![10.0, 11.0]);
        let (_, ys_b) = t.series(1);
        assert_eq!(ys_b, vec![20.0, 21.0]);
        assert_eq!(t.last_time(), Some(1.0));
    }

    #[test]
    fn ignores_non_csv_and_banner_lines() {
        let mut t = SimTrace::new();
        assert!(!t.feed_line("MflowLinkModel entry=lowpass blocks=4"));
        assert!(!t.feed_line("  solver type=variable_step"));
        // header still picked up afterwards
        assert!(!t.feed_line("t,scope"));
        assert!(t.feed_line("0.0,0.5"));
        // a stray trailing log line with wrong arity is dropped
        assert!(!t.feed_line("done in 10ms"));
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn data_before_header_synthesizes_names() {
        let mut t = SimTrace::new();
        assert!(t.feed_line("0.0,1.0,2.0"));
        assert_eq!(t.columns, ["t", "y1", "y2"]);
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn csv_window_keeps_only_visible_rows() {
        let mut t = SimTrace::new();
        t.feed_line("t,a");
        for i in 0..5 {
            t.feed_line(&format!("{}.0,{}.0", i, i * 10));
        }
        // Full export keeps every row; the window drops out-of-range times.
        assert_eq!(t.to_csv().lines().count(), 6); // header + 5
        let windowed = t.to_csv_window(Some((1.0, 3.0)));
        assert_eq!(windowed.lines().count(), 4); // header + t=1,2,3
        assert!(windowed.contains("1,10"));
        assert!(windowed.contains("3,30"));
        assert!(!windowed.contains("0,0"));
        assert!(!windowed.contains("4,40"));
    }

    #[test]
    fn to_csv_roundtrips_header_and_rows() {
        let mut t = SimTrace::new();
        t.feed_line("t,a,b");
        t.feed_line("0,1,2");
        t.feed_line("0.5,1.5,2.5");
        let csv = t.to_csv();
        assert_eq!(csv, "t,a,b\n0,1,2\n0.5,1.5,2.5\n");
        // Re-importing the export reproduces the same trace.
        let mut t2 = SimTrace::new();
        for line in csv.lines() {
            t2.feed_line(line);
        }
        assert_eq!(t2.columns, t.columns);
        assert_eq!(t2.rows, t.rows);
    }

    #[test]
    fn to_csv_empty_trace_is_header_only() {
        let mut t = SimTrace::new();
        t.feed_line("t,scope");
        assert_eq!(t.to_csv(), "t,scope\n");
    }

    #[test]
    fn wrong_arity_row_is_rejected() {
        let mut t = SimTrace::new();
        t.feed_line("t,a,b");
        assert!(!t.feed_line("0.0,1.0")); // too few columns
        assert!(t.rows.is_empty());
    }
}
