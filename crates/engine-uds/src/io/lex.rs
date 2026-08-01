//! The lexical layer of the predecessor input format (§14.2).
//!
//! Everything here is Tier 1: the accept-set is the predecessor's exactly.
//! A line is at most [`MAX_LINE`] characters — re-measured up to the first
//! `;`, so overflow entirely inside a comment is legal; everything from `;`
//! to end of line is cut before tokenising; at most [`MAX_TOKENS`] tokens per
//! line; a token opening with a double quote runs to the next quote or end of
//! line, the only way to carry a separator inside a value.

/// Maximum characters per input line, comment excluded (`MAXLINE`).
pub const MAX_LINE: usize = 1024;

/// Maximum tokens per input line (`MAXTOKS`).
pub const MAX_TOKENS: usize = 40;

/// A lexical failure on one input line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    /// The line exceeds [`MAX_LINE`] characters before its first `;`.
    LineTooLong {
        /// Character count of the offending line up to its first `;`.
        effective_len: usize,
    },
    /// The line carries more than [`MAX_TOKENS`] tokens.
    TooManyTokens,
}

/// The portion of a raw line the parser sees: everything before the first
/// `;`. A line whose first token begins with `;` is thereby empty — skipped
/// whole — and a comment may follow data on the same line.
pub fn effective_content(raw: &str) -> &str {
    match raw.find(';') {
        Some(i) => &raw[..i],
        None => raw,
    }
}

/// §14.2 line-length rule: the limit applies to the *effective* content, so
/// overflow lying entirely past a `;` is legal.
pub fn check_line_length(raw: &str) -> Result<(), LexError> {
    let effective_len = effective_content(raw).chars().count();
    if effective_len > MAX_LINE {
        return Err(LexError::LineTooLong { effective_len });
    }
    Ok(())
}

/// Tokenise one line's effective content.
///
/// Separators are spaces, tabs, and carriage returns (line feeds terminate
/// lines before this layer). A token opening with `"` runs to the next `"`
/// or the end of the line, quotes excluded from the token's content.
pub fn tokenize(content: &str) -> Result<Vec<&str>, LexError> {
    let mut tokens = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip separators.
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let token = if bytes[i] == b'"' {
            // Quoted: to the closing quote or end of line.
            let start = i + 1;
            let end = content[start..]
                .find('"')
                .map(|j| start + j)
                .unwrap_or(bytes.len());
            i = if end < bytes.len() { end + 1 } else { end };
            &content[start..end]
        } else {
            let start = i;
            while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\r') {
                i += 1;
            }
            &content[start..i]
        };
        if tokens.len() == MAX_TOKENS {
            return Err(LexError::TooManyTokens);
        }
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_overlong_line_is_refused() {
        let raw = "x".repeat(MAX_LINE + 1);
        assert_eq!(
            check_line_length(&raw),
            Err(LexError::LineTooLong {
                effective_len: MAX_LINE + 1
            })
        );
    }

    #[test]
    fn overflow_entirely_past_a_semicolon_is_legal() {
        // 1000 chars of data, then a comment pushing the raw line far past
        // the limit: the length is re-measured up to the first `;`.
        let raw = format!("{} ;{}", "x".repeat(1000), "c".repeat(2000));
        assert_eq!(check_line_length(&raw), Ok(()));
    }

    #[test]
    fn a_comment_may_follow_data_on_the_same_line() {
        let toks = tokenize(effective_content("J1  10.0  ; invert elevation")).unwrap();
        assert_eq!(toks, vec!["J1", "10.0"]);
    }

    #[test]
    fn a_line_opening_with_a_comment_is_empty() {
        let toks = tokenize(effective_content(";; header row")).unwrap();
        assert!(toks.is_empty());
    }

    #[test]
    fn a_quoted_token_carries_separators() {
        let toks = tokenize(r#"GAGE1 "My Rain File.dat" INTENSITY"#).unwrap();
        assert_eq!(toks, vec!["GAGE1", "My Rain File.dat", "INTENSITY"]);
    }

    #[test]
    fn an_unterminated_quote_runs_to_end_of_line() {
        let toks = tokenize(r#"A "runs to the end"#).unwrap();
        assert_eq!(toks, vec!["A", "runs to the end"]);
    }

    #[test]
    fn the_forty_first_token_is_refused() {
        let forty = vec!["t"; MAX_TOKENS].join(" ");
        assert_eq!(tokenize(&forty).unwrap().len(), MAX_TOKENS);
        let forty_one = vec!["t"; MAX_TOKENS + 1].join(" ");
        assert_eq!(tokenize(&forty_one), Err(LexError::TooManyTokens));
    }

    #[test]
    fn tabs_and_carriage_returns_separate_tokens() {
        let toks = tokenize("A\tB\rC").unwrap();
        assert_eq!(toks, vec!["A", "B", "C"]);
    }
}
