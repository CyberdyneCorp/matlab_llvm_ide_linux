//! Pure scope-tile math for the mflowLink overlay scope: autoscale over many
//! signals, manual-Y override, pixel↔data mapping for box-zoom / pan, and a
//! stable per-signal color palette. The GTK layer renders with Cairo and feeds
//! gestures through here; everything in this module is side-effect free.

/// Plot bounds in data space.
pub type Bounds = (f64, f64, f64, f64); // (x0, x1, y0, y1)

/// A stable, distinct color (RGB in 0..1) for signal `index`, cycling through a
/// fixed palette so a given trace keeps the same colors across redraws.
pub fn signal_color(index: usize) -> (f64, f64, f64) {
    const PALETTE: [(f64, f64, f64); 8] = [
        (0.31, 0.64, 0.89),  // blue
        (0.37, 0.745, 0.43), // green
        (0.88, 0.54, 0.27),  // orange
        (0.78, 0.47, 0.87),  // magenta
        (0.90, 0.36, 0.36),  // red
        (0.36, 0.78, 0.78),  // cyan
        (0.85, 0.78, 0.35),  // yellow
        (0.60, 0.60, 0.72),  // slate
    ];
    PALETTE[index % PALETTE.len()]
}

/// The visible window of a scope. `None` on an axis means autoscale that axis;
/// `Some((lo, hi))` pins it (manual Y range, or a box-zoom / pan result).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ScopeView {
    pub x: Option<(f64, f64)>,
    pub y: Option<(f64, f64)>,
}

impl ScopeView {
    /// Data bounds over every finite sample of every series, with 5% Y padding.
    /// `None` when there is no finite data to frame.
    pub fn autoscale(series: &[Vec<(f64, f64)>]) -> Option<Bounds> {
        let (mut x0, mut x1, mut y0, mut y1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        let mut any = false;
        for s in series {
            for &(x, y) in s {
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                any = true;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        if !any {
            return None;
        }
        // Widen zero-width axes so the mapping never divides by zero.
        if x1 <= x0 {
            x1 = x0 + 1.0;
        }
        let pad = if y1 > y0 { (y1 - y0) * 0.05 } else { 0.5 };
        Some((x0, x1, y0 - pad, y1 + pad))
    }

    /// The effective window: a manual axis overrides autoscale; with no data and
    /// no manual range it falls back to the unit box.
    pub fn resolve(&self, series: &[Vec<(f64, f64)>]) -> Bounds {
        let auto = ScopeView::autoscale(series).unwrap_or((0.0, 1.0, 0.0, 1.0));
        let (ax0, ax1, ay0, ay1) = auto;
        let (x0, x1) = self.x.unwrap_or((ax0, ax1));
        let (y0, y1) = self.y.unwrap_or((ay0, ay1));
        (x0, x1, y0, y1)
    }

    /// Convert a pixel box (drawn inside the plot rect `(px, py, pw, ph)`) to a
    /// pinned data window, given the currently-resolved `window`. A box smaller
    /// than `min_px` on either side is ignored (returns `self` unchanged) so a
    /// stray click doesn't zoom to a sliver.
    pub fn zoom_to_box(
        self,
        window: Bounds,
        plot: (f64, f64, f64, f64),
        b: (f64, f64, f64, f64),
    ) -> ScopeView {
        const MIN_PX: f64 = 6.0;
        let (px, py, pw, ph) = plot;
        let (bx0, by0, bx1, by1) = b;
        if pw <= 0.0 || ph <= 0.0 || (bx1 - bx0).abs() < MIN_PX || (by1 - by0).abs() < MIN_PX {
            return self;
        }
        let (wx0, wx1, wy0, wy1) = window;
        let to_x = |sx: f64| wx0 + (sx - px) / pw * (wx1 - wx0);
        // Pixel y grows downward; the top of the plot is the max data y.
        let to_y = |sy: f64| wy1 - (sy - py) / ph * (wy1 - wy0);
        let (xa, xb) = (to_x(bx0), to_x(bx1));
        let (ya, yb) = (to_y(by0), to_y(by1));
        ScopeView {
            x: Some((xa.min(xb), xa.max(xb))),
            y: Some((ya.min(yb), ya.max(yb))),
        }
    }

    /// Pin `window` shifted by a data-space delta (used by drag-to-pan).
    pub fn panned(window: Bounds, dx: f64, dy: f64) -> ScopeView {
        let (x0, x1, y0, y1) = window;
        ScopeView {
            x: Some((x0 + dx, x1 + dx)),
            y: Some((y0 + dy, y1 + dy)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_stable_and_cycle() {
        assert_eq!(signal_color(0), signal_color(0));
        assert_eq!(signal_color(0), signal_color(8)); // wraps at palette length
        assert_ne!(signal_color(0), signal_color(1));
    }

    #[test]
    fn autoscale_frames_all_series_with_padding() {
        let s = vec![vec![(0.0, 1.0), (1.0, 3.0)], vec![(0.0, -2.0), (2.0, 0.0)]];
        let (x0, x1, y0, y1) = ScopeView::autoscale(&s).unwrap();
        assert_eq!((x0, x1), (0.0, 2.0));
        // y spans -2..3 with 5% padding either side.
        assert!(y0 < -2.0 && y1 > 3.0);
        assert!((y0 - (-2.25)).abs() < 1e-9 && (y1 - 3.25).abs() < 1e-9);
        // NaN / empty → None.
        assert!(ScopeView::autoscale(&[]).is_none());
        assert!(ScopeView::autoscale(&[vec![(f64::NAN, 1.0)]]).is_none());
    }

    #[test]
    fn manual_axis_overrides_autoscale() {
        let s = vec![vec![(0.0, 0.0), (10.0, 100.0)]];
        let view = ScopeView {
            x: None,
            y: Some((-1.0, 1.0)),
        };
        let (x0, x1, y0, y1) = view.resolve(&s);
        assert_eq!((x0, x1), (0.0, 10.0)); // x still autoscaled
        assert_eq!((y0, y1), (-1.0, 1.0)); // y pinned
    }

    #[test]
    fn box_zoom_maps_pixels_to_data() {
        // window 0..10 (x), 0..10 (y); plot rect at origin 100x100 px.
        let window = (0.0, 10.0, 0.0, 10.0);
        let plot = (0.0, 0.0, 100.0, 100.0);
        // Box from (20,80) to (60,20) px → x 2..6, y 2..8 (y inverted).
        let v = ScopeView::default().zoom_to_box(window, plot, (20.0, 80.0, 60.0, 20.0));
        let (x0, x1) = v.x.unwrap();
        let (y0, y1) = v.y.unwrap();
        assert!((x0 - 2.0).abs() < 1e-9 && (x1 - 6.0).abs() < 1e-9);
        assert!((y0 - 2.0).abs() < 1e-9 && (y1 - 8.0).abs() < 1e-9);

        // A sliver box is ignored.
        let unchanged = ScopeView::default().zoom_to_box(window, plot, (20.0, 20.0, 22.0, 21.0));
        assert_eq!(unchanged, ScopeView::default());
    }

    #[test]
    fn pan_shifts_the_window() {
        let v = ScopeView::panned((0.0, 10.0, 0.0, 10.0), 2.0, -3.0);
        assert_eq!(v.x, Some((2.0, 12.0)));
        assert_eq!(v.y, Some((-3.0, 7.0)));
    }
}
