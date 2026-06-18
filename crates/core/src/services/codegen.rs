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
}

impl ExportTarget {
    /// Every lane, in menu order.
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
}
