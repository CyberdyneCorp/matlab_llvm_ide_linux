//! Run pipeline: compile a `.m` source to LLVM IR, link it against
//! `libMatlabRuntime.a` with clang, and execute the result — the Linux
//! equivalent of the macOS `build_and_run.sh`, per `matlab_llvm/docs/
//! build_and_run.md`. The command builders are pure and tested; the staged
//! executor is the real impl, covered by env-gated integration tests.

use std::path::{Path, PathBuf};

/// The three commands that make up a Run, derived from a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    /// Filename stem (no extension) — the executable + IR basename.
    pub stem: String,
    /// Where the emitted LLVM IR is written.
    pub ll_path: PathBuf,
    /// Where the linked executable is written.
    pub bin_path: PathBuf,
}

impl RunPlan {
    /// Build a plan placing intermediate + output files in `out_dir`.
    pub fn new(source: &Path, out_dir: &Path) -> RunPlan {
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "program".to_string());
        RunPlan {
            ll_path: out_dir.join(format!("{stem}.ll")),
            bin_path: out_dir.join(&stem),
            stem,
        }
    }

    /// `matlabc` args that emit LLVM IR for `source` (IR goes to stdout; the
    /// executor redirects it to [`ll_path`](RunPlan::ll_path)).
    pub fn emit_args(source: &Path) -> Vec<String> {
        vec!["-emit-llvm".to_string(), source.to_string_lossy().into_owned()]
    }

    /// The clang link command: `(program, args)`. Mirrors the doc's recipe
    /// (`-std=c++20 -O2 -Wno-override-module … -ldl -lpthread -Wl,-dead_strip
    /// -o <bin>`). `extra_libs` are appended after the runtime archive (and
    /// before `-o`) so libraries the runtime pulls in — e.g. FFmpeg for
    /// `VideoWriter` — resolve in left-to-right link order.
    pub fn link_command(&self, runtime_archive: &Path, extra_libs: &[String]) -> (String, Vec<String>) {
        let mut args = vec![
            "-std=c++20".to_string(),
            "-O2".to_string(),
            "-Wno-override-module".to_string(),
            self.ll_path.to_string_lossy().into_owned(),
            runtime_archive.to_string_lossy().into_owned(),
            "-ldl".to_string(),
            "-lpthread".to_string(),
            // GNU ld dead-strip (the doc's macOS `-Wl,-dead_strip` equivalent).
            "-Wl,--gc-sections".to_string(),
        ];
        args.extend(extra_libs.iter().cloned());
        args.push("-o".to_string());
        args.push(self.bin_path.to_string_lossy().into_owned());
        ("clang++".to_string(), args)
    }
}

/// Outcome of a full Run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub stdout: String,
    pub log_lines: Vec<String>,
    pub exit_code: i32,
    /// True if compile + link + execute all succeeded.
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_derives_stem_and_paths() {
        let plan = RunPlan::new(Path::new("/proj/sine_wave.m"), Path::new("/tmp"));
        assert_eq!(plan.stem, "sine_wave");
        assert_eq!(plan.ll_path, PathBuf::from("/tmp/sine_wave.ll"));
        assert_eq!(plan.bin_path, PathBuf::from("/tmp/sine_wave"));
    }

    #[test]
    fn emit_args_match_doc() {
        assert_eq!(
            RunPlan::emit_args(Path::new("a.m")),
            vec!["-emit-llvm".to_string(), "a.m".to_string()]
        );
    }

    #[test]
    fn link_command_matches_doc_recipe() {
        let plan = RunPlan::new(Path::new("/proj/diff.m"), Path::new("/tmp"));
        let (prog, args) = plan.link_command(Path::new("/rt/libMatlabRuntime.a"), &[]);
        assert_eq!(prog, "clang++");
        assert_eq!(args[0], "-std=c++20");
        assert!(args.contains(&"-Wno-override-module".to_string()));
        assert!(args.contains(&"/rt/libMatlabRuntime.a".to_string()));
        assert!(args.contains(&"-Wl,--gc-sections".to_string()));
        // ends with -o <bin>
        assert_eq!(args[args.len() - 2], "-o");
        assert_eq!(args[args.len() - 1], "/tmp/diff");
    }

    #[test]
    fn extra_libs_land_after_the_archive_and_before_output() {
        let plan = RunPlan::new(Path::new("/proj/vid.m"), Path::new("/tmp"));
        let extra = vec!["-lavformat".to_string(), "-lavcodec".to_string()];
        let (_prog, args) = plan.link_command(Path::new("/rt/libMatlabRuntime.a"), &extra);
        let archive = args.iter().position(|a| a == "/rt/libMatlabRuntime.a").unwrap();
        let avformat = args.iter().position(|a| a == "-lavformat").unwrap();
        let out = args.iter().position(|a| a == "-o").unwrap();
        // FFmpeg libs must resolve the archive's references (after it) and come
        // before the `-o <bin>` tail.
        assert!(archive < avformat && avformat < out);
        assert_eq!(args[args.len() - 1], "/tmp/vid");
    }

    #[test]
    fn falls_back_to_program_stem_for_extensionless_source() {
        let plan = RunPlan::new(Path::new("/x/"), Path::new("/tmp"));
        assert_eq!(plan.stem, "x");
    }
}
