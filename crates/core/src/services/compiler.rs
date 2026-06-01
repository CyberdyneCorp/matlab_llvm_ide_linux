//! Compiler invocation: building the `matlabc` argv for an emit target, the
//! clang-style diagnostic parser, and a synchronous `CompilerService` trait
//! (real `std::process` impl + in-crate fake). The argv builder and diagnostic
//! parser are pure and carry the coverage; the process impl is exercised by the
//! env-gated integration tests. Mirrors `CompilerService.swift`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::models::{CompilerTarget, OptimizationProfile};

/// A fully-resolved `matlabc` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilerInvocation {
    pub binary: PathBuf,
    pub args: Vec<String>,
}

impl CompilerInvocation {
    /// Build the argv for emitting `target` from `source`. Returns `None` for
    /// run-to-emit lanes (`Verilog-A`) that have no `-emit-*` flag — the caller
    /// routes those through the Run pipeline instead.
    pub fn emit(
        binary: impl Into<PathBuf>,
        target: CompilerTarget,
        opt: OptimizationProfile,
        source: &Path,
    ) -> Option<CompilerInvocation> {
        let flag = target.matlabc_flag()?;
        let mut args = vec![flag.to_string()];
        if opt.passes_o_flag() {
            args.push("-O".to_string());
        }
        args.push(source.to_string_lossy().into_owned());
        Some(CompilerInvocation { binary: binary.into(), args })
    }

    /// Display form for logging (`matlabc -emit-cpp -O foo.m`).
    pub fn command_line(&self) -> String {
        let name = self
            .binary
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.binary.to_string_lossy().into_owned());
        format!("{} {}", name, self.args.join(" "))
    }
}

/// Result of running a `matlabc` compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileResult {
    /// Captured stdout — the generated artifact (C/C++/LLVM/…).
    pub stdout: String,
    /// Stderr lines, streamed during the run and collected here.
    pub stderr_lines: Vec<String>,
    pub exit_code: i32,
}

impl CompileResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Severity of a compiler diagnostic line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

/// A parsed clang-style diagnostic: `path:line:col: level: message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub level: DiagnosticLevel,
    pub message: String,
}

/// Parse one clang-style diagnostic line, or `None` if it isn't one.
pub fn parse_diagnostic(line: &str) -> Option<Diagnostic> {
    // Split into at most 5 pieces: file : line : col : "<level>: message".
    // File paths on Linux don't contain ':' in practice for matlabc output.
    let mut parts = line.splitn(4, ':');
    let file = parts.next()?.trim();
    let line_no: usize = parts.next()?.trim().parse().ok()?;
    let col: usize = parts.next()?.trim().parse().ok()?;
    let rest = parts.next()?.trim();
    let (level_str, message) = rest.split_once(':')?;
    let level = match level_str.trim() {
        "error" => DiagnosticLevel::Error,
        "warning" => DiagnosticLevel::Warning,
        "note" => DiagnosticLevel::Note,
        _ => return None,
    };
    if file.is_empty() {
        return None;
    }
    Some(Diagnostic {
        file: file.to_string(),
        line: line_no,
        column: col,
        level,
        message: message.trim().to_string(),
    })
}

/// Synchronous compiler runner. The app calls this off the UI thread and
/// marshals the result back; tests inject [`FakeCompilerService`].
pub trait CompilerService {
    /// Run `inv`, streaming each stderr line through `on_log`, returning the
    /// captured stdout + exit code.
    fn run(&self, inv: &CompilerInvocation, on_log: &mut dyn FnMut(&str)) -> std::io::Result<CompileResult>;
}

/// Real `matlabc` runner via `std::process::Command`.
pub struct ProcessCompilerService;

