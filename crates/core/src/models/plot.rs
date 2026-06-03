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
            source_variable: None,
            png_data: None,
            runtime_id: None,
        }
    }

    pub fn with_source(mut self, name: impl Into<String>) -> PlotFigure {
        self.source_variable = Some(name.into());
        self
    }

    pub fn is_rendered(&self) -> bool {
        self.png_data.is_some()
    }
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
}
