//! Composition root. Mirrors `MainViewModel`: owns every sub view model and the
//! service handles, and implements the cross-cutting commands (compile, run,
//! open, REPL event routing). Command logic is split into a pure "build request"
//! step and an "apply result" step so the GTK app can run the blocking process
//! off the UI thread and marshal the result back — both halves are unit-tested.

use std::path::Path;
use std::rc::Rc;

use crate::models::{CompilerTarget, ConsoleLevel, PlotFigure, PlotKind};
use crate::observable::Property;
use crate::services::compiler::{parse_diagnostic, CompileResult, CompilerInvocation, CompilerService};
use crate::services::filesystem::FileSystem;
use crate::services::sentinels::ReplEvent;
use crate::services::settings::Settings;
use crate::services::system_bridge::{Clipboard, FilePicker};

use super::{
    activity_bar::ActivityBarViewModel, appearance::AppearanceViewModel,
    breakpoints::BreakpointsViewModel, console::ConsoleViewModel, toast::ToastViewModel,
    debug::DebugViewModel, editor::EditorViewModel, layout::LayoutViewModel, plots::PlotsViewModel,
    project_explorer::ProjectExplorerViewModel, repl::ReplViewModel, search::SearchViewModel,
    status_bar::StatusBarViewModel, toolbar::ToolbarViewModel, workspace::WorkspaceViewModel,
};

pub struct MainViewModel {
    pub activity_bar: ActivityBarViewModel,
    pub layout: LayoutViewModel,
    pub toolbar: ToolbarViewModel,
    pub status_bar: StatusBarViewModel,
    pub console: ConsoleViewModel,
    pub workspace: WorkspaceViewModel,
    pub plots: PlotsViewModel,
    pub editor: EditorViewModel,
    pub project: ProjectExplorerViewModel,
    pub repl: ReplViewModel,
    pub debug: DebugViewModel,
    pub search: SearchViewModel,
    pub breakpoints: BreakpointsViewModel,
    pub appearance: AppearanceViewModel,
    pub toast: ToastViewModel,

    pub settings: Settings,
    /// A `(variable, kind)` plot requested via "Plot As" — fulfilled when the
    /// variable's value arrives over the REPL value channel.
    pub pending_plot: Property<Option<(String, PlotKind)>>,
    /// The most recent video file path seen in program output (e.g. a
    /// `VideoWriter` "wrote …mp4/.avi" line). The GTK side plays it back.
    pub last_video: Property<Option<String>>,
    fs: Rc<dyn FileSystem>,
    clipboard: Rc<dyn Clipboard>,
    picker: Rc<dyn FilePicker>,
}

impl MainViewModel {
    pub fn new(
        fs: Rc<dyn FileSystem>,
        clipboard: Rc<dyn Clipboard>,
        picker: Rc<dyn FilePicker>,
        settings: Settings,
    ) -> MainViewModel {
        MainViewModel {
            activity_bar: ActivityBarViewModel::new(),
            layout: LayoutViewModel::new(),
            toolbar: ToolbarViewModel::new(),
            status_bar: StatusBarViewModel::new(),
            console: ConsoleViewModel::new(),
            workspace: WorkspaceViewModel::new(),
            plots: PlotsViewModel::new(),
            editor: EditorViewModel::new(),
            project: ProjectExplorerViewModel::new(),
            repl: ReplViewModel::new(),
            debug: DebugViewModel::new(),
            search: SearchViewModel::new(),
            breakpoints: BreakpointsViewModel::new(),
            appearance: AppearanceViewModel::new(),
            toast: ToastViewModel::new(),
            settings,
            pending_plot: Property::new(None),
            last_video: Property::new(None),
            fs,
            clipboard,
            picker,
        }
    }

    pub fn fs(&self) -> &dyn FileSystem {
        self.fs.as_ref()
    }

    // ---- File commands -----------------------------------------------------

