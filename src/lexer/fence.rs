//! Fenced-block pre-pass: compile only what is inside ```` ```moxi ```` fences.
//!
//! Scripts are Markdown. Before S1, the lexer treated `#` and `>` lines as
//! comments and everything else as code, which forced every design note to be
//! a blockquote and made previews fight Markdown's own 4-space indented-code
//! rule. Now prose is prose and code lives in fences.
//!
//! # Why masking, not extraction
//!
//! The obvious implementation — concatenate the fence bodies and lex that —
//! renumbers every line, so an error in the second fence points at the wrong
//! place in the user's file. Instead this replaces every non-code line with an
//! *empty line*, one for one. Line count is identical, code lines are byte-
//! identical, so `Span` is correct by construction and there is no offset
//! table to drift. The lexer skips blank lines already; it never learns that
//! fences exist.
//!
//! # Legacy fallback
//!
//! A file containing *no fences at all* is returned untouched and compiles
//! under the old `#`/`>` comment rules, so existing scripts keep working for
//! one release.
//!
//! The condition is "no fences", not "no moxi fences", and the difference
//! matters: a documentation file whose only Moxi-looking text sits inside a
//! quoted ```` ```` ```` block has no moxi fence, and falling back to legacy
//! there would compile the very example it was quoting. Using fences at all
//! is the opt-in.

use crate::error::MoxiError;

/// Mask everything outside ```` ```moxi ```` fences, preserving line numbers.
///
/// Returns the masked source and any fence-level diagnostics. When the source
/// contains no fences at all, the source is returned unchanged (legacy mode).
pub fn preprocess(source: &str) -> (String, Vec<MoxiError>) {
    let mut out = String::with_capacity(source.len());
    let mut errors = Vec::new();

    // (fence marker, its length, is this a moxi fence, line it opened on)
    let mut open: Option<(char, usize, bool, usize)> = None;
    let mut saw_any_fence = false;

    for (idx, line) in source.lines().enumerate() {
        let lineno = idx + 1;

        match open {
            // Outside any fence: this line is prose, whatever it says.
            None => {
                if let Some((marker, len, info)) = fence_opener(line) {
                    saw_any_fence = true;
                    let is_moxi = info.eq_ignore_ascii_case("moxi");
                    open = Some((marker, len, is_moxi, lineno));
                }
                out.push('\n');
            }

            // Inside a fence: only a matching closer ends it. A shorter or
            // different-marker run is content, which is what lets a ````
            // block quote a ``` block without ending early.
            Some((marker, len, is_moxi, _)) => {
                if is_fence_closer(line, marker, len) {
                    open = None;
                    out.push('\n');
                } else if is_moxi {
                    out.push_str(line);
                    out.push('\n');
                } else {
                    out.push('\n');
                }
            }
        }
    }

    // An unterminated moxi fence would silently swallow the rest of the file.
    // Strict mode default: say so, and name the line that opened it.
    if let Some((marker, len, true, opened_at)) = open {
        errors.push(MoxiError::UnexpectedEof {
            expected: format!(
                "closing '{}' fence for the moxi block opened at line {}",
                marker.to_string().repeat(len),
                opened_at
            ),
        });
    }

    if !saw_any_fence {
        // Legacy: this file predates fences, compile it the old way.
        return (source.to_string(), errors);
    }

    (out, errors)
}

/// Recognize a fence opener, returning its marker, run length, and the first
/// word of its info string. Follows CommonMark closely enough for our purpose:
/// at least three `` ` `` or `~`, and a backtick fence's info string may not
/// itself contain a backtick.
fn fence_opener(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;

    if marker != '`' && marker != '~' {
        return None;
    }

    let len = trimmed.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return None;
    }

    // The marker is ASCII, so the char count is also the byte offset.
    let info = trimmed[len..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }

    Some((marker, len, info.split_whitespace().next().unwrap_or("")))
}

/// A closer is a run of the same marker, at least as long as the opener, with
/// nothing else on the line.
fn is_fence_closer(line: &str, marker: char, opener_len: usize) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= opener_len && trimmed.chars().all(|c| c == marker)
}

