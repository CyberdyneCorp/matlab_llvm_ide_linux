//! Char-by-char syntax tokenizer ported from `SyntaxHighlighter.swift`. Instead
//! of building an `NSAttributedString`, it returns `Vec<TokenSpan>` (char-offset
//! ranges + a color tag) which the GTK editor applies as `GtkTextTag`s — keeping
//! all the classification logic GTK-free and unit-testable. Char offsets line up
//! with `GtkTextBuffer` iters, which are also char-based.

use std::collections::HashSet;

use crate::theme::{code, palette, Rgb};

/// Source language; selects comment delimiters, quote semantics, and word tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    Matlab,
    Cpp,
    C,
    Python,
    TypeScript,
    Verilog,
    LlvmIr,
    Mlir,
    Markdown,
    Plain,
}

impl Language {
    /// Map an `EditorTab.language` label onto a `Language` (falls back to plain).
    pub fn from_label(label: &str) -> Language {
        match label.to_lowercase().as_str() {
            "matlab" | "m" => Language::Matlab,
            "c++" | "cpp" | "cxx" | "cc" => Language::Cpp,
            "c" => Language::C,
            "header" => Language::Cpp,
            "python" | "py" => Language::Python,
            "typescript" | "ts" => Language::TypeScript,
            "verilog" | "systemverilog" | "sv" | "v" | "verilog-a" | "va" => Language::Verilog,
            "llvm ir" | "llvm" | "ll" => Language::LlvmIr,
            "mlir" => Language::Mlir,
            "markdown" | "md" => Language::Markdown,
            _ => Language::Plain,
        }
    }

    pub fn from_extension(ext: &str) -> Language {
        match ext.to_lowercase().as_str() {
            "m" => Language::Matlab,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => Language::Cpp,
            "c" | "h" => Language::C,
            "py" => Language::Python,
            "ts" => Language::TypeScript,
            "sv" | "v" | "va" => Language::Verilog,
            "ll" => Language::LlvmIr,
            "mlir" => Language::Mlir,
            "md" | "markdown" => Language::Markdown,
            _ => Language::Plain,
        }
    }
}

/// Semantic color class for a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenColor {
    Plain,
    Keyword,
    Control,
    Number,
    Str,
    Comment,
    Function,
    Identifier,
    Operator,
    /// LLVM/MLIR `@name` global — orange.
    SsaGlobal,
    /// LLVM/MLIR `%name` local — blue.
    SsaLocal,
}

impl TokenColor {
    pub fn rgb(self) -> Rgb {
        match self {
            TokenColor::Plain => code::PLAIN,
            TokenColor::Keyword => code::KEYWORD,
            TokenColor::Control => code::CONTROL,
            TokenColor::Number => code::NUMBER,
            TokenColor::Str => code::STRING,
            TokenColor::Comment => code::COMMENT,
            TokenColor::Function => code::FUNCTION,
            TokenColor::Identifier => code::IDENTIFIER,
            TokenColor::Operator => code::OPERATOR,
            TokenColor::SsaGlobal => palette::ACCENT_ORANGE,
            TokenColor::SsaLocal => palette::ACCENT_BLUE,
        }
    }

    /// GtkTextTag name (stable per color, created once per buffer).
    pub fn tag_name(self) -> &'static str {
        match self {
            TokenColor::Plain => "tok-plain",
            TokenColor::Keyword => "tok-keyword",
            TokenColor::Control => "tok-control",
            TokenColor::Number => "tok-number",
            TokenColor::Str => "tok-string",
            TokenColor::Comment => "tok-comment",
            TokenColor::Function => "tok-function",
            TokenColor::Identifier => "tok-identifier",
            TokenColor::Operator => "tok-operator",
            TokenColor::SsaGlobal => "tok-ssa-global",
            TokenColor::SsaLocal => "tok-ssa-local",
        }
    }

    pub const ALL: [TokenColor; 11] = [
        TokenColor::Plain,
        TokenColor::Keyword,
        TokenColor::Control,
        TokenColor::Number,
        TokenColor::Str,
        TokenColor::Comment,
        TokenColor::Function,
        TokenColor::Identifier,
        TokenColor::Operator,
        TokenColor::SsaGlobal,
        TokenColor::SsaLocal,
    ];
}

/// A classified character range `[start, end)` in char offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenSpan {
    pub start: usize,
    pub end: usize,
    pub color: TokenColor,
}