    /// Open a folder via the file picker (no-op if cancelled).
    pub fn open_folder_via_picker(&self) -> std::io::Result<()> {
        if let Some(path) = self.picker.open_folder() {
            self.open_folder(&path)?;
        }
        Ok(())
    }

    pub fn open_folder(&self, path: &Path) -> std::io::Result<()> {
        self.project.open_folder(self.fs.as_ref(), path)?;
        self.status_bar.set_message(format!("Opened {}", path.display()));
        Ok(())
    }

    /// Open a file in the editor and sync the status-bar language.
    pub fn open_file(&self, path: &Path) -> std::io::Result<u64> {
        let id = self.editor.open_file(self.fs.as_ref(), path)?;
        if let Some(tab) = self.editor.active_tab() {
            self.status_bar.set_language(tab.language);
        }
        Ok(id)
    }

    /// Copy text to the clipboard (e.g. "Copy Path", artifact "Copy").
    pub fn copy_to_clipboard(&self, text: &str) {
        self.clipboard.set_text(text);
    }

    // ---- Compile -----------------------------------------------------------

    /// Build the `matlabc` invocation for the active tab + toolbar target.
    /// Returns `None` when there's no saved active file, or the target is a
    /// run-to-emit lane (Verilog-A) with no emit flag.
    pub fn compile_invocation(&self) -> Option<(CompilerTarget, CompilerInvocation)> {
        let tab = self.editor.active_tab()?;
        let url = tab.url?;
        let target = self.toolbar.target.get();
        let opt = self.toolbar.optimization.get();
        let inv = CompilerInvocation::emit(&self.settings.matlabc_path, target, opt, &url)?;
        Some((target, inv))
    }

    /// Apply a finished compile: stream diagnostics to the console + problems
    /// pane, and on success store the artifact and focus its tab.
    pub fn apply_compile_result(&self, target: CompilerTarget, result: &CompileResult) {
        let mut problems = Vec::new();
        for line in &result.stderr_lines {
            if let Some(diag) = parse_diagnostic(line) {
                problems.push(diag);
            }
            self.console.log(classify_log(line), line.clone());
        }
        self.console.set_problems(problems);
        self.toolbar.last_build.set(Some(result.success()));
        if result.success() {
            self.console.set_artifact(target, result.stdout.clone());
            self.status_bar.set_message(format!("Compiled to {}", target.label()));
            self.toast.show(format!("Compiled to {}", target.label()));
        } else {
            self.status_bar.set_message(format!("Compile failed (exit {})", result.exit_code));
        }
    }

    /// Convenience: run a compile synchronously through `svc` and apply it.
    /// Returns whether it succeeded. The GTK app uses the split build/apply
    /// methods to run off-thread; this is for tests + integration.
    pub fn run_compile(&self, svc: &dyn CompilerService) -> bool {
        let Some((target, inv)) = self.compile_invocation() else {
            self.status_bar.set_message("Nothing to compile");
            return false;
        };
        self.status_bar.set_message(format!("Compiling {}…", target.label()));
        self.toolbar.is_compiling.set(true);
        let outcome = match svc.run(&inv, &mut |_| {}) {
            Ok(result) => {
                let ok = result.success();
                self.apply_compile_result(target, &result);
                ok
            }
            Err(e) => {
                self.console.log(ConsoleLevel::Error, format!("compiler error: {e}"));
                self.toolbar.last_build.set(Some(false));
                false
            }
        };
        self.toolbar.is_compiling.set(false);
        outcome
    }

    // ---- REPL routing ------------------------------------------------------

    /// Feed one REPL stdout line: transcript handling happens in the REPL VM;
    /// structured payloads are routed to the workspace / plots VMs here.
    pub fn feed_repl_line(&self, line: &str) {
        self.detect_video(line);
        if let Some(event) = self.repl.feed_line(line) {
            self.route_repl_event(event);
        }
    }

