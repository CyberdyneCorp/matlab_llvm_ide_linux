//! Synchronous process runners used by the toolbar buttons. Compile uses the
//! core `ProcessCompilerService`; Run executes the three-stage
//! emit-llvm → clang-link → exec pipeline from `RunPlan`. These block the main
//! loop briefly (fine for the small programs the IDE compiles); streaming the
//! long-lived REPL/DAP processes off-thread is a later phase.

use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use matforge_core::models::ConsoleLevel;
use matforge_core::services::run::RunPlan;
use matforge_core::services::settings::Settings;
use matforge_core::viewmodels::MainViewModel;

use crate::process::{run_streaming, RUN_EXIT_PREFIX};

/// Compile the active tab to the selected target and apply the result.
pub fn compile(vm: &MainViewModel) {
    use matforge_core::services::compiler::ProcessCompilerService;
    vm.run_compile(&ProcessCompilerService);
}

/// Run the active tab: emit LLVM IR, link with clang, then execute streaming its
/// output live (so `drawnow` animations and long logs appear as they happen).
pub fn run(vm: Rc<MainViewModel>, settings: &Settings) {
    let Some(tab) = vm.editor.active_tab() else {
        vm.status_bar.set_message("Nothing to run");
        return;
    };
    let Some(source) = tab.url else {
        vm.status_bar.set_message("Save the file before running");
        return;
    };
    let out_dir = std::env::temp_dir();
    let plan = RunPlan::new(&source, &out_dir);
    vm.status_bar.set_message(format!("Running {}…", plan.stem));

    // 1. matlabc -emit-llvm source > stem.ll
    let emit = Command::new(&settings.matlabc_path)
        .args(RunPlan::emit_args(&source))
        .output();
    let emit = match emit {
        Ok(o) if o.status.success() => o,
        Ok(o) => return fail(&vm, &String::from_utf8_lossy(&o.stderr)),
        Err(e) => return fail(&vm, &format!("matlabc: {e}")),
    };
    if std::fs::write(&plan.ll_path, &emit.stdout).is_err() {
        return fail(&vm, "could not write LLVM IR");
    }

    // 2. clang++ … -o stem
    let (clang, args) = plan.link_command(&settings.runtime_archive);
    match Command::new(&clang).args(&args).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => return fail(&vm, &String::from_utf8_lossy(&o.stderr)),
        Err(e) => return fail(&vm, &format!("clang: {e}")),
    }

    // 3. execute, streaming each output line to the REPL transcript + figure
    //    pipeline as it is produced. MATLAB_LLVM_IDE_FIGURES makes the runtime
    //    emit ___MF_FIG_*___ sentinels, so a `plot(...); drawnow` loop animates
    //    the Plots panel live instead of only showing the final frame.
    vm.toolbar.is_running.set(true);
    let stem = plan.stem.clone();
    let started = run_streaming(&plan.bin_path, &out_dir, true, move |line| {
        if let Some(code) = line.strip_prefix(RUN_EXIT_PREFIX) {
            vm.toolbar.is_running.set(false);
            vm.status_bar.set_message(format!("Finished {stem} (exit {code})"));
        } else {
            vm.feed_repl_line(&line);
        }
    });
    if let Err(e) = started {
        // `vm` was moved into the closure; surface the spawn error on the status
        // bar via a fresh log is not possible here, so leave a console message.
        eprintln!("run: could not start {}: {e}", plan.bin_path.display());
    }
}

fn fail(vm: &MainViewModel, message: &str) {
    for line in message.lines() {
        vm.console.log(ConsoleLevel::Error, line.to_string());
    }
    vm.status_bar.set_message("Run failed");
}

/// True if the configured `matlabc` binary exists.
pub fn matlabc_available(settings: &Settings) -> bool {
    Path::new(&settings.matlabc_path).exists()
}
