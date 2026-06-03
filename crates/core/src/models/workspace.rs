//! Workspace variable table + matrix/inspection viewers. Mirrors `Models.swift`.

use super::ids::next_id;

/// Type tag for the workspace table's `Class` column.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DType {
    Double,
    Int32,
    Complex,
    Char,
    Logical,
    Cell,
    Struct,
    Table,
    Categorical,
    Datetime,
    Duration,
    Object(String),
}

impl DType {
    pub fn display_name(&self) -> String {
        match self {
            DType::Double => "double".into(),
            DType::Int32 => "int32".into(),
            DType::Complex => "complex".into(),
            DType::Char => "char".into(),
            DType::Logical => "logical".into(),
            DType::Cell => "cell".into(),
            DType::Struct => "struct".into(),
            DType::Table => "table".into(),
            DType::Categorical => "categorical".into(),
            DType::Datetime => "datetime".into(),
            DType::Duration => "duration".into(),
            DType::Object(name) => name.clone(),
        }
    }

    /// Whether `disp(var)` yields a numeric matrix the inspector can parse.
    /// Also a safety gate: `disp` on a struct/cell currently segfaults the
    /// matlabc REPL (matlab_llvm#156), so the IDE must not probe non-matrix
    /// variables.
    pub fn is_inspectable_matrix(&self) -> bool {
        matches!(self, DType::Double | DType::Int32 | DType::Complex | DType::Logical)
    }

    /// Map a MATLAB `whos` class string onto a `DType`.
    pub fn from_class(class: &str) -> DType {
        match class {
            "double" => DType::Double,
            "int32" => DType::Int32,
            "complex" => DType::Complex,
            "char" => DType::Char,
            "logical" => DType::Logical,
            "cell" => DType::Cell,
            "struct" => DType::Struct,
            "table" => DType::Table,
            "categorical" => DType::Categorical,
            "datetime" => DType::Datetime,
            "duration" => DType::Duration,
            other => DType::Object(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceVariable {
    pub id: u64,
    pub name: String,
    pub dtype: DType,
    pub size: String,
    pub bytes: usize,
    pub preview: String,
}

impl WorkspaceVariable {
    pub fn new(name: impl Into<String>, dtype: DType, size: impl Into<String>, bytes: usize) -> WorkspaceVariable {
        WorkspaceVariable {
            id: next_id(),
            name: name.into(),
            dtype,
            size: size.into(),
            bytes,
            preview: String::new(),
        }
    }

    pub fn with_preview(mut self, preview: impl Into<String>) -> WorkspaceVariable {
        self.preview = preview.into();
        self
    }
}

/// Numeric matrix preview for the Matrix Viewer heatmap.
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixView {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Vec<f64>>,
    pub title: String,
}

impl MatrixView {
    pub fn new(title: impl Into<String>, cells: Vec<Vec<f64>>) -> MatrixView {
        let rows = cells.len();
        let cols = cells.first().map_or(0, |r| r.len());
        MatrixView { rows, cols, cells, title: title.into() }
    }

    /// Min and max finite cell values, for the heatmap gradient scale.
    /// Returns `None` for an empty matrix.
    pub fn value_range(&self) -> Option<(f64, f64)> {
        let mut iter = self.cells.iter().flatten().copied().filter(|v| v.is_finite());
        let first = iter.next()?;
        let (mut lo, mut hi) = (first, first);
        for v in iter {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        Some((lo, hi))
    }
}

/// One row in the structured Variable Inspector (struct/object field).
#[derive(Clone, Debug, PartialEq)]
pub struct InspectionField {
    pub name: String,
    pub value: String,
    pub type_hint: Option<String>,
}

/// One column of a MATLAB table local during a debug session.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectionColumn {
    pub id: u64,
    pub name: String,
    pub matlab_type: String,
    pub cell_preview: String,
    pub variables_reference: i64,
}

impl InspectionColumn {
    pub fn new(name: impl Into<String>, matlab_type: impl Into<String>) -> InspectionColumn {
        InspectionColumn {
            id: next_id(),
            name: name.into(),
            matlab_type: matlab_type.into(),
            cell_preview: String::new(),
            variables_reference: 0,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self.matlab_type.to_lowercase().as_str(),
            "double" | "single" | "float" | "int8" | "int16" | "int32" | "int64" | "uint8"
                | "uint16" | "uint32" | "uint64" | "logical"
        )
    }
}

/// One method row in the Variable Inspector for a class instance.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectionMethod {
    pub name: String,
    pub signature: String,
    pub inherited_from: Option<String>,
}

impl InspectionMethod {
    pub fn is_inherited(&self) -> bool {
        self.inherited_from.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtype_from_class_and_display() {
        assert_eq!(DType::from_class("double"), DType::Double);
        assert_eq!(DType::from_class("BankAccount"), DType::Object("BankAccount".into()));
        assert_eq!(DType::Object("BankAccount".into()).display_name(), "BankAccount");
        assert_eq!(DType::Struct.display_name(), "struct");
    }

    #[test]
    fn only_numeric_types_are_inspectable() {
        assert!(DType::Double.is_inspectable_matrix());
        assert!(DType::Int32.is_inspectable_matrix());
        assert!(DType::Logical.is_inspectable_matrix());
        assert!(!DType::Struct.is_inspectable_matrix());
        assert!(!DType::Cell.is_inspectable_matrix());
        assert!(!DType::Object("BankAccount".into()).is_inspectable_matrix());
    }

    #[test]
    fn matrix_view_dimensions_and_range() {
        let m = MatrixView::new("M", vec![vec![1.0, 2.0], vec![-3.0, 4.0]]);
        assert_eq!((m.rows, m.cols), (2, 2));
        assert_eq!(m.value_range(), Some((-3.0, 4.0)));
    }

    #[test]
    fn matrix_view_empty_has_no_range() {
        let m = MatrixView::new("empty", vec![]);
        assert_eq!(m.value_range(), None);
    }

    #[test]
    fn matrix_range_ignores_non_finite() {
        let m = MatrixView::new("M", vec![vec![f64::NAN, 5.0, f64::INFINITY, 1.0]]);
        assert_eq!(m.value_range(), Some((1.0, 5.0)));
    }

    #[test]
    fn inspection_column_numeric_detection() {
        assert!(InspectionColumn::new("a", "double").is_numeric());
        assert!(!InspectionColumn::new("b", "string").is_numeric());
    }

    #[test]
    fn inspection_method_inheritance() {
        let m = InspectionMethod {
            name: "f".into(),
            signature: "@f()".into(),
            inherited_from: Some("Base".into()),
        };
        assert!(m.is_inherited());
    }
}