/// Tokenize `source` for `language`, returning colored spans in source order.
/// `Plain` language yields no spans (whole buffer keeps the default color).
pub fn highlight(source: &str, language: Language) -> Vec<TokenSpan> {
    if language == Language::Plain || language == Language::Markdown {
        return Vec::new();
    }
    let kw = keywords(language);
    let cw = control_words(language);
    let bi = builtins(language);
    let line_comment = line_comment_prefix(language);
    let block_comment = block_comment_delims(language);
    let prefixed = prefixed_identifier_starts(language);
    let support_trans = language == Language::Matlab;
    let single_quote_is_string = language == Language::Matlab || language == Language::Python;

    let s: Vec<char> = source.chars().collect();
    let n = s.len();
    let mut out = Vec::new();
    let mut i = 0;

    while i < n {
        let c = s[i];

        // Line comment
        if let Some(lc) = line_comment {
            if starts_with(&s, i, lc) {
                let start = i;
                while i < n && s[i] != '\n' {
                    i += 1;
                }
                push(&mut out, start, i, TokenColor::Comment);
                continue;
            }
        }

        // Block comment
        if let Some((open, close)) = block_comment {
            if starts_with(&s, i, open) {
                let start = i;
                i += open.chars().count();
                while i < n && !starts_with(&s, i, close) {
                    i += 1;
                }
                if i < n {
                    i += close.chars().count();
                }
                push(&mut out, start, i, TokenColor::Comment);
                continue;
            }
        }

        // Double-quoted string
        if c == '"' {
            let start = i;
            i += 1;
            while i < n && s[i] != '"' && s[i] != '\n' {
                if s[i] == '\\' && i + 1 < n {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < n {
                i += 1;
            }
            push(&mut out, start, i, TokenColor::Str);
            continue;
        }

        // Single quote: string vs transpose vs verilog literal
        if c == '\'' {
            if support_trans && is_transpose_context(&s, i) {
                push(&mut out, i, i + 1, TokenColor::Operator);
                i += 1;
                continue;
            }
            if single_quote_is_string {
                let start = i;
                i += 1;
                while i < n && s[i] != '\'' && s[i] != '\n' {
                    if language == Language::Python && s[i] == '\\' && i + 1 < n {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < n {
                    i += 1;
                }
                push(&mut out, start, i, TokenColor::Str);
                continue;
            }
            push(&mut out, i, i + 1, TokenColor::Operator);
            i += 1;
            continue;
        }

        // LLVM/MLIR prefixed identifiers (%name, @name)
        if prefixed.contains(&c) {
            let start = i;
            i += 1;
            while i < n
                && (s[i].is_alphanumeric() || s[i] == '_' || s[i] == '.')
            {
                i += 1;
            }
            let color = if c == '@' { TokenColor::SsaGlobal } else { TokenColor::SsaLocal };
            push(&mut out, start, i, color);
            continue;
        }

        // Number
        if c.is_ascii_digit() {
            let start = i;
            while i < n && is_number_char(s[i]) {
                i += 1;
            }
            push(&mut out, start, i, TokenColor::Number);
            continue;
        }

        // Identifier / keyword / builtin
        if c.is_alphabetic() || c == '_' || (c == '$' && language == Language::Verilog) {
            let start = i;
            // Verilog system tasks ($display) include the leading $.
            if c == '$' {
                i += 1;
            }
            while i < n && (s[i].is_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            let word: String = s[start..i].iter().collect();
            let color = if kw.contains(word.as_str()) {
                TokenColor::Keyword
            } else if cw.contains(word.as_str()) {
                TokenColor::Control
            } else if bi.contains(word.as_str()) {
                TokenColor::Function
            } else if is_followed_by_call(&s, i) {
                TokenColor::Function
            } else {
                TokenColor::Identifier
            };
            push(&mut out, start, i, color);
            continue;
        }

        // Operator / punctuation
        if "+-*/=<>~&|!^:;,()[]{}.".contains(c) {
            push(&mut out, i, i + 1, TokenColor::Operator);
            i += 1;
            continue;
        }

        // Whitespace / unknown — default color.
        i += 1;
    }

    out
}

fn push(out: &mut Vec<TokenSpan>, start: usize, end: usize, color: TokenColor) {
    if end > start {
        out.push(TokenSpan { start, end, color });
    }
}

fn starts_with(s: &[char], i: usize, prefix: &str) -> bool {
    let p: Vec<char> = prefix.chars().collect();
    if i + p.len() > s.len() {
        return false;
    }
    s[i..i + p.len()] == p[..]
}

/// Accepts int / float / hex / binary / scientific / matlab `i` digits.
fn is_number_char(c: char) -> bool {
    if c.is_ascii_digit() {
        return true;
    }
    matches!(
        c,
        '.' | '_' | 'x' | 'X' | 'b' | 'B' | 'e' | 'E' | 'p' | 'P' | 'i' | 'j'
    ) || c.is_ascii_hexdigit()
}

fn is_transpose_context(s: &[char], i: usize) -> bool {
    let mut j = i as isize - 1;
    while j >= 0 && (s[j as usize] == ' ' || s[j as usize] == '\t') {
        j -= 1;
    }
    if j < 0 {
        return false;
    }
    let p = s[j as usize];
    p.is_alphanumeric() || p == '_' || p == ')' || p == ']' || p == '.'
}

fn is_followed_by_call(s: &[char], i: usize) -> bool {
    let mut j = i;
    while j < s.len() && (s[j] == ' ' || s[j] == '\t') {
        j += 1;
    }
    j < s.len() && s[j] == '('
}

fn line_comment_prefix(lang: Language) -> Option<&'static str> {
    match lang {
        Language::Matlab => Some("%"),
        Language::Cpp | Language::C | Language::TypeScript | Language::Verilog | Language::Mlir => {
            Some("//")
        }
        Language::Python => Some("#"),
        Language::LlvmIr => Some(";"),
        Language::Markdown | Language::Plain => None,
    }
}

fn block_comment_delims(lang: Language) -> Option<(&'static str, &'static str)> {
    match lang {
        Language::Cpp | Language::C | Language::TypeScript | Language::Verilog | Language::Mlir => {
            Some(("/*", "*/"))
        }
        _ => None,
    }
}

fn prefixed_identifier_starts(lang: Language) -> Vec<char> {
    match lang {
        Language::LlvmIr | Language::Mlir => vec!['%', '@'],
        _ => vec![],
    }
}

fn set(words: &[&'static str]) -> HashSet<&'static str> {
    words.iter().copied().collect()
}

fn keywords(lang: Language) -> HashSet<&'static str> {
    set(match lang {
        Language::Matlab => tables::MATLAB_KEYWORDS,
        Language::Cpp | Language::C => tables::CPP_KEYWORDS,
        Language::Python => tables::PYTHON_KEYWORDS,
        Language::TypeScript => tables::TYPESCRIPT_KEYWORDS,
        Language::Verilog => tables::VERILOG_KEYWORDS,
        Language::LlvmIr => tables::LLVM_KEYWORDS,
        Language::Mlir => tables::MLIR_KEYWORDS,
        Language::Markdown | Language::Plain => &[],
    })
}

fn control_words(lang: Language) -> HashSet<&'static str> {
    set(match lang {
        Language::Matlab => tables::MATLAB_CONTROL,
        Language::Cpp | Language::C => tables::CPP_CONTROL,
        Language::Python => tables::PYTHON_CONTROL,
        Language::TypeScript => tables::TYPESCRIPT_CONTROL,
        Language::LlvmIr => tables::LLVM_CONTROL,
        Language::Mlir => tables::MLIR_CONTROL,
        Language::Verilog | Language::Markdown | Language::Plain => &[],
    })
}

fn builtins(lang: Language) -> HashSet<&'static str> {
    set(match lang {
        Language::Matlab => tables::MATLAB_BUILTINS,
        Language::Cpp | Language::C => tables::CPP_BUILTINS,
        Language::Python => tables::PYTHON_BUILTINS,
        Language::TypeScript => tables::TYPESCRIPT_BUILTINS,
        Language::Verilog => tables::VERILOG_BUILTINS,
        Language::LlvmIr | Language::Mlir | Language::Markdown | Language::Plain => &[],
    })
}

mod tables;

#[cfg(test)]
mod tests {
    use super::*;

    fn colors_at(spans: &[TokenSpan], src: &str, needle: &str) -> Option<TokenColor> {
        let chars: Vec<char> = src.chars().collect();
        let start = src.find(needle)?;
        // convert byte index to char index
        let char_start = src[..start].chars().count();
        spans
            .iter()
            .find(|s| s.start == char_start && &chars[s.start..s.end].iter().collect::<String>() == needle)
            .map(|s| s.color)
    }

    #[test]
    fn plain_language_yields_nothing() {
        assert!(highlight("anything here", Language::Plain).is_empty());
    }

    #[test]
    fn matlab_keyword_and_comment() {
        let src = "function y = f(x) % doc\nend";
        let spans = highlight(src, Language::Matlab);
        assert_eq!(colors_at(&spans, src, "function"), Some(TokenColor::Keyword));
        assert_eq!(colors_at(&spans, src, "end"), Some(TokenColor::Keyword));
        // comment span covers "% doc"
        let comment = spans.iter().find(|s| s.color == TokenColor::Comment).unwrap();
        let chars: Vec<char> = src.chars().collect();
        let text: String = chars[comment.start..comment.end].iter().collect();
        assert!(text.starts_with("% doc"));
    }

    #[test]
    fn matlab_builtin_and_call() {
        let src = "disp(myvar)\nfoo(1)";
        let spans = highlight(src, Language::Matlab);
        assert_eq!(colors_at(&spans, src, "disp"), Some(TokenColor::Function));
        // foo is colored function because it is followed by '('
        assert_eq!(colors_at(&spans, src, "foo"), Some(TokenColor::Function));
        // myvar is a plain identifier
        assert_eq!(colors_at(&spans, src, "myvar"), Some(TokenColor::Identifier));
    }

    #[test]
    fn matlab_transpose_vs_string() {
        let transpose = "a'";
        let spans = highlight(transpose, Language::Matlab);
        // the apostrophe after identifier is an operator (transpose)
        assert!(spans.iter().any(|s| s.color == TokenColor::Operator && s.start == 1));

        let string = "x = 'hi'";
        let spans = highlight(string, Language::Matlab);
        assert!(spans.iter().any(|s| s.color == TokenColor::Str));
    }

    #[test]
    fn numbers_classified() {
        let src = "x = 3.14e-2 + 0xFF";
        let spans = highlight(src, Language::Matlab);
        assert!(spans.iter().any(|s| s.color == TokenColor::Number));
    }

    #[test]
    fn cpp_block_comment_and_keyword() {
        let src = "int x; /* block */ return x;";
        let spans = highlight(src, Language::Cpp);
        assert_eq!(colors_at(&spans, src, "int"), Some(TokenColor::Keyword));
        assert_eq!(colors_at(&spans, src, "return"), Some(TokenColor::Keyword));
        let comment = spans.iter().find(|s| s.color == TokenColor::Comment).unwrap();
        let chars: Vec<char> = src.chars().collect();
        let text: String = chars[comment.start..comment.end].iter().collect();
        assert_eq!(text, "/* block */");
    }

    #[test]
    fn llvm_prefixed_identifiers() {
        let src = "%1 = call i32 @foo()";
        let spans = highlight(src, Language::LlvmIr);
        assert_eq!(colors_at(&spans, src, "%1"), Some(TokenColor::SsaLocal));
        assert_eq!(colors_at(&spans, src, "@foo"), Some(TokenColor::SsaGlobal));
        assert_eq!(colors_at(&spans, src, "call"), Some(TokenColor::Keyword));
    }

    #[test]
    fn python_hash_comment_and_keyword() {
        let src = "def f(): # c\n  return 1";
        let spans = highlight(src, Language::Python);
        assert_eq!(colors_at(&spans, src, "def"), Some(TokenColor::Keyword));
        assert!(spans.iter().any(|s| s.color == TokenColor::Comment));
    }

    #[test]
    fn verilog_system_task_is_builtin() {
        let src = "$display(\"x\");";
        let spans = highlight(src, Language::Verilog);
        assert_eq!(colors_at(&spans, src, "$display"), Some(TokenColor::Function));
    }

    #[test]
    fn language_detection() {
        assert_eq!(Language::from_label("MATLAB"), Language::Matlab);
        assert_eq!(Language::from_label("ts"), Language::TypeScript);
        assert_eq!(Language::from_label("unknown"), Language::Plain);
        assert_eq!(Language::from_extension("ll"), Language::LlvmIr);
        assert_eq!(Language::from_extension("sv"), Language::Verilog);
    }

    #[test]
    fn token_colors_have_distinct_tags() {
        let mut seen = HashSet::new();
        for c in TokenColor::ALL {
            assert!(seen.insert(c.tag_name()));
            let _ = c.rgb();
        }
    }

    #[test]
    fn spans_are_within_bounds_for_unicode() {
        let src = "x = 1; % café ☕\ny = 2;";
        let spans = highlight(src, Language::Matlab);
        let len = src.chars().count();
        for s in spans {
            assert!(s.end <= len);
            assert!(s.start < s.end);
        }
    }
}
