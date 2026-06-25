//! Detect sim3d usage and the HTML scene files a MATLAB script exports, so the
//! IDE can open them in the 3-D Scene viewer after a run.
//!
//! `sim3d` is the compiler's MATLAB command-line 3-D API — `sim3d.World()` /
//! `sim3d.Actor(name, shape)` plus `sim3d.export(w, 'scene.html')` — which writes
//! the same self-contained Babylon.js HTML as `-emit-mflowlink-babylon`. The
//! export call is synchronous and prints no marker, so the IDE finds the produced
//! file by reading the literal path(s) the script passes to `sim3d.export`.
//!
//! Pure + tested; the GTK side resolves the path against the run directory,
//! checks freshness, and opens the viewer.

/// Filename `sim3d.export(w)` writes when called with no explicit path.
pub const DEFAULT_SCENE_FILE: &str = "sim3d_scene.html";

/// Whether `source` uses the sim3d API at all (gates the post-run scan).
pub fn uses_sim3d(source: &str) -> bool {
    source.contains("sim3d.")
}

/// The scene HTML paths a script exports, parsed from its `sim3d.export(...)`
/// calls. Returns the literal path argument (e.g. `"moving_vehicle.html"`); a
/// call with no path argument contributes the default [`DEFAULT_SCENE_FILE`].
/// Paths are returned in first-seen order, de-duplicated.
pub fn exported_scene_paths(source: &str) -> Vec<String> {
    const CALL: &str = "sim3d.export";
    let mut out: Vec<String> = Vec::new();
    let mut search = 0;
    while let Some(rel) = source[search..].find(CALL) {
        let after = search + rel + CALL.len();
        search = after;
        let Some(open) = source[after..].find('(') else {
            continue;
        };
        let args_start = after + open + 1;
        let Some(close) = source[args_start..].find(')') else {
            continue;
        };
        let args = &source[args_start..args_start + close];
        let path = first_quoted(args).unwrap_or_else(|| DEFAULT_SCENE_FILE.to_string());
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// The first single- or double-quoted string literal in `s`, unquoted.
fn first_quoted(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let q = chars[i];
        if q == '\'' || q == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != q {
                j += 1;
            }
            if j < chars.len() {
                return Some(chars[i + 1..j].iter().collect());
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sim3d_usage() {
        assert!(uses_sim3d("w = sim3d.World();"));
        assert!(uses_sim3d("a = sim3d.Actor('cube', 'box');"));
        assert!(!uses_sim3d("plot([1 2 3]); disp('hi')"));
    }

    #[test]
    fn extracts_the_literal_export_path() {
        let src = "w = sim3d.World();\nsim3d.export(w, 'moving_vehicle.html');\n";
        assert_eq!(exported_scene_paths(src), vec!["moving_vehicle.html"]);
    }

    #[test]
    fn double_quoted_paths_work() {
        let src = "sim3d.export(w, \"orbit_cube.html\")";
        assert_eq!(exported_scene_paths(src), vec!["orbit_cube.html"]);
    }

    #[test]
    fn no_path_argument_uses_the_default() {
        assert_eq!(
            exported_scene_paths("sim3d.export(w)"),
            vec![DEFAULT_SCENE_FILE]
        );
    }

    #[test]
    fn multiple_distinct_exports_are_collected_once_each() {
        let src =
            "sim3d.export(w, 'a.html'); sim3d.export(w2, 'b.html'); sim3d.export(w, 'a.html');";
        assert_eq!(exported_scene_paths(src), vec!["a.html", "b.html"]);
    }

    #[test]
    fn no_export_call_yields_nothing() {
        assert!(exported_scene_paths("w = sim3d.World(); w.run(0.1);").is_empty());
    }
}
