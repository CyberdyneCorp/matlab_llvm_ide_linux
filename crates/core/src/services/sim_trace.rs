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
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let all_numeric = fields.iter().all(|f| f.parse::<f64>().is_ok());

        if self.columns.is_empty() {
            if all_numeric {
                // Data before a header — synthesize generic column names.
                self.columns = std::iter::once("t".to_string())
                    .chain((1..fields.len()).map(|i| format!("y{i}")))
                    .collect();
                return self.push_row(&fields);
            }
            self.columns = fields.iter().map(|s| s.to_string()).collect();
            return false;
        }

        if all_numeric && fields.len() == self.columns.len() {
            self.push_row(&fields)
        } else {
            false
        }
    }

    fn push_row(&mut self, fields: &[&str]) -> bool {
        let row: Vec<f64> = fields.iter().map(|f| f.parse::<f64>().unwrap_or(f64::NAN)).collect();
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
        self.rows.iter().map(|r| r.first().copied().unwrap_or(f64::NAN)).collect()
    }

    /// `(time, values)` for signal `i` (0-based among signals).
    pub fn series(&self, i: usize) -> (Vec<f64>, Vec<f64>) {
        let col = i + 1;
        let xs = self.time();
        let ys = self.rows.iter().map(|r| r.get(col).copied().unwrap_or(f64::NAN)).collect();
        (xs, ys)
    }

    /// Latest sampled time, if any.
    pub fn last_time(&self) -> Option<f64> {
        self.rows.last().and_then(|r| r.first().copied())
    }
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
    fn wrong_arity_row_is_rejected() {
        let mut t = SimTrace::new();
        t.feed_line("t,a,b");
        assert!(!t.feed_line("0.0,1.0")); // too few columns
        assert!(t.rows.is_empty());
    }
}