    /// Note any video file path in `line` so the GTK side can offer playback.
    fn detect_video(&self, line: &str) {
        if let Some(path) = crate::services::media::video_path_in_line(line) {
            self.last_video.set(Some(path));
        }
    }

    /// Route a line of program output captured during a **debug** session.
    /// Figure / workspace / value sentinels are extracted and routed (so
    /// `plot(...)` while debugging lands in the Plots panel, like Run/REPL),
    /// while plain text is returned for the caller to surface in the console.
    pub fn feed_debug_output(&self, line: &str) -> Option<String> {
        self.detect_video(line);
        match self.repl.consume_sentinel(line) {
            Some(ReplEvent::Console(text)) => Some(text),
            Some(event) => {
                self.route_repl_event(event);
                None
            }
            None => None,
        }
    }

    /// Request that `name` be plotted as `kind` once its value is captured.
    /// The caller is responsible for triggering the value capture (the live
    /// REPL `disp` probe).
    pub fn request_plot(&self, name: impl Into<String>, kind: PlotKind) {
        self.pending_plot.set(Some((name.into(), kind)));
    }

    /// If a "Plot As" is pending for `name`, build a line-style figure from the
    /// freshly inspected matrix and add it to the Plots panel.
    fn fulfil_pending_plot(&self, name: &str) {
        let Some((pname, kind)) = self.pending_plot.get() else { return };
        if pname != name {
            return;
        }
        if let Some(m) = self.workspace.inspected_matrix.get() {
            let ys: Vec<f64> = m.cells.iter().flatten().copied().collect();
            if !ys.is_empty() {
                let xs: Vec<f64> = (0..ys.len()).map(|i| i as f64).collect();
                let index = self.plots.figures.with(|f| f.len() as i32) + 1;
                let fig = PlotFigure::series(index, pname.clone(), kind, xs, ys).with_source(pname);
                self.plots.add(fig);
            }
        }
        self.pending_plot.set(None);
    }

    fn route_repl_event(&self, event: ReplEvent) {
        match event {
            ReplEvent::Workspace(text) => {
                self.workspace.update_from_whos(&text);
                self.workspace.live.set(true);
            }
            ReplEvent::Value(text) => {
                let name = self.workspace.selected_name.get().unwrap_or_else(|| "ans".to_string());
                self.workspace.set_matrix_from_disp(name.as_str(), &text);
                self.fulfil_pending_plot(&name);
            }
            ReplEvent::Figure { runtime_id, width, height, png } => {
                use crate::models::{PlotFigure, PlotKind};
                let mut fig = PlotFigure::series(
                    runtime_id as i32,
                    format!("Figure {runtime_id}  ·  {width}×{height} px"),
                    PlotKind::Rendered,
                    vec![],
                    vec![],
                );
                fig.png_data = Some(png);
                fig.runtime_id = Some(runtime_id);
                self.plots.upsert_runtime(fig);
            }
            ReplEvent::Console(_) => {}
        }
    }
}

