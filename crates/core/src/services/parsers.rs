//! Text-output parsers for the REPL: `whos` → workspace variables, and
//! `disp(var)` → a 2-D matrix. Pure functions, ported from
//! `WorkspaceParser.swift` and `MatrixParser.swift`.

use crate::models::{DType, WorkspaceVariable};

/// Parse `whos` text into workspace variables. Supports the 4-column standard
/// MATLAB layout (with Bytes) and the 3-column `matlabc -repl` layout.
pub fn parse_workspace(text: &str) -> Vec<WorkspaceVariable> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line == "Name" || line.starts_with("Name ") || line.starts_with("Name\t") {
            continue;
        }
        if line.starts_with(">>") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 3 {
            continue;
        }
        let name = tokens[0];
        let size = tokens[1];
        // Reject rows whose "size" isn't a dimension descriptor.
        if !(size.contains('x') || size.contains('\u{00d7}') || size == "scalar") {
            continue;
        }
        // token[2] integer → Bytes present (4-col); else it's the Class (3-col).
        let (bytes, class) = if tokens.len() >= 4 {
            match tokens[2].parse::<usize>() {
                Ok(b) => (b, tokens[3]),
                Err(_) => (0, tokens[2]),
            }
        } else {
            (0, tokens[2])
        };
        out.push(
            WorkspaceVariable::new(name, dtype_for(class), size, bytes)
                .with_preview(format!("{size} {class}")),
        );
    }
    out
}

fn dtype_for(class: &str) -> DType {
    let lower = class.to_lowercase();
    match lower.as_str() {
        "double" | "single" | "float" => DType::Double,
        "char" | "string" => DType::Char,
        "logical" | "bool" => DType::Logical,
        "cell" => DType::Cell,
        "struct" | "object" => DType::Struct,
        "table" => DType::Table,
        "categorical" => DType::Categorical,
        "datetime" => DType::Datetime,
        "duration" => DType::Duration,
        _ if lower.starts_with("int") || lower.starts_with("uint") => DType::Int32,
        _ if lower.contains("complex") => DType::Complex,
        _ => DType::Double,
    }
}

/// Parse a captured `disp` output block into a row-major matrix. Handles
/// `Columns N through M` column-group headers by stitching groups side by side.
/// Non-numeric tokens become `0.0`.
pub fn parse_matrix(text: &str) -> (usize, usize, Vec<Vec<f64>>) {
    let mut row_groups: Vec<Vec<Vec<f64>>> = Vec::new();
    let mut current: Vec<Vec<f64>> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(">>") {
            continue;
        }
        if line.to_lowercase().starts_with("columns ") {
            if !current.is_empty() {
                row_groups.push(std::mem::take(&mut current));
            }
            continue;
        }
        let tokens: Vec<f64> = line
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            // Rust parses "NaN"/"inf" as real floats; the reference treats any
            // non-finite token as 0.0 (obvious in the heatmap, fine for preview).
            .map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0))
            .collect();
        if tokens.is_empty() {
            continue;
        }
        current.push(tokens);
    }
    if !current.is_empty() {
        row_groups.push(current);
    }

    let mut iter = row_groups.into_iter();
    let mut rows = match iter.next() {
        Some(first) => first,
        None => return (0, 0, Vec::new()),
    };
    for next in iter {
        for (i, added_row) in next.into_iter().enumerate() {
            if i < rows.len() {
                rows[i].extend(added_row);
            } else {
                rows.push(added_row);
            }
        }
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    for row in &mut rows {
        if row.len() < cols {
            row.resize(cols, 0.0);
        }
    }
    (rows.len(), cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_four_column_whos() {
        let text = "Name      Size       Bytes  Class      Attributes\n\
                    A         3x3           72  double\n\
                    x         1x1001      8008  double";
        let vars = parse_workspace(text);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "A");
        assert_eq!(vars[0].size, "3x3");
        assert_eq!(vars[0].bytes, 72);
        assert_eq!(vars[0].dtype, DType::Double);
    }

    #[test]
    fn parses_three_column_whos() {
        let text = "Name      Size      Class\n\
                    a         1x1       double\n\
                    A         2x2       int32";
        let vars = parse_workspace(text);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[1].dtype, DType::Int32);
        assert_eq!(vars[1].bytes, 0);
    }

    #[test]
    fn skips_headers_prompts_and_junk() {
        let text = ">> whos\nName  Size  Class\n----  ----  -----\nb  1x1  logical";
        let vars = parse_workspace(text);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "b");
        assert_eq!(vars[0].dtype, DType::Logical);
    }

    #[test]
    fn dtype_classification() {
        assert_eq!(dtype_for("uint8"), DType::Int32);
        assert_eq!(dtype_for("string"), DType::Char);
        assert_eq!(dtype_for("table"), DType::Table);
        assert_eq!(dtype_for("Complex double"), DType::Complex);
    }

    #[test]
    fn parses_simple_matrix() {
        let text = "    1     2     3\n    4     5     6";
        let (rows, cols, cells) = parse_matrix(text);
        assert_eq!((rows, cols), (2, 3));
        assert_eq!(cells[1], vec![4.0, 5.0, 6.0]);
    }

    #[test]
    fn stitches_column_groups() {
        let text = "  Columns 1 through 3\n  1  2  3\n  4  5  6\n  Columns 4 through 5\n  7  8\n  9  10";
        let (rows, cols, cells) = parse_matrix(text);
        assert_eq!((rows, cols), (2, 5));
        assert_eq!(cells[0], vec![1.0, 2.0, 3.0, 7.0, 8.0]);
        assert_eq!(cells[1], vec![4.0, 5.0, 6.0, 9.0, 10.0]);
    }

    #[test]
    fn pads_ragged_rows() {
        let text = "1 2 3\n4 5";
        let (rows, cols, cells) = parse_matrix(text);
        assert_eq!((rows, cols), (2, 3));
        assert_eq!(cells[1], vec![4.0, 5.0, 0.0]);
    }

    #[test]
    fn non_numeric_tokens_become_zero() {
        let (_, _, cells) = parse_matrix("1 NaN 3");
        assert_eq!(cells[0], vec![1.0, 0.0, 3.0]);
    }

    #[test]
    fn empty_input_yields_empty_matrix() {
        assert_eq!(parse_matrix("   \n>> \n"), (0, 0, Vec::new()));
    }
}
