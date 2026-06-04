//! Plot figure model for the Plots panel. Mirrors `Models.swift` (`PlotFigure`).
//! `png_data` holds a Cairo-rendered bitmap from the runtime emit path; when
//! present the chart pane shows it directly and ignores the xs/ys series.

use super::ids::next_id;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlotKind {
    Line2D,
    Line2DMulti,
    Scatter,
    Bar,
    Histogram,
    Spectrum,
    Surface3D,
    /// Rendered PNG from the runtime — bitmap is the source of truth.
    Rendered,
}

impl PlotKind {
    pub const ALL: [PlotKind; 8] = [
        PlotKind::Line2D,
        PlotKind::Line2DMulti,
        PlotKind::Scatter,
        PlotKind::Bar,
        PlotKind::Histogram,
        PlotKind::Spectrum,
        PlotKind::Surface3D,
        PlotKind::Rendered,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PlotKind::Line2D => "Line",
            PlotKind::Line2DMulti => "Multi-line",
            PlotKind::Scatter => "Scatter",
            PlotKind::Bar => "Bar",
            PlotKind::Histogram => "Histogram",
            PlotKind::Spectrum => "Spectrum (area)",
            PlotKind::Surface3D => "Surface (3D)",
            PlotKind::Rendered => "Rendered",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotFigure {
    pub id: u64,
    pub index: i32,
    pub title: String,
    pub kind: PlotKind,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub ys2: Vec<f64>,
    /// Height grid for `Surface3D` figures (`zs[row][col]`); empty otherwise.
    pub zs: Vec<Vec<f64>>,
    pub source_variable: Option<String>,
    pub png_data: Option<Vec<u8>>,
    /// Runtime-side figure id (`id=N` on the BEGIN sentinel) for dedupe.
    pub runtime_id: Option<i64>,
}

impl PlotFigure {
    pub fn series(index: i32, title: impl Into<String>, kind: PlotKind, xs: Vec<f64>, ys: Vec<f64>) -> PlotFigure {
        PlotFigure {
            id: next_id(),
            index,
            title: title.into(),
            kind,
            xs,
            ys,
            ys2: Vec::new(),
            zs: Vec::new(),
            source_variable: None,
            png_data: None,
            runtime_id: None,
        }
    }

    /// Build a 3-D surface figure from a height grid (`grid[row][col]`).
    pub fn surface(index: i32, title: impl Into<String>, grid: Vec<Vec<f64>>) -> PlotFigure {
        let mut f = PlotFigure::series(index, title, PlotKind::Surface3D, Vec::new(), Vec::new());
        f.zs = grid;
        f
    }

    pub fn with_source(mut self, name: impl Into<String>) -> PlotFigure {
        self.source_variable = Some(name.into());
        self
    }

    pub fn is_rendered(&self) -> bool {
        self.png_data.is_some()
    }

    /// True when the figure carries data the viewer can manipulate (2-D zoom/pan
    /// or 3-D orbit) — i.e. not a runtime bitmap.
    pub fn is_interactive(&self) -> bool {
        if self.png_data.is_some() {
            return false;
        }
        match self.kind {
            PlotKind::Surface3D => !self.zs.is_empty(),
            _ => !self.ys.is_empty(),
        }
    }

    /// True for an orbitable 3-D surface (height grid present, not a bitmap).
    pub fn is_surface(&self) -> bool {
        self.kind == PlotKind::Surface3D && !self.zs.is_empty() && self.png_data.is_none()
    }

    /// The x-axis samples (explicit `xs`, or `0,1,2,…` when only `ys` is set).
    fn x_samples(&self) -> Vec<f64> {
        if self.xs.len() == self.ys.len() {
            self.xs.clone()
        } else {
            (0..self.ys.len()).map(|i| i as f64).collect()
        }
    }

    /// The auto-fit data window, or `None` for non-interactive / 3-D figures.
    pub fn auto_view(&self) -> Option<PlotView> {
        if self.kind == PlotKind::Surface3D || !self.is_interactive() {
            return None;
        }
        let (x_min, x_max) = finite_range(&self.x_samples());
        let (mut y_min, mut y_max) = finite_range(&self.ys);
        if !self.ys2.is_empty() {
            let (a, b) = finite_range(&self.ys2);
            y_min = y_min.min(a);
            y_max = y_max.max(b);
        }
        if matches!(self.kind, PlotKind::Bar | PlotKind::Histogram) {
            y_min = y_min.min(0.0); // bars sit on the zero baseline
        }
        Some(PlotView { x_min, x_max, y_min, y_max })
    }

    /// The data point whose x is closest to `x` (for the hover readout).
    pub fn nearest(&self, x: f64) -> Option<(f64, f64)> {
        let xs = self.x_samples();
        xs.iter()
            .zip(&self.ys)
            .min_by(|a, b| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
            .map(|(&xv, &yv)| (xv, yv))
    }
}

/// The visible data window of a plot: `[x_min, x_max] × [y_min, y_max]`. The
/// renderer maps this rectangle onto the canvas; zoom/pan adjust it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotView {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl PlotView {
    /// Scale the window by `factor` about the fixed data point `(fx, fy)`
    /// (`factor < 1` zooms in). Keeps that point under the cursor.
    pub fn zoom_at(&self, fx: f64, fy: f64, factor: f64) -> PlotView {
        PlotView {
            x_min: fx + (self.x_min - fx) * factor,
            x_max: fx + (self.x_max - fx) * factor,
            y_min: fy + (self.y_min - fy) * factor,
            y_max: fy + (self.y_max - fy) * factor,
        }
    }

    /// Shift the window by a data-space delta (dragging right pans left).
    pub fn pan_by(&self, dx: f64, dy: f64) -> PlotView {
        PlotView {
            x_min: self.x_min - dx,
            x_max: self.x_max - dx,
            y_min: self.y_min - dy,
            y_max: self.y_max - dy,
        }
    }

    pub fn x_span(&self) -> f64 {
        self.x_max - self.x_min
    }
    pub fn y_span(&self) -> f64 {
        self.y_max - self.y_min
    }
}

/// Orbit camera for a 3-D surface: `azimuth` spins about the vertical axis,
/// `elevation` tilts, `zoom` scales. Drag orbits, scroll zooms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceCamera {
    pub azimuth: f64,
    pub elevation: f64,
    pub zoom: f64,
}

impl Default for SurfaceCamera {
    fn default() -> Self {
        // A 3/4 view that reads clearly as 3-D.
        SurfaceCamera { azimuth: 0.7, elevation: 0.5, zoom: 1.0 }
    }
}

impl SurfaceCamera {
    /// Rotate by a screen-drag delta (radians), clamping elevation to keep the
    /// surface from flipping past top/bottom.
    pub fn orbit_by(&self, d_az: f64, d_el: f64) -> SurfaceCamera {
        SurfaceCamera {
            azimuth: self.azimuth + d_az,
            elevation: (self.elevation + d_el).clamp(-1.5, 1.5),
            zoom: self.zoom,
        }
    }

