//! `PromptContextSanitizer`: prompt-ready text without control bytes,
//! ragged spacing or stacked blank lines. Punctuation is kept.

/// Control characters become spaces, every newline variant becomes `\n`,
/// inline whitespace collapses to one space, blank-line runs collapse to one,
/// edges are trimmed. With `max_characters`, the result is cut at a word
/// boundary with a trailing `…` counted inside the cap.
pub fn sanitize(text: &str, max_characters: Option<usize>) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                folded.push('\n');
            }
            '\n' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}' => folded.push('\n'),
            c if c.is_control() || is_format(c) => folded.push(' '),
            c => folded.push(c),
        }
    }
    let mut lines: Vec<String> = Vec::new();
    let mut pending_blank = false;
    for line in folded.split('\n') {
        let collapsed = line
            .split(is_inline_whitespace)
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if collapsed.is_empty() {
            pending_blank = !lines.is_empty();
        } else {
            if pending_blank {
                lines.push(String::new());
            }
            pending_blank = false;
            lines.push(collapsed);
        }
    }
    let trimmed = lines.join("\n").trim().to_string();
    match max_characters {
        None => trimmed,
        Some(0) => String::new(),
        Some(cap) if trimmed.chars().count() <= cap => trimmed,
        Some(cap) => hard_cap(&trimmed, cap),
    }
}

/// Unicode format characters (Cf) Swift's control set also removes.
fn is_format(c: char) -> bool {
    matches!(c, '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
}

fn is_inline_whitespace(c: char) -> bool {
    c != '\n' && c.is_whitespace()
}

fn hard_cap(text: &str, cap: usize) -> String {
    let budget = cap - 1;
    if budget == 0 {
        return text.chars().take(cap).collect();
    }
    let head: String = text.chars().take(budget).collect();
    if let Some(index) = head.rfind(char::is_whitespace) {
        let word = head[..index].trim();
        if !word.is_empty() {
            return format!("{word}…");
        }
    }
    format!("{head}…")
}

/// `sanitize` with every whitespace run (newlines included) as one space.
pub fn single_line(text: &str, max_characters: usize) -> String {
    sanitize(text, Some(max_characters))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whitespace runs collapsed and trimmed.
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first `n` characters (not bytes).
pub fn prefix(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_control_bytes_spacing_and_blank_lines() {
        assert_eq!(sanitize("raw\u{7}output", None), "raw output");
        assert_eq!(sanitize("  a \t b \r\n\n\n\n c  ", None), "a b\n\nc");
        assert_eq!(sanitize("\n\nx\n", None), "x");
    }

    #[test]
    fn caps_at_a_word_boundary_with_an_ellipsis() {
        assert_eq!(sanitize("alpha beta gamma", Some(12)), "alpha beta…");
        assert_eq!(sanitize("alphabetagamma", Some(5)), "alph…");
        assert_eq!(sanitize("short", Some(10)), "short");
        assert_eq!(sanitize("x", Some(0)), "");
        assert_eq!(single_line("a\nb   c", 100), "a b c");
    }
}
