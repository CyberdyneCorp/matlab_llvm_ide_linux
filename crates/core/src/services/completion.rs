//! Identifier completion for the REPL command line and the code editor. Combines
//! a curated set of MATLAB language keywords + commonly-used built-in and
//! toolbox commands (including the GPU / Parallel Computing Toolbox the IDE
//! targets — `gpuArray`, `gather`, …) with dynamic symbols the caller supplies
//! (workspace variables, identifiers already in scope). Pure + unit-tested; the
//! GTK layer feeds in the dynamic symbols and renders/inserts the result.

/// MATLAB keywords + commonly-used built-in / toolbox commands offered for
/// completion. Sorted-and-deduped at query time, so order here is not load-bearing.
pub static MATLAB_COMMANDS: &[&str] = &[
    // control-flow keywords
    "break",
    "case",
    "catch",
    "continue",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "persistent",
    "return",
    "switch",
    "try",
    "while",
    // core built-ins
    "abs",
    "acos",
    "all",
    "angle",
    "any",
    "arrayfun",
    "asin",
    "atan",
    "atan2",
    "axis",
    "bar",
    "cat",
    "ceil",
    "cell",
    "cellfun",
    "clc",
    "clear",
    "close",
    "cos",
    "cumprod",
    "cumsum",
    "det",
    "diag",
    "diff",
    "disp",
    "dot",
    "eig",
    "error",
    "exp",
    "eye",
    "false",
    "fft",
    "figure",
    "find",
    "fix",
    "fliplr",
    "flipud",
    "floor",
    "fprintf",
    "grid",
    "hold",
    "ifft",
    "imag",
    "inv",
    "isempty",
    "isequal",
    "isnan",
    "kron",
    "legend",
    "length",
    "linspace",
    "load",
    "log",
    "log10",
    "logical",
    "magic",
    "max",
    "mean",
    "median",
    "min",
    "mod",
    "ndims",
    "norm",
    "numel",
    "ones",
    "plot",
    "plot3",
    "prod",
    "rand",
    "randi",
    "randn",
    "real",
    "repmat",
    "reshape",
    "round",
    "save",
    "scatter",
    "sign",
    "sin",
    "single",
    "size",
    "sort",
    "sprintf",
    "sqrt",
    "squeeze",
    "struct",
    "subplot",
    "sum",
    "surf",
    "tan",
    "tic",
    "title",
    "toc",
    "trace",
    "transpose",
    "true",
    "who",
    "whos",
    "xlabel",
    "ylabel",
    "zeros",
    "zlabel",
    // GPU / Parallel Computing Toolbox
    "gpuArray",
    "gather",
    "gpuDevice",
    "gpuDeviceCount",
    "existsOnGPU",
    "isgpuArray",
    "pagefun",
    "arrayfun",
    "parfor",
    "parpool",
    "gcp",
    "parfeval",
    "spmd",
    "distributed",
    "codistributed",
    "wait",
    "reset",
];

/// The identifier token immediately before `cursor` (a byte offset into `line`).
/// Returns `(start, prefix)` where `prefix == line[start..cursor]` is the run of
/// `[A-Za-z0-9_]` ending at the cursor (empty when the cursor isn't on a word).
pub fn token_at(line: &str, cursor: usize) -> (usize, &str) {
    let cursor = cursor.min(line.len());
    let head = &line[..cursor];
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .map(|(i, _)| i)
        .last()
        .unwrap_or(cursor);
    (start, &line[start..cursor])
}

fn matches(candidate: &str, lower_prefix: &str) -> bool {
    candidate.len() >= lower_prefix.len() && candidate.to_lowercase().starts_with(lower_prefix)
}

/// Completion candidates for `prefix`, ranked dynamic-symbols-first (workspace
/// variables / in-scope identifiers) then built-in commands; each group
/// case-insensitively prefix-matched, alphabetically sorted, and de-duplicated.
/// Empty when `prefix` is empty.
pub fn candidates(prefix: &str, dynamic: &[&str]) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let lp = prefix.to_lowercase();
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut out = Vec::new();

    let group = |items: &mut Vec<&str>,
                 out: &mut Vec<String>,
                 seen: &mut std::collections::BTreeSet<String>| {
        items.sort_unstable();
        for s in items.iter() {
            if seen.insert((*s).to_string()) {
                out.push((*s).to_string());
            }
        }
    };

    let mut dyn_m: Vec<&str> = dynamic
        .iter()
        .copied()
        .filter(|s| matches(s, &lp))
        .collect();
    let mut bi_m: Vec<&str> = MATLAB_COMMANDS
        .iter()
        .copied()
        .filter(|s| matches(s, &lp))
        .collect();
    group(&mut dyn_m, &mut out, &mut seen);
    group(&mut bi_m, &mut out, &mut seen);
    out
}

/// The longest common prefix of `items` (for inline Tab completion), or `None`
/// when empty.
pub fn longest_common_prefix(items: &[String]) -> Option<String> {
    let first = items.first()?;
    let mut len = first.len();
    for s in &items[1..] {
        len = first
            .char_indices()
            .zip(s.char_indices())
            .take_while(|((_, a), (_, b))| a == b)
            .map(|((i, c), _)| i + c.len_utf8())
            .last()
            .unwrap_or(0)
            .min(len);
    }
    Some(first[..len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_at_finds_the_word_before_the_cursor() {
        assert_eq!(token_at("disp(gpu", 8), (5, "gpu"));
        assert_eq!(token_at("x = Ag", 6), (4, "Ag"));
        // Cursor after a non-word char → empty prefix (no completion).
        assert_eq!(token_at("x = ", 4).1, "");
        assert_eq!(token_at("foo_bar", 7), (0, "foo_bar"));
    }

    #[test]
    fn builtins_include_gpu_and_toolbox_commands() {
        let c = candidates("gpu", &[]);
        assert!(c.iter().any(|s| s == "gpuArray"));
        assert!(c.iter().any(|s| s == "gpuDevice"));
        assert!(candidates("gat", &[]).iter().any(|s| s == "gather"));
        assert!(candidates("par", &[]).iter().any(|s| s == "parfor"));
        assert!(candidates("arr", &[]).iter().any(|s| s == "arrayfun"));
    }

    #[test]
    fn dynamic_symbols_rank_before_builtins() {
        // "A" matches workspace var Ag and builtins (abs, acos, …); Ag comes first.
        let c = candidates("A", &["Ag", "Bg"]);
        assert_eq!(c.first().map(String::as_str), Some("Ag"));
        assert!(c.iter().any(|s| s == "abs"));
        assert!(!c.iter().any(|s| s == "Bg")); // Bg doesn't start with A
    }

    #[test]
    fn case_insensitive_prefix_and_dedup() {
        // A workspace variable shadowing a builtin name appears once.
        let c = candidates("su", &["sum"]);
        assert_eq!(c.iter().filter(|s| *s == "sum").count(), 1);
        // Case-insensitive: "GPU" matches gpuArray.
        assert!(candidates("GPU", &[]).iter().any(|s| s == "gpuArray"));
        // Empty prefix → nothing.
        assert!(candidates("", &["x"]).is_empty());
    }

    #[test]
    fn common_prefix_extends_inline() {
        assert_eq!(
            longest_common_prefix(&["gather".into(), "gat".into()]),
            Some("gat".into())
        );
        assert_eq!(
            longest_common_prefix(&["gpuArray".into(), "gpuDevice".into()]),
            Some("gpu".into())
        );
        assert_eq!(longest_common_prefix(&[]), None);
    }
}
