//! Matching-bracket finder for the editor's bracket highlight. Given a character
//! offset that sits on a bracket, returns the offset of its nesting-aware match
//! (or `None`). Pure + tested; the editor view turns the offsets into highlights.

/// The offset of the bracket matching the one at `pos`, respecting nesting.
/// Returns `None` if `pos` isn't a bracket or the match is unbalanced.
pub fn matching_bracket(text: &[char], pos: usize) -> Option<usize> {
    let c = *text.get(pos)?;
    let (open, close, forward) = match c {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        '}' => ('{', '}', false),
        _ => return None,
    };
    let mut depth = 0i32;
    if forward {
        for (i, &ch) in text.iter().enumerate().skip(pos) {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    } else {
        for i in (0..=pos).rev() {
            let ch = text[i];
            if ch == close {
                depth += 1;
            } else if ch == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn matches_simple_pairs_both_directions() {
        let t = chars("f(x)");
        assert_eq!(matching_bracket(&t, 1), Some(3)); // ( -> )
        assert_eq!(matching_bracket(&t, 3), Some(1)); // ) -> (
    }

    #[test]
    fn respects_nesting() {
        let t = chars("a[b(c)d]e");
        assert_eq!(matching_bracket(&t, 1), Some(7)); // outer [ -> ]
        assert_eq!(matching_bracket(&t, 3), Some(5)); // inner ( -> )
        assert_eq!(matching_bracket(&t, 7), Some(1)); // ] -> [
    }

    #[test]
    fn mixed_bracket_kinds() {
        let t = chars("{[()]}");
        assert_eq!(matching_bracket(&t, 0), Some(5));
        assert_eq!(matching_bracket(&t, 1), Some(4));
        assert_eq!(matching_bracket(&t, 2), Some(3));
    }

    #[test]
    fn non_bracket_or_unbalanced_is_none() {
        let t = chars("a(b");
        assert_eq!(matching_bracket(&t, 0), None); // 'a'
        assert_eq!(matching_bracket(&t, 1), None); // unbalanced '('
        assert_eq!(matching_bracket(&chars(""), 0), None);
    }
}