/// Convenience for callers that only want the text.
pub fn strip(source: &str) -> String {
    preprocess(source).0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headline requirement: an error inside the *second* fence must
    /// report its real line number in the original file.
    #[test]
    fn spans_are_absolute_to_the_original_file() {
        let src = "\
Some prose about palm trees.

```moxi
atom BARK { color = brown }
```

More prose, several lines of it.
Still prose.

```moxi
atom LEAF { color = green }
```
";
        let (masked, errors) = preprocess(src);
        assert!(errors.is_empty());

        let lines: Vec<&str> = masked.lines().collect();
        assert_eq!(lines.len(), src.lines().count(), "line count must be preserved");

        // Line 4 (1-indexed) is the first atom; line 11 is the second.
        assert_eq!(lines[3], "atom BARK { color = brown }");
        assert_eq!(lines[10], "atom LEAF { color = green }");
        // Fence markers and prose alike are blanked.
        assert_eq!(lines[0], "");
        assert_eq!(lines[6], "");
        assert_eq!(lines[11], "", "the closing fence is not code");
    }

    #[test]
    fn prose_outside_fences_may_look_like_moxi() {
        let src = "\
Here is how you would write it:
entity Skeleton { part Skull { shape = sphere(radius=4) } }

```moxi
atom BONE { color = ivory }
```
";
        let (masked, _) = preprocess(src);
        assert!(!masked.contains("entity Skeleton"), "prose must not compile");
        assert!(masked.contains("atom BONE"));
    }

    #[test]
    fn a_file_with_no_fence_is_returned_untouched() {
        let src = "# Heading\n> note\natom BONE { color = ivory }\n";
        let (masked, errors) = preprocess(src);
        assert_eq!(masked, src);
        assert!(errors.is_empty());
    }

    #[test]
    fn multiple_fences_concatenate_in_document_order() {
        let src = "```moxi\nfirst\n```\nprose\n```moxi\nsecond\n```\n";
        let masked = strip(src);
        let code: Vec<&str> = masked.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(code, vec!["first", "second"]);
    }

    #[test]
    fn tilde_fences_and_case_insensitive_info_strings_work() {
        let src = "~~~MOXI\natom BONE { color = ivory }\n~~~\n";
        assert!(strip(src).contains("atom BONE"));
    }

    #[test]
    fn other_languages_are_prose() {
        let src = "```rust\nfn main() {}\n```\n\n```moxi\natom BONE { color = ivory }\n```\n";
        let masked = strip(src);
        assert!(!masked.contains("fn main"));
        assert!(masked.contains("atom BONE"));
    }

    /// A longer fence may quote a shorter one — this is how documentation
    /// shows a moxi block without it being compiled. Note this file contains
    /// no *moxi* fence, only a markdown one: the legacy fallback must key on
    /// "no fences at all", or it would hand back the quoted example as code.
    #[test]
    fn a_longer_fence_quotes_a_shorter_one() {
        let src = "````markdown\n```moxi\natom NOPE { color = red }\n```\n````\n";
        let masked = strip(src);
        assert!(!masked.contains("atom NOPE"), "quoted example must not compile");
    }

    #[test]
    fn an_unterminated_moxi_fence_is_an_error() {
        let src = "```moxi\natom BONE { color = ivory }\n";
        let (_, errors) = preprocess(src);
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], MoxiError::UnexpectedEof { expected } if expected.contains("line 1")),
            "got: {:?}",
            errors[0]
        );
    }

    /// An unterminated *non-moxi* fence is just prose running to EOF — the
    /// document's problem, not the compiler's.
    #[test]
    fn an_unterminated_prose_fence_is_silent() {
        let src = "```moxi\natom BONE { color = ivory }\n```\n\n```text\ndangling\n";
        let (masked, errors) = preprocess(src);
        assert!(errors.is_empty());
        assert!(masked.contains("atom BONE"));
        assert!(!masked.contains("dangling"));
    }

    #[test]
    fn indented_fences_are_recognized() {
        let src = "  ```moxi\n  atom BONE { color = ivory }\n  ```\n";
        assert!(strip(src).contains("atom BONE"));
    }

    #[test]
    fn masking_preserves_columns_exactly() {
        let src = "prose\n```moxi\n    atom BONE { color = ivory }\n```\n";
        let masked = strip(src);
        let line = masked.lines().nth(2).unwrap();
        assert!(line.starts_with("    "), "leading whitespace must survive");
    }
}