    /// Scale the view, clamped to a sane range.
    pub fn zoom_by(&self, factor: f64) -> SurfaceCamera {
        SurfaceCamera { zoom: (self.zoom * factor).clamp(0.25, 6.0), ..*self }
    }

    /// Orthographically project a point in the unit cube (`x`,`y` base plane,
    /// `z` height, each roughly in `[-0.5, 0.5]`) to screen `(sx, sy)` plus a
    /// `depth` key (larger = farther) for painter's-order sorting.
    pub fn project(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        let (sa, ca) = self.azimuth.sin_cos();
        // Spin about the vertical (height) axis.
        let rx = x * ca - y * sa;
        let ry = x * sa + y * ca;
        // Tilt about the screen-horizontal axis: height stays vertical at
        // elevation 0, becomes depth as elevation → π/2 (top-down).
        let (se, ce) = self.elevation.sin_cos();
        let sy = ry * se + z * ce;
        let depth = ry * ce - z * se;
        (rx * self.zoom, sy * self.zoom, depth)
    }
}

/// Min/max over the finite values of `v`, padded so a flat/empty series still
/// has a non-zero span.
fn finite_range(v: &[f64]) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &x in v.iter().filter(|x| x.is_finite()) {
        lo = lo.min(x);
        hi = hi.max(x);
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if (hi - lo).abs() < f64::EPSILON {
        return (lo - 1.0, hi + 1.0);
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_figure_defaults() {
        let f = PlotFigure::series(1, "Figure 1", PlotKind::Line2D, vec![0.0, 1.0], vec![1.0, 2.0]);
        assert_eq!(f.kind, PlotKind::Line2D);
        assert!(!f.is_rendered());
        assert_eq!(f.xs.len(), 2);
    }

    #[test]
    fn with_source_sets_variable() {
        let f = PlotFigure::series(1, "F", PlotKind::Bar, vec![], vec![]).with_source("y");
        assert_eq!(f.source_variable.as_deref(), Some("y"));
    }

    #[test]
    fn kind_labels() {
        assert_eq!(PlotKind::ALL.len(), 8);
        // Every kind has a distinct, non-empty label.
        let labels: Vec<&str> = PlotKind::ALL.iter().map(|k| k.label()).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "labels must be unique");
        assert_eq!(PlotKind::Surface3D.label(), "Surface (3D)");
    }

    #[test]
    fn rendered_figure_uses_png() {
        let mut f = PlotFigure::series(2, "F", PlotKind::Rendered, vec![], vec![]);
        assert!(!f.is_rendered());
        f.png_data = Some(vec![0x89, 0x50]);
        assert!(f.is_rendered());
    }

    #[test]
    fn auto_view_fits_data_and_interactivity() {
        let f = PlotFigure::series(1, "L", PlotKind::Line2D, vec![0.0, 1.0, 2.0], vec![3.0, 9.0, 5.0]);
        assert!(f.is_interactive());
        let v = f.auto_view().unwrap();
        assert_eq!((v.x_min, v.x_max), (0.0, 2.0));
        assert_eq!((v.y_min, v.y_max), (3.0, 9.0));
        // Bitmap + 3-D figures are not interactive (no view).
        let mut png = PlotFigure::series(2, "P", PlotKind::Rendered, vec![], vec![1.0]);
        png.png_data = Some(vec![1, 2]);
        assert!(!png.is_interactive() && png.auto_view().is_none());
    }

    #[test]
    fn bar_view_includes_zero_baseline() {
        let f = PlotFigure::series(1, "B", PlotKind::Bar, vec![0.0, 1.0], vec![4.0, 7.0]);
        assert_eq!(f.auto_view().unwrap().y_min, 0.0);
    }

    #[test]
    fn surface_is_interactive_and_orbits() {
        let grid = vec![vec![0.0, 1.0, 0.0], vec![1.0, 2.0, 1.0], vec![0.0, 1.0, 0.0]];
        let f = PlotFigure::surface(1, "peak", grid);
        assert_eq!(f.kind, PlotKind::Surface3D);
        assert!(f.is_interactive() && f.is_surface());
        // 3-D figures have no 2-D auto-fit window.
        assert!(f.auto_view().is_none());
        // An empty-grid surface is inert.
        assert!(!PlotFigure::surface(2, "empty", vec![]).is_interactive());

        // Orbit clamps elevation; zoom clamps to its range.
        let c = SurfaceCamera::default();
        let o = c.orbit_by(1.0, 5.0);
        assert!((o.azimuth - 1.7).abs() < 1e-9);
        assert_eq!(o.elevation, 1.5); // clamped
        assert_eq!(c.zoom_by(100.0).zoom, 6.0); // clamped
        assert_eq!(c.zoom_by(0.0001).zoom, 0.25);
    }

    #[test]
    fn surface_projection_separates_depth() {
        // Looking from a 3/4 angle, two points at different base-plane y must
        // land at different depths, and a taller z must sit higher on screen.
        let cam = SurfaceCamera::default();
        let (_, _, dn) = cam.project(0.0, -0.5, 0.0); // near edge
        let (_, _, df) = cam.project(0.0, 0.5, 0.0); // far edge
        assert!(df > dn, "far row should have greater depth ({df} vs {dn})");
        let (_, y_low, _) = cam.project(0.0, 0.0, -0.5);
        let (_, y_high, _) = cam.project(0.0, 0.0, 0.5);
        assert!(y_high > y_low, "taller z should be higher on screen");
    }

    #[test]
    fn zoom_pan_and_nearest() {
        let v = PlotView { x_min: 0.0, x_max: 10.0, y_min: 0.0, y_max: 10.0 };
        // Zoom in (factor 0.5) about (5,5) halves the span, keeps the center.
        let z = v.zoom_at(5.0, 5.0, 0.5);
        assert_eq!((z.x_min, z.x_max), (2.5, 7.5));
        // Pan right by 2 data units shifts the window left.
        let p = v.pan_by(2.0, 0.0);
        assert_eq!((p.x_min, p.x_max), (-2.0, 8.0));

        let f = PlotFigure::series(1, "L", PlotKind::Line2D, vec![0.0, 1.0, 2.0], vec![10.0, 20.0, 30.0]);
        assert_eq!(f.nearest(1.4), Some((1.0, 20.0)));
        assert_eq!(f.nearest(1.6), Some((2.0, 30.0)));
    }
}
