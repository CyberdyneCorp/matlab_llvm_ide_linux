//! Tiny subsequence fuzzy matcher for the command palette and quick-open.
//! `score` returns `None` when `query`'s characters don't appear in order in the
//! candidate, otherwise a score that rewards consecutive runs, word-boundary
//! hits, and earlier matches — so "of" ranks "Open Folder" above "Toggle Plots".
//! Case-insensitive, ASCII-oriented; pure + tested.

/// Score `candidate` against `query`. Higher is better; `None` = no match.
/// An empty query matches everything with a neutral score.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let cand: Vec<char> = candidate.chars().collect();
    let lower: Vec<char> = candidate.to_lowercase().chars().collect();

    let mut qi = 0usize;
    let mut total = 0i32;
    let mut run = 0i32; // current consecutive-match streak
    let mut prev_match: Option<usize> = None;

    for (i, &lc) in lower.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if lc == q[qi] {
            let mut s = 10;
            // Consecutive characters chain a growing bonus.
            if prev_match == Some(i.wrapping_sub(1)) {
                run += 1;
                s += run * 5;
            } else {
                run = 0;
            }
            // Word-boundary / camelCase boundary bonus.
            let boundary = i == 0
                || matches!(cand.get(i - 1), Some(' ' | '_' | '-' | '/' | '.'))
                || (cand.get(i).is_some_and(|c| c.is_ascii_uppercase()));
            if boundary {
                s += 8;
            }
            // Earlier matches are slightly better.
            s -= (i as i32) / 4;
            total += s;
            prev_match = Some(i);
            qi += 1;
        }
    }

    (qi == q.len()).then_some(total)
}

/// Filter `items` to those matching `query`, sorted best-first. `key` extracts
/// the text to match against; ties keep the original order (stable).
pub fn filter_sort<T, F>(query: &str, items: impl IntoIterator<Item = T>, key: F) -> Vec<T>
where
    F: Fn(&T) -> &str,
{
    let mut scored: Vec<(i32, usize, T)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(i, item)| score(query, key(&item)).map(|s| (s, i, item)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequence_only() {
        assert!(score("of", "Open Folder").is_some());
        assert!(score("opf", "Open Folder").is_some());
        assert!(score("xyz", "Open Folder").is_none());
        // out of order fails
        assert!(score("fo", "Open Folder").is_some()); // f..o exists (Folder)
        assert!(score("zzz", "abc").is_none());
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "anything"), Some(0));
        assert_eq!(score("   ", "anything"), Some(0));
    }

    #[test]
    fn prefix_and_word_boundary_outrank_scattered() {
        // "of" should prefer "Open Folder" (two word-initials) to "Toggle ... of"
        let a = score("of", "Open Folder").unwrap();
        let b = score("of", "Toggle Workspace off").unwrap();
        assert!(a > b, "a={a} b={b}");
    }

    #[test]
    fn consecutive_run_beats_gaps() {
        let consec = score("comp", "Compile").unwrap();
        let gappy = score("comp", "c o m p lex").unwrap();
        assert!(consec > gappy);
    }

    #[test]
    fn filter_sort_orders_best_first_and_drops_non_matches() {
        let items = ["Toggle Plots", "Open Folder", "Preferences", "Open Recent"];
        let out = filter_sort("open", items, |s| s);
        assert_eq!(out.len(), 2);
        assert!(out[0].starts_with("Open"));
        assert!(out.iter().all(|s| s.to_lowercase().contains('o')));
    }

    #[test]
    fn case_insensitive() {
        assert!(score("RUN", "Run program").is_some());
        assert!(score("run", "RUN PROGRAM").is_some());
    }
}
