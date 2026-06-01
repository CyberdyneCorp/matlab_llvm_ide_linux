//! Compiler configuration enums (`CompilerTarget`, `OptimizationProfile`,
//! `NumericMode`). Mirrors `Models.swift`. `matlabc_flag` is the authoritative
//! source for how the IDE invokes the compiler.

/// Output target. Drives both the toolbar picker and the `matlabc` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompilerTarget {
    Cpp,
    C,
    Llvm,
    Python,
    TypeScript,
    Mlir,
    Sv,
    /// Verilog-A has no `-emit-*` flag — it side-emits during a Run.
    Va,
}

impl CompilerTarget {
    pub const ALL: [CompilerTarget; 8] = [
        CompilerTarget::Cpp,
        CompilerTarget::C,
        CompilerTarget::Llvm,
        CompilerTarget::Python,
        CompilerTarget::TypeScript,
        CompilerTarget::Mlir,
        CompilerTarget::Sv,
        CompilerTarget::Va,
    ];

    /// Human label (matches the reference `rawValue`).
    pub fn label(self) -> &'static str {
        match self {
            CompilerTarget::Cpp => "C++",
            CompilerTarget::C => "C",
            CompilerTarget::Llvm => "LLVM IR",
            CompilerTarget::Python => "Python",
            CompilerTarget::TypeScript => "TypeScript",
            CompilerTarget::Mlir => "MLIR",
            CompilerTarget::Sv => "SystemVerilog",
            CompilerTarget::Va => "Verilog-A",
        }
    }

    /// `matlabc` emit-flag, or `None` for run-to-emit lanes (`.va`).
    pub fn matlabc_flag(self) -> Option<&'static str> {
        match self {
            CompilerTarget::Cpp => Some("-emit-cpp"),
            CompilerTarget::C => Some("-emit-c"),
            CompilerTarget::Llvm => Some("-emit-llvm"),
            CompilerTarget::Python => Some("-emit-python"),
            CompilerTarget::TypeScript => Some("-emit-ts"),
            CompilerTarget::Mlir => Some("-emit-mlir"),
            CompilerTarget::Sv => Some("-emit-sv"),
            CompilerTarget::Va => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OptimizationProfile {
    O0,
    O1,
    O2,
    O3,
    Os,
}

impl OptimizationProfile {
    pub const ALL: [OptimizationProfile; 5] = [
        OptimizationProfile::O0,
        OptimizationProfile::O1,
        OptimizationProfile::O2,
        OptimizationProfile::O3,
        OptimizationProfile::Os,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OptimizationProfile::O0 => "O0 (Debug)",
            OptimizationProfile::O1 => "O1",
            OptimizationProfile::O2 => "O2 (Fast)",
            OptimizationProfile::O3 => "O3 (Aggressive)",
            OptimizationProfile::Os => "Os (Size)",
        }
    }

    /// Whether to pass `-O` to `matlabc` (O0 = no flag).
    pub fn passes_o_flag(self) -> bool {
        !matches!(self, OptimizationProfile::O0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericMode {
    Dynamic,
    Strict,
    Float32,
    Float64,
}

impl NumericMode {
    pub const ALL: [NumericMode; 4] =
        [NumericMode::Dynamic, NumericMode::Strict, NumericMode::Float32, NumericMode::Float64];

    pub fn label(self) -> &'static str {
        match self {
            NumericMode::Dynamic => "Dynamic (MATLAB-like)",
            NumericMode::Strict => "Strict (Static)",
            NumericMode::Float32 => "Float (single)",
            NumericMode::Float64 => "Double (default)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_match_compiler_cli() {
        assert_eq!(CompilerTarget::Cpp.matlabc_flag(), Some("-emit-cpp"));
        assert_eq!(CompilerTarget::TypeScript.matlabc_flag(), Some("-emit-ts"));
        assert_eq!(CompilerTarget::Sv.matlabc_flag(), Some("-emit-sv"));
        assert_eq!(CompilerTarget::Va.matlabc_flag(), None);
    }

    #[test]
    fn opt_flag_gate() {
        assert!(!OptimizationProfile::O0.passes_o_flag());
        assert!(OptimizationProfile::O2.passes_o_flag());
    }

    #[test]
    fn labels_present_for_all() {
        for t in CompilerTarget::ALL {
            assert!(!t.label().is_empty());
            // every target either has an emit flag or is the Va run-to-emit lane
            assert!(t.matlabc_flag().is_some() || t == CompilerTarget::Va);
        }
        for o in OptimizationProfile::ALL {
            assert!(!o.label().is_empty());
        }
        for n in NumericMode::ALL {
            assert!(!n.label().is_empty());
        }
        assert_eq!(NumericMode::Float64.label(), "Double (default)");
    }

    #[test]
    fn opt_flag_gate_for_all_profiles() {
        assert!(!OptimizationProfile::O0.passes_o_flag());
        for o in [OptimizationProfile::O1, OptimizationProfile::O2, OptimizationProfile::O3, OptimizationProfile::Os] {
            assert!(o.passes_o_flag());
        }
    }
}