fn classify_log(line: &str) -> ConsoleLevel {
    let lower = line.to_lowercase();
    if lower.contains(" error:") || lower.starts_with("error") {
        ConsoleLevel::Error
    } else if lower.contains(" warning:") || lower.starts_with("warning") {
        ConsoleLevel::Warning
    } else {
        ConsoleLevel::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConsoleTab, OptimizationProfile};
    use crate::services::compiler::FakeCompilerService;
    use crate::services::filesystem::FakeFileSystem;
    use crate::services::system_bridge::{FakeClipboard, FakeFilePicker};

    fn main_vm(fs: FakeFileSystem) -> (MainViewModel, Rc<FakeClipboard>, Rc<FakeFilePicker>) {
        let clipboard = Rc::new(FakeClipboard::new());
        let picker = Rc::new(FakeFilePicker::new());
        let settings = Settings::resolve(Some("/opt/matlabc"), None);
        let vm = MainViewModel::new(
            Rc::new(fs),
            clipboard.clone(),
            picker.clone(),
            settings,
        );
        (vm, clipboard, picker)
    }

    #[test]
    fn open_file_sets_language() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x = 1;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        assert_eq!(vm.editor.active_tab().unwrap().language, "Matlab");
        assert_eq!(vm.status_bar.state.get().language, "Matlab");
    }

    #[test]
    fn compile_invocation_uses_toolbar_and_active_tab() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x = 1;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        vm.toolbar.set_target(CompilerTarget::Llvm);
        vm.toolbar.set_optimization(OptimizationProfile::O2);
        let (target, inv) = vm.compile_invocation().unwrap();
        assert_eq!(target, CompilerTarget::Llvm);
        assert_eq!(inv.args, vec!["-emit-llvm", "-O", "/p/a.m"]);
        assert_eq!(inv.binary, std::path::PathBuf::from("/opt/matlabc"));
    }

    #[test]
    fn compile_invocation_none_without_open_file() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        assert!(vm.compile_invocation().is_none());
    }

    #[test]
    fn run_compile_success_populates_artifact() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x = 1;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        vm.toolbar.set_target(CompilerTarget::Cpp);
        let svc = FakeCompilerService::new(CompileResult {
            stdout: "int main(){}".into(),
            stderr_lines: vec![],
            exit_code: 0,
        });
        assert!(vm.run_compile(&svc));
        assert_eq!(vm.console.active_tab.get(), ConsoleTab::Cpp);
        assert_eq!(vm.console.artifacts.get().get(&ConsoleTab::Cpp).unwrap(), "int main(){}");
        assert!(vm.status_bar.state.get().message.contains("Compiled to C++"));
    }

    #[test]
    fn run_compile_failure_surfaces_diagnostics() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x = + +;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        let svc = FakeCompilerService::new(CompileResult {
            stdout: String::new(),
            stderr_lines: vec!["/p/a.m:1:5: error: bad syntax".into()],
            exit_code: 1,
        });
        assert!(!vm.run_compile(&svc));
        assert_eq!(vm.console.problems.get().len(), 1);
        assert_eq!(vm.console.problems.get()[0].line, 1);
        assert!(vm.console.messages.get().iter().any(|m| m.level == ConsoleLevel::Error));
    }

    #[test]
    fn repl_workspace_event_updates_table_and_live() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        use crate::services::sentinels::{WS_BEGIN, WS_END};
        vm.feed_repl_line(WS_BEGIN);
        vm.feed_repl_line("a  1x1  double");
        vm.feed_repl_line(WS_END);
        assert_eq!(vm.workspace.variables.get().len(), 1);
        assert!(vm.workspace.live.get());
    }

    #[test]
    fn debug_output_routes_figures_to_plots_and_text_to_caller() {
        use crate::services::sentinels::{FIG_BEGIN, FIG_END};
        let (vm, _, _) = main_vm(FakeFileSystem::new());

        // Plain program output during debug comes back for the console.
        assert_eq!(vm.feed_debug_output("result = 42").as_deref(), Some("result = 42"));
        assert_eq!(vm.plots.figures.get().len(), 0);

        // A figure emitted while debugging lands in the Plots panel (not the
        // console) — the regression this method fixes.
        assert_eq!(vm.feed_debug_output(&format!("{FIG_BEGIN} id=5 w=320 h=240")), None);
        assert_eq!(vm.feed_debug_output("QUJD"), None); // base64 payload ("ABC")
        assert_eq!(vm.feed_debug_output(FIG_END), None);
        assert_eq!(vm.plots.figures.get().len(), 1);
        assert_eq!(vm.plots.figures.get()[0].runtime_id, Some(5));
    }

    #[test]
    fn repl_value_event_builds_matrix_for_selection() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        use crate::services::sentinels::{VAL_BEGIN, VAL_END};
        vm.workspace.select("M");
        vm.feed_repl_line(VAL_BEGIN);
        vm.feed_repl_line("1 2\n3 4");
        vm.feed_repl_line("3 4");
        vm.feed_repl_line(VAL_END);
        let m = vm.workspace.inspected_matrix.get().unwrap();
        assert_eq!(m.title, "M");
    }

    #[test]
    fn plot_as_creates_figure_when_value_arrives() {
        use crate::services::sentinels::{VAL_BEGIN, VAL_END};
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        vm.workspace.select("M");
        vm.request_plot("M", crate::models::PlotKind::Bar);
        // Value capture round-trips over the REPL channel.
        vm.feed_repl_line(VAL_BEGIN);
        vm.feed_repl_line("1 2 3 4");
        vm.feed_repl_line(VAL_END);
        let figs = vm.plots.figures.get();
        assert_eq!(figs.len(), 1);
        assert_eq!(figs[0].kind, crate::models::PlotKind::Bar);
        assert_eq!(figs[0].ys.len(), 4);
        assert!(vm.pending_plot.get().is_none());
    }

    #[test]
    fn copy_to_clipboard_routes_through_bridge() {
        let (vm, clipboard, _) = main_vm(FakeFileSystem::new());
        vm.copy_to_clipboard("/p/a.m");
        assert_eq!(clipboard.last.borrow().as_deref(), Some("/p/a.m"));
    }

    struct ErroringCompiler;
    impl CompilerService for ErroringCompiler {
        fn run(&self, _inv: &CompilerInvocation, _on_log: &mut dyn FnMut(&str)) -> std::io::Result<CompileResult> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "matlabc missing"))
        }
    }

    #[test]
    fn run_compile_with_no_file_reports_nothing() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        let svc = FakeCompilerService::new(CompileResult { stdout: String::new(), stderr_lines: vec![], exit_code: 0 });
        assert!(!vm.run_compile(&svc));
        assert_eq!(vm.status_bar.state.get().message, "Nothing to compile");
    }

    #[test]
    fn run_compile_surfaces_service_error() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x=1;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        assert!(!vm.run_compile(&ErroringCompiler));
        assert!(vm.console.messages.get().iter().any(|m| m.text.contains("compiler error")));
    }

    #[test]
    fn verilog_a_target_has_no_compile_invocation() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/a.m", "x=1;");
        let (vm, _, _) = main_vm(fs);
        vm.open_file(Path::new("/p/a.m")).unwrap();
        vm.toolbar.set_target(CompilerTarget::Va);
        assert!(vm.compile_invocation().is_none());
    }

    #[test]
    fn repl_figure_event_adds_rendered_plot() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        use crate::services::sentinels::FIG_END;
        // Minimal valid base64 "AAAA" decodes to 3 zero bytes.
        vm.feed_repl_line("___MF_FIG_BEGIN___ id=2 w=10 h=20");
        vm.feed_repl_line("AAAA");
        vm.feed_repl_line(FIG_END);
        let figs = vm.plots.figures.get();
        assert_eq!(figs.len(), 1);
        assert!(figs[0].is_rendered());
        assert_eq!(figs[0].runtime_id, Some(2));
    }

    #[test]
    fn open_folder_via_picker_cancelled_is_noop() {
        let (vm, _, _) = main_vm(FakeFileSystem::new());
        // No queued folder -> picker returns None -> no error, no root.
        vm.open_folder_via_picker().unwrap();
        assert!(vm.project.root.get().is_none());
    }

    #[test]
    fn open_folder_via_picker_uses_queued_path() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/proj/a.m", "x=1;").add_dir("/proj");
        let (vm, _, picker) = main_vm(fs);
        picker.queue_open_folder("/proj");
        vm.open_folder_via_picker().unwrap();
        assert!(vm.project.root.get().is_some());
    }
}
