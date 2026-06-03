//! End-to-end integration tests against the real `matlabc` compiler.
//!
//! Gated on the binary existing (resolved via `$MATLABC_PATH` or the configured
//! default) so the suite skips cleanly on machines without the compiler. Run:
//!
//! ```sh
//! MATLABC_PATH=/home/leonardo/work/matlab_llvm/build/matlabc \
//!     cargo test -p matforge-core --test integration
//! ```

use std::io::Write;
use std::path::PathBuf;

use matforge_core::models::{CompilerTarget, OptimizationProfile};
use matforge_core::services::compiler::{CompilerInvocation, CompilerService, ProcessCompilerService};
use matforge_core::services::settings::Settings;

/// Resolve `matlabc`, or `None` (→ skip) if it isn't installed.
fn matlabc() -> Option<PathBuf> {
    let settings = Settings::from_env();
    settings.matlabc_path.exists().then_some(settings.matlabc_path)
}

/// Write a `.m` source to a unique temp file and return its path.
fn temp_source(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "matforge_it_{}_{}_{name}",
        std::process::id(),
        matforge_core::models::next_id()
    ));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

#[test]
fn emit_cpp_produces_source() {
    let Some(binary) = matlabc() else {
        eprintln!("skipping: matlabc not found");
        return;
    };
    let src = temp_source("hello.m", "x = 1 + 2;\ndisp(x)\n");
    let inv = CompilerInvocation::emit(&binary, CompilerTarget::Cpp, OptimizationProfile::O0, &src).unwrap();
    let result = ProcessCompilerService.run(&inv, &mut |_| {}).unwrap();
    std::fs::remove_file(&src).ok();

    assert!(result.success(), "stderr: {:?}", result.stderr_lines);
    assert!(!result.stdout.trim().is_empty(), "expected generated C++ on stdout");
}

#[test]
fn emit_llvm_contains_ir() {
    let Some(binary) = matlabc() else {
        eprintln!("skipping: matlabc not found");
        return;
    };
    let src = temp_source("ir.m", "y = 3 * 4;\n");
    let inv = CompilerInvocation::emit(&binary, CompilerTarget::Llvm, OptimizationProfile::O0, &src).unwrap();
    let result = ProcessCompilerService.run(&inv, &mut |_| {}).unwrap();
    std::fs::remove_file(&src).ok();

    assert!(result.success(), "stderr: {:?}", result.stderr_lines);
    // LLVM IR text always carries at least one `define` or a target line.
    assert!(
        result.stdout.contains("define") || result.stdout.contains("target"),
        "stdout did not look like LLVM IR:\n{}",
        &result.stdout.chars().take(200).collect::<String>()
    );
}

#[test]
fn repl_plot_emits_figure_sentinel() {
    // The basis for REPL / JIT animation: a `plot(...)` in `matlabc -repl` with
    // the IDE figures flag emits a figure-begin sentinel the Plots panel renders.
    use std::io::Read;
    use std::process::{Command, Stdio};

    let Some(binary) = matlabc() else {
        eprintln!("skipping: matlabc not found");
        return;
    };
    let mut child = Command::new(&binary)
        .arg("-repl")
        .arg("/dev/stdin")
        .env("MATLAB_LLVM_IDE_FIGURES", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn matlabc -repl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"plot(1:10);\ndrawnow;\nexit\n")
        .unwrap();
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let _ = child.wait();

    assert!(
        out.contains(matforge_core::services::sentinels::FIG_BEGIN),
        "REPL did not emit a figure sentinel for plot():\n{}",
        out.chars().take(400).collect::<String>()
    );
}

#[test]
fn diagnostics_surface_for_bad_source() {
    let Some(binary) = matlabc() else {
        eprintln!("skipping: matlabc not found");
        return;
    };
    // Undefined name should produce a clang-style diagnostic on stderr.
    let src = temp_source("bad.m", "x = 1 + + undefined_name_zzz;\n");
    let inv = CompilerInvocation::emit(&binary, CompilerTarget::Cpp, OptimizationProfile::O0, &src).unwrap();
    let mut logs = Vec::new();
    let result = ProcessCompilerService.run(&inv, &mut |l| logs.push(l.to_string())).unwrap();
    std::fs::remove_file(&src).ok();

    // Either it fails, or it emits at least one diagnostic-looking line.
    let saw_diag = result.stderr_lines.iter().any(|l| {
        matforge_core::services::compiler::parse_diagnostic(l).is_some()
    });
    assert!(
        !result.success() || saw_diag,
        "expected a failure or a diagnostic for undefined name"
    );
}
