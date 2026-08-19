//! The scan helpers: the recognition primitives of the standard tokenizer, as free
//! functions over the text being scanned.
//!
//! A **scan helper** recognizes one construct at a byte offset in a string of content and
//! answers what it found — a *match value*, holding byte ranges
//! ([`Span`](crate::source::Span)) into that content plus the rule that matched — or
//! nothing. A helper advances no position, builds no token, and never sees a
//! [`Source`](crate::source::Source), so a reader may compose the helpers it wants and
//! build tokens of its own from their answers.
//! [`StdTokenReader::scan_std_token_at`](super::StdTokenReader::scan_std_token_at) is the
//! composition the standard reader uses, which is what keeps one implementation per
//! construct.
//!
//! `content` and `pos` mean the same thing in every helper: the text being scanned, and a
//! byte offset into it that must satisfy `pos <= content.len()` and fall on a `char`
//! boundary. A `pos` that violates this panics in all builds (each helper says so in its
//! `# Panics` section); a reader validates the offsets it receives from its caller once,
//! where it receives them, and passes derived offsets on.

use crate::state::{FeaturePresence, Lang, LangFeatures};

use super::rules::TokenRules;

/// End position of the whitespace run starting at `pos` (= `pos` if none, if
/// whitespace handling is disabled, or if the language declares it absent —
/// [`LangFeatures::Whitespace`], whose absent store holds no whitespace data at all).
///
/// A `pos` that is out of bounds for `content` or not on a `char` boundary is a
/// caller-contract violation and panics, in all builds — one of the crate's few
/// deliberate panics (see the [Panics list](techy::guide::panics)).
///
/// **The multi-newline rule** (`TokenRules::paragraphs_enabled`): skipped
/// whitespace never contains `\n\s*\n`, nor consumes a newline from such a sequence —
/// skipping stops right *before* the first newline of a paragraph break. This one
/// primitive serves pre-space, command post-space, and comment post-space, which is what
/// makes "post-space never crosses a paragraph break" hold everywhere by construction.
pub fn skip_whitespace<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>) -> usize {
    if !<L::Features as LangFeatures>::Whitespace::PRESENT || !rules.whitespace_enabled() {
        return pos;
    }
    let Some(rest) = content.get(pos..) else {
        panic!(
            "pos {} is out of bounds or not a char boundary (content len {})",
            pos,
            content.len()
        );
    };
    let ws_chars = rules.whitespace_chars();
    let mut end = pos;
    for c in rest.chars() {
        if !ws_chars.contains(c) {
            break;
        }
        if c == '\n'
            && <L::Features as LangFeatures>::Paragraphs::PRESENT
            && rules.paragraphs_enabled()
            && paragraph_continues(content, end + 1, ws_chars)
        {
            break;
        }
        end += c.len_utf8();
    }
    end
}

/// Whether another newline follows within the whitespace run starting at `after_nl`
/// (i.e. the newline just before `after_nl` opens a `\n\s*\n` paragraph sequence).
fn paragraph_continues(content: &str, after_nl: usize, ws_chars: &str) -> bool {
    for c in content[after_nl..].chars() {
        if c == '\n' {
            return true;
        }
        if !ws_chars.contains(c) {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TrivialLang;
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsRules, WhitespaceRules,
    };
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    // Group classes of the hardcoded latexlike-flavored test rules below (the test langs
    // use the `TrivialLang`-style `u32` class space). Distinct per rule, so a test can
    // look a rule up by class.
    const BRACES: u32 = 0;
    const BRACKETS: u32 = 1;
    const MATH_INLINE: u32 = 2;
    const MATH_DISPLAY: u32 = 3;
    const MATH_INLINE_PAREN: u32 = 4;
    const MATH_DISPLAY_BRACKET: u32 = 5;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestLang;
    impl TrivialLang for TestLang {}

    /// Hardcoded LaTeX-flavored rules — the same set the reader's own tests use, so a
    /// helper's answer here is comparable with the token the reader builds from it.
    fn latex_rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n\r\u{000B}\u{000C}".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![
                    group(BRACES, "{", "}"),
                    group(BRACKETS, "[", "]"),
                    group(MATH_INLINE, "$", "$"),
                    group(MATH_DISPLAY, "$$", "$$"),
                    group(MATH_INLINE_PAREN, r"\(", r"\)"),
                    group(MATH_DISPLAY_BRACKET, r"\[", r"\]"),
                ],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
                })],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![Arc::new(CommentRule { start: "%".into() })],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn group<L: Lang<GroupTypeId = u32>>(
        group_type: u32,
        open: &str,
        close: &str,
    ) -> Arc<GroupRule<L>> {
        Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
    }

    // --- skip_whitespace ---------------------------------------------------------------

    #[test]
    fn skip_whitespace_never_consumes_paragraph_newlines() {
        let rules: TokenRules<TestLang> = latex_rules();
        // Plain run (lone newline included): consumed fully.
        assert_eq!(skip_whitespace("  \n x", 0, &rules), 4);
        // Run holding a \n\s*\n sequence: stops before its first newline.
        assert_eq!(skip_whitespace("   \n  \n x", 0, &rules), 3);
        assert_eq!(skip_whitespace("\n\nx", 0, &rules), 0);
        // Flag off: everything is consumable.
        let mut no_par: TokenRules<TestLang> = latex_rules();
        no_par.paragraphs.enabled = false;
        assert_eq!(skip_whitespace("   \n  \n x", 0, &no_par), 8);
        // Whitespace handling disabled: nothing is skipped.
        let mut no_ws: TokenRules<TestLang> = latex_rules();
        no_ws.whitespace.enabled = false;
        assert_eq!(skip_whitespace("  x", 0, &no_ws), 0);
    }

    /// An invalid `pos` is a caller-contract violation and panics in all builds
    /// (the approved panic-policy exception).
    #[test]
    #[should_panic(expected = "char boundary")]
    fn skip_whitespace_panics_on_an_out_of_bounds_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = skip_whitespace("ab", 5, &rules);
    }

    /// A mid-character `pos` is the same contract violation (the boundary half).
    #[test]
    #[should_panic(expected = "char boundary")]
    fn skip_whitespace_panics_on_a_mid_char_pos() {
        let rules: TokenRules<TestLang> = latex_rules();
        let _ = skip_whitespace("é!", 1, &rules);
    }
}
