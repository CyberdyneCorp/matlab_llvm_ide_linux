//! Tiny helpers for spotting media a program produced. `VideoWriter` programs
//! print the file they wrote (e.g. `wrote /tmp/plot_wave.avi`); the IDE scans
//! run / REPL / debug output for such a path so it can offer to play it back.
//! Pure + tested; the GTK side handles existence checks and playback.

/// The recognised video file extensions (lowercase, without the dot).
const VIDEO_EXTS: [&str; 5] = ["mp4", "avi", "mov", "mkv", "webm"];

/// True if `name` ends in a known video extension (case-insensitive).
pub fn is_video_file(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .map(|ext| VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// The first filesystem path in `line` that looks like a video file, if any.
/// Requires a `/` so bare words like "movie.mp4 demo" in prose don't match;
/// surrounding quotes/brackets/punctuation are trimmed.
pub fn video_path_in_line(line: &str) -> Option<String> {
    for raw in line.split_whitespace() {
        let tok = raw.trim_matches(|c: char| "'\"`()[]{}<>,;:".contains(c));
        if tok.contains('/') && is_video_file(tok) {
            return Some(tok.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_video_extensions() {
        assert!(is_video_file("a.mp4"));
        assert!(is_video_file("A.AVI"));
        assert!(is_video_file("clip.webm"));
        assert!(!is_video_file("a.png"));
        assert!(!is_video_file("noext"));
    }

    #[test]
    fn finds_path_in_wrote_line() {
        assert_eq!(
            video_path_in_line("wrote /tmp/plot_wave.avi"),
            Some("/tmp/plot_wave.avi".to_string())
        );
        assert_eq!(
            video_path_in_line("'/tmp/out.mp4'"),
            Some("/tmp/out.mp4".to_string())
        );
    }

    #[test]
    fn ignores_prose_and_non_paths() {
        assert_eq!(video_path_in_line("rendering movie.mp4 now"), None); // no '/'
        assert_eq!(video_path_in_line("just some text"), None);
        assert_eq!(video_path_in_line("see /docs/readme.md"), None);
    }
}
