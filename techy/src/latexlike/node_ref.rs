//! `NodeRef` accessor sugar for latexlike trees.
//!
//! These are **inherent methods** on `NodeRef<'_, Latexlike>` — the preset shares the
//! crate with `node`, so consumers need no extra import (decided at the 7.5
//! checkpoint; an out-of-crate language attaches its sugar through an extension trait
//! instead). The fuller extraction/view API is Phase 7.8's design session; this is
//! the minimal preset-vocabulary layer.

use crate::node::NodeRef;

use super::{CallableType, GroupType, Latexlike};

/// Inline vs. display presentation of a math group — a *delimiter* fact, not a group
/// class ([`GroupType::Math`] is a single class; 7.5 checkpoint): read from the
/// node's recorded opening delimiter by [`NodeRef::math_style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathStyle {
    /// `$…$` / `\(…\)` — inline math.
    Inline,
    /// `$$…$$` / `\[…\]` — display math.
    Display,
}

/// The preset's math-delimiter pairs `(open, close, style)` — the single source of
/// truth shared by [`default_token_rules`](super::default_token_rules), which builds
/// the [`Math`](GroupType::Math) group rules from `(open, close)`, and
/// [`math_style`](NodeRef::math_style), which reads the style back off a node's
/// recorded opening delimiter. Keeping both readers on one table removes the drift
/// risk of two hand-maintained delimiter lists. Embedder-registered math delimiters
/// are not listed here, so `math_style` answers `None` for them.
pub(super) const MATH_DELIMITERS: [(&str, &str, MathStyle); 4] = [
    ("$", "$", MathStyle::Inline),
    ("$$", "$$", MathStyle::Display),
    (r"\(", r"\)", MathStyle::Inline),
    (r"\[", r"\]", MathStyle::Display),
];

/// Latexlike accessor sugar (preset vocabulary over the generic accessors).
impl<'t> NodeRef<'t, Latexlike> {
    /// Whether this node is a math group ([`GroupType::Math`]).
    pub fn is_math_group(&self) -> bool {
        self.group_type() == Some(GroupType::Math)
    }

    /// A math group's presentation style, read from its recorded opening delimiter:
    /// `$`/`\(` are [`Inline`](MathStyle::Inline), `$$`/`\[` are
    /// [`Display`](MathStyle::Display). `None` for non-math nodes — and for math
    /// groups over embedder-registered delimiters this table does not know (read
    /// [`group_delimiters`](NodeRef::group_delimiters) directly there).
    pub fn math_style(&self) -> Option<MathStyle> {
        if !self.is_math_group() {
            return None;
        }
        let (open, _close) = self.group_delimiters()?;
        MATH_DELIMITERS
            .iter()
            .find(|(delim_open, _, _)| *delim_open == open)
            .map(|&(_, _, style)| style)
    }

    /// The macro name, when this node is a macro invocation (`\emph` → `"emph"`).
    pub fn macro_name(&self) -> Option<&'t str> {
        if self.callable_type() == Some(CallableType::Macro) {
            self.name()
        } else {
            None
        }
    }

    /// The environment name, when this node is an environment invocation
    /// (`\begin{itemize}…` → `"itemize"`).
    pub fn environment_name(&self) -> Option<&'t str> {
        if self.callable_type() == Some(CallableType::Environment) {
            self.name()
        } else {
            None
        }
    }

    /// The specials spelling, when this node is a specials invocation (`~`, `---`).
    pub fn specials_name(&self) -> Option<&'t str> {
        if self.callable_type() == Some(CallableType::Specials) {
            self.name()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_support::{macro_package, strict, with_provider};
    use crate::engine::Language;
    use alloc::vec::Vec;

    /// A latexlike `Language` seeded with the zero-argument macro `\emph`.
    fn language() -> Language<Latexlike> {
        with_provider(strict(), macro_package("testpkg", "emph", None))
    }

    #[test]
    fn math_style_reads_the_recorded_delimiters() {
        let result = language().parse(r"$a$ $$b$$ \(c\) \[d\] {e} f").unwrap();
        let root = result.tree.root();
        let styles: Vec<Option<MathStyle>> =
            root.children().iter().map(|child| child.math_style()).collect();
        assert_eq!(
            styles,
            [
                Some(MathStyle::Inline),
                None, // whitespace chars
                Some(MathStyle::Display),
                None,
                Some(MathStyle::Inline),
                None,
                Some(MathStyle::Display),
                None,
                None, // {e}: a content group is not math
                None, // chars
            ]
        );
        assert!(root.child(0).unwrap().is_math_group());
        assert!(!root.child(8).unwrap().is_math_group());
    }

    #[test]
    fn callable_name_accessors_filter_by_invocation_form() {
        let result = language().parse(r"\emph ~x").unwrap();
        let root = result.tree.root();

        let emph = root.child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        assert_eq!(emph.environment_name(), None);
        assert_eq!(emph.specials_name(), None);

        let tilde = root.child(1).unwrap();
        assert_eq!(tilde.specials_name(), Some("~"));
        assert_eq!(tilde.macro_name(), None);

        // Non-callables answer None everywhere.
        let chars = root.child(2).unwrap();
        assert_eq!(chars.macro_name(), None);
        assert_eq!(chars.specials_name(), None);
        assert_eq!(chars.math_style(), None);
    }
}
