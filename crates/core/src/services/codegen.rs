//! Codegen export lanes for flowchart documents — the `matlabc -emit-*` /
//! `-dump-chart` targets surfaced by the editor's Export menu. Pure metadata
//! (flag, menu label, output extension); the GTK layer runs the subprocess.

/// One `matlabc` codegen lane a flowchart can be exported through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Matlab,
    DumpChart,
    C,
    Cpp,
    Llvm,
    SystemVerilog,
    /// The mflowLink 3-D scene: `matlabc -emit-mflowlink-babylon` produces a
    /// self-contained interactive Babylon.js HTML viewer. Unlike the text lanes
    /// this opens in an embedded 3-D Scene window rather than the editor, so it
    /// is intentionally excluded from [`ExportTarget::all`].
    Babylon,
}

impl ExportTarget {
    /// Every text-export lane, in menu order. The Babylon lane is excluded — it
    /// is surfaced through the gated 3-D Scene action, not the Export menu.
    pub fn all() -> [ExportTarget; 6] {
        use ExportTarget::*;
        [Matlab, DumpChart, C, Cpp, Llvm, SystemVerilog]
    }

    /// The `matlabc` command-line flag for this lane.
    pub fn flag(self) -> &'static str {
        match self {
            ExportTarget::Matlab => "-emit-matlab",
            ExportTarget::DumpChart => "-dump-chart",
            ExportTarget::C => "-emit-c",
            ExportTarget::Cpp => "-emit-cpp",
            ExportTarget::Llvm => "-emit-llvm",
            ExportTarget::SystemVerilog => "-emit-systemverilog",
            ExportTarget::Babylon => "-emit-mflowlink-babylon",
        }
    }

    /// File extension for the generated artifact (drives the editor's syntax
    /// highlighting when the result is opened).
    pub fn extension(self) -> &'static str {
        match self {
            ExportTarget::Matlab => "m",
            ExportTarget::DumpChart => "txt",
            ExportTarget::C => "c",
            ExportTarget::Cpp => "cpp",
            ExportTarget::Llvm => "ll",
            ExportTarget::SystemVerilog => "sv",
            ExportTarget::Babylon => "html",
        }
    }

    /// Human-readable menu label.
    pub fn label(self) -> &'static str {
        match self {
            ExportTarget::Matlab => "MATLAB (.m)",
            ExportTarget::DumpChart => "Chart dump (.txt)",
            ExportTarget::C => "C (.c)",
            ExportTarget::Cpp => "C++ (.cpp)",
            ExportTarget::Llvm => "LLVM IR (.ll)",
            ExportTarget::SystemVerilog => "SystemVerilog (.sv)",
            ExportTarget::Babylon => "3-D Scene (.html)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_cover_every_target_with_distinct_flags_and_extensions() {
        let all = ExportTarget::all();
        assert_eq!(all.len(), 6);

        // Flags match the compiler's chart codegen lanes.
        assert_eq!(ExportTarget::Matlab.flag(), "-emit-matlab");
        assert_eq!(ExportTarget::DumpChart.flag(), "-dump-chart");
        assert_eq!(ExportTarget::SystemVerilog.flag(), "-emit-systemverilog");

        // Flags and extensions are unique per lane.
        let mut flags: Vec<&str> = all.iter().map(|t| t.flag()).collect();
        flags.sort_unstable();
        flags.dedup();
        assert_eq!(flags.len(), 6);

        let mut exts: Vec<&str> = all.iter().map(|t| t.extension()).collect();
        exts.sort_unstable();
        exts.dedup();
        assert_eq!(exts.len(), 6);

        // Every lane has a non-empty label.
        assert!(all.iter().all(|t| !t.label().is_empty()));
    }

    #[test]
    fn babylon_lane_is_html_and_outside_the_text_export_menu() {
        // The 3-D scene lane carries the compiler flag and an .html extension,
        assert_eq!(ExportTarget::Babylon.flag(), "-emit-mflowlink-babylon");
        assert_eq!(ExportTarget::Babylon.extension(), "html");
        assert!(!ExportTarget::Babylon.label().is_empty());

        // …but is deliberately not part of the generic text Export menu (it
        // opens a 3-D Scene window instead of loading text into the editor).
        assert!(!ExportTarget::all().contains(&ExportTarget::Babylon));
    }
}
