//! `NodeRef` accessor sugar for latexlike trees.
//!
//! These are **inherent methods** on `NodeRef` for any language of the latexlike
//! family (`LLL: LatexlikeLang`, annotated trees included) — the preset shares the
//! crate with `node`, so consumers need no extra import (decided at the 7.5
//! checkpoint), and the accessors read the vocabulary through the role traits, so a
//! family member with its own enums gets the same sugar. The fuller
//! extraction/view API lives with the node-tree helpers; this is the minimal
//! preset-vocabulary layer.

use crate::node::NodeRef;

use super::{
    LatexlikeCallableType, LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang,
    MathGroupForm,
};

/// Latexlike accessor sugar (preset vocabulary over the generic accessors), for
/// every language of the latexlike family.
impl<'t, LLL: LatexlikeLang, A> NodeRef<'t, LLL, A> {
    /// Whether this node is a math group (a group class answering
    /// [`is_math`](LatexlikeGroupType::is_math), any form).
    pub fn is_math_group(&self) -> bool {
        self.group_type().is_some_and(|group_type| group_type.is_math())
    }

    /// A math group's [`MathGroupForm`] — the typed class payload the delimiter rule
    /// declared at registration
    /// ([`GroupType::Math`](super::GroupType::Math)): no delimiter table, no string
    /// matching, no state lookup, correct for embedder-registered and
    /// mid-parse-minted delimiters alike. `None` for non-math nodes (and for
    /// math-like classes without a presentation form — the
    /// [`is_math`/`math_form` split](LatexlikeGroupType)).
    pub fn math_form(&self) -> Option<MathGroupForm> {
        self.group_type()?.math_form()
    }

    /// The macro name, when this node is a macro invocation (`\emph` → `"emph"`).
    pub fn macro_name(&self) -> Option<&'t str> {
        if self.callable_type().is_some_and(|callable_type| callable_type.is_macro()) {
            self.name()
        } else {
            None
        }
    }

    /// The environment name, when this node is an environment invocation
    /// (`\begin{itemize}…` → `"itemize"`).
    pub fn environment_name(&self) -> Option<&'t str> {
        if self
            .callable_type()
            .is_some_and(|callable_type| callable_type.is_environment())
        {
            self.name()
        } else {
            None
        }
    }

    /// The specials spelling, when this node is a specials invocation (`~`, `---`).
    pub fn specials_name(&self) -> Option<&'t str> {
        if self.callable_type().is_some_and(|callable_type| callable_type.is_specials()) {
            self.name()
        } else {
            None
        }
    }

    /// A `Callable` node's recorded post-space, as logical text — read off the
    /// invocation-syntax payload
    /// ([`macro_syntax`](LatexlikeInvocationSyntax::macro_syntax)): a macro-formed
    /// invocation answers **exactly its trigger token's syntactic post-space**
    /// (the name-terminating whitespace of a multi-character command; nothing
    /// beyond the token's own post-space is ever recorded — whitespace after a
    /// single-character command or a final argument is sibling/region content);
    /// environment- and specials-formed invocations answer `Some("")` (the
    /// whitespace of an environment's begin/end syntax is recorded per side in its
    /// [`environment_syntax`](LatexlikeInvocationSyntax::environment_syntax)
    /// record, and specials record no post-space). `None` for non-callables.
    pub fn post_space(&self) -> Option<&'t str> {
        let syntax = self.invocation_syntax()?;
        Some(match syntax.macro_syntax() {
            Some((_escape_char, post_space)) => post_space.resolve(self.source()),
            None => "",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::minidefs::minilatex_package;
    use super::super::test_support::{macro_package, with_packages};
    use super::super::{GroupType, Latexlike};
    use crate::engine::Language;
    use crate::error::Recovery;
    use alloc::vec::Vec;

    /// A latexlike `Language` seeded with the zero-argument macro `\emph` (shadowing
    /// minilatex's one-argument `\emph`) plus minilatex for its specials.
    fn language() -> Language<Latexlike> {
        with_packages(
            Recovery::Strict,
            [minilatex_package(), macro_package("testpkg", "emph", None)],
        )
    }

    #[test]
    fn math_form_reads_the_declared_class_payload() {
        let result = language().parse(r"$a$ $$b$$ \(c\) \[d\] {e} f").unwrap();
        let root = result.tree.root();
        let forms: Vec<Option<MathGroupForm>> =
            root.children().iter().map(|child| child.math_form()).collect();
        assert_eq!(
            forms,
            [
                Some(MathGroupForm::Inline),
                None, // whitespace chars
                Some(MathGroupForm::Display),
                None,
                Some(MathGroupForm::Inline),
                None,
                Some(MathGroupForm::Display),
                None,
                None, // {e}: a content group is not math
                None, // chars
            ]
        );
        assert!(root.child(0).unwrap().is_math_group());
        assert!(!root.child(8).unwrap().is_math_group());
    }

    #[test]
    fn math_form_answers_for_custom_registered_delimiters() {
        // The decisive math-form property: the form is declared at rule
        // registration, so an embedder-registered delimiter pair answers without any
        // preset table (the superseded delimiter-table sugar answered `None` here).
        use crate::latexlike::{default_token_rules, LatexlikeDriver};
        use crate::state::{ParsingState, ParsingStateDelta, TokenRulesOverrides};
        use crate::token::GroupRule;
        use alloc::sync::Arc;
        use alloc::vec::Vec;

        let mut groups: Vec<Arc<GroupRule<Latexlike>>> = default_token_rules().groups;
        groups.push(Arc::new(GroupRule {
            group_type: GroupType::Math(MathGroupForm::Display),
            open: "«".into(),
            close: "»".into(),
        }));
        let seed = ParsingState::<Latexlike>::lang_initial()
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                groups: Some(groups),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Strict), seed);

        let result = language.parse("a «x» b").unwrap();
        let math = result.tree.root().child(1).unwrap();
        assert!(math.is_math_group());
        assert_eq!(math.math_form(), Some(MathGroupForm::Display));
        assert_eq!(math.group_delimiters(), Some(("«", "»")));
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
        assert_eq!(chars.math_form(), None);
    }
}