impl CompilerService for ProcessCompilerService {
    fn run(&self, inv: &CompilerInvocation, on_log: &mut dyn FnMut(&str)) -> std::io::Result<CompileResult> {
        let output = std::process::Command::new(&inv.binary).args(&inv.args).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stderr_lines: Vec<String> = stderr.lines().map(|l| l.to_string()).collect();
        for l in &stderr_lines {
            on_log(l);
        }
        Ok(CompileResult {
            stdout,
            stderr_lines,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

/// Scripted compiler for tests: returns a canned result and records the
/// invocations it was asked to run.
pub struct FakeCompilerService {
    pub result: CompileResult,
    pub calls: RefCell<Vec<CompilerInvocation>>,
}

impl FakeCompilerService {
    pub fn new(result: CompileResult) -> FakeCompilerService {
        FakeCompilerService { result, calls: RefCell::new(Vec::new()) }
    }
}

impl CompilerService for FakeCompilerService {
    fn run(&self, inv: &CompilerInvocation, on_log: &mut dyn FnMut(&str)) -> std::io::Result<CompileResult> {
        self.calls.borrow_mut().push(inv.clone());
        for l in &self.result.stderr_lines {
            on_log(l);
        }
        Ok(self.result.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_builds_argv_with_opt() {
        let inv = CompilerInvocation::emit(
            "/bin/matlabc",
            CompilerTarget::Cpp,
            OptimizationProfile::O2,
            Path::new("/p/foo.m"),
        )
        .unwrap();
        assert_eq!(inv.args, vec!["-emit-cpp", "-O", "/p/foo.m"]);
        assert_eq!(inv.command_line(), "matlabc -emit-cpp -O /p/foo.m");
    }

    #[test]
    fn emit_omits_o_flag_for_o0() {
        let inv = CompilerInvocation::emit(
            "matlabc",
            CompilerTarget::Llvm,
            OptimizationProfile::O0,
            Path::new("a.m"),
        )
        .unwrap();
        assert_eq!(inv.args, vec!["-emit-llvm", "a.m"]);
    }

    #[test]
    fn emit_none_for_verilog_a() {
        assert!(CompilerInvocation::emit(
            "matlabc",
            CompilerTarget::Va,
            OptimizationProfile::O0,
            Path::new("a.m")
        )
        .is_none());
    }

    #[test]
    fn parses_error_diagnostic() {
        let d = parse_diagnostic("/tmp/test.m:1:11: error: undefined name 'y'").unwrap();
        assert_eq!(d.file, "/tmp/test.m");
        assert_eq!((d.line, d.column), (1, 11));
        assert_eq!(d.level, DiagnosticLevel::Error);
        assert_eq!(d.message, "undefined name 'y'");
    }

    #[test]
    fn parses_warning_and_note() {
        assert_eq!(
            parse_diagnostic("a.m:2:3: warning: unused").unwrap().level,
            DiagnosticLevel::Warning
        );
        assert_eq!(
            parse_diagnostic("a.m:2:3: note: declared here").unwrap().level,
            DiagnosticLevel::Note
        );
    }

    #[test]
    fn ignores_non_diagnostic_lines() {
        assert!(parse_diagnostic("Compiling foo...").is_none());
        assert!(parse_diagnostic("a.m:x:3: error: bad").is_none());
        assert!(parse_diagnostic("a.m:1:2: bogus: msg").is_none());
    }

    #[test]
    fn process_service_captures_stdout() {
        // Use `echo` as a stand-in process to exercise the real runner.
        let svc = ProcessCompilerService;
        let inv = CompilerInvocation { binary: "echo".into(), args: vec!["hello".into()] };
        let mut logs = Vec::new();
        let res = svc.run(&inv, &mut |l| logs.push(l.to_string())).unwrap();
        assert_eq!(res.stdout.trim(), "hello");
        assert_eq!(res.exit_code, 0);
        assert!(res.success());
    }

    #[test]
    fn process_service_streams_stderr() {
        let svc = ProcessCompilerService;
        let inv = CompilerInvocation {
            binary: "sh".into(),
            args: vec!["-c".into(), "echo oops 1>&2; exit 3".into()],
        };
        let mut logs = Vec::new();
        let res = svc.run(&inv, &mut |l| logs.push(l.to_string())).unwrap();
        assert_eq!(logs, vec!["oops"]);
        assert_eq!(res.exit_code, 3);
        assert!(!res.success());
    }

    #[test]
    fn fake_records_calls_and_streams_logs() {
        let result = CompileResult {
            stdout: "int main(){}".into(),
            stderr_lines: vec!["warning: x".into()],
            exit_code: 0,
        };
        let fake = FakeCompilerService::new(result.clone());
        let inv = CompilerInvocation::emit("m", CompilerTarget::C, OptimizationProfile::O0, Path::new("a.m")).unwrap();
        let mut logs = Vec::new();
        let got = fake.run(&inv, &mut |l| logs.push(l.to_string())).unwrap();
        assert_eq!(got, result);
        assert!(got.success());
        assert_eq!(logs, vec!["warning: x"]);
        assert_eq!(fake.calls.borrow().len(), 1);
    }
}
