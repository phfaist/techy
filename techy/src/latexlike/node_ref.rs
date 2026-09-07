//! Accessors that read the preset's vocabulary off a parsed node.
//!
//! These are inherent methods on [`NodeRef`], so they are available on every node of
//! a parsed tree with no import beyond the node reference itself. There is one per
//! question a reader of a latexlike tree asks first:
//!
//! - [`macro_name`](NodeRef::macro_name),
//!   [`environment_name`](NodeRef::environment_name) and
//!   [`specials_name`](NodeRef::specials_name) — the invocation spelling of a
//!   `Callable` node, one accessor per invocation form
//!   ([`CallableType`](super::CallableType));
//! - [`is_math_group`](NodeRef::is_math_group) and
//!   [`math_form`](NodeRef::math_form) — whether a `Group` node is math, and whether
//!   it is inline or display;
//! - [`post_space`](NodeRef::post_space) — the whitespace a macro's trigger token
//!   consumed after the macro name.
//!
//! Every one of them answers `None` for a node it does not apply to, so they compose
//! as filters over [`children`](NodeRef::children) without a preceding kind test.
//!
//! The accessors are defined for every language of the latexlike family
//! ([`LatexlikeLang`], annotated trees included) and read the vocabulary through the
//! role traits, so a language with vocabulary enums of its own gets the same set.
//! The kind-generic accessors underneath them — [`name`](NodeRef::name),
//! [`group_type`](NodeRef::group_type), [`callable_type`](NodeRef::callable_type),
//! [`arguments`](NodeRef::arguments) — are documented with
//! [`NodeRef`] itself, and [Node trees](crate::guide::node_trees) covers reading a
//! tree in general.

use crate::node::NodeRef;

use super::{
    LatexlikeCallableType, LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang,
    MathGroupForm,
};

/// The preset's accessors, defined for every language of the latexlike family.
impl<'t, LLL: LatexlikeLang, A> NodeRef<'t, LLL, A> {
    /// Whether this node is a math group, in either form.
    ///
    /// `false` for a node that is not a group at all, and for a content or verbatim
    /// group. Both math forms answer `true`; use
    /// [`math_form`](NodeRef::math_form) to tell inline from display.
    ///
    /// The question asked is the group class's
    /// [`is_math`](LatexlikeGroupType::is_math), which is also what the parser keys
    /// on when it puts the group's interior into
    /// [`Mode::Math`](super::Mode::Math).
    pub fn is_math_group(&self) -> bool {
        self.group_type().is_some_and(|group_type| group_type.is_math())
    }

    /// Whether this math group is inline or display math.
    ///
    /// `Some(MathGroupForm::Inline)` for `$…$` and `\(…\)`,
    /// `Some(MathGroupForm::Display)` for `$$…$$` and `\[…\]`. `None` for every other
    /// node: a node that is not a group, a content or verbatim group, and a
    /// math-like class an extending language declared without a presentation form
    /// (see the [`is_math`/`math_form` split](LatexlikeGroupType)). A `None` here is
    /// therefore not by itself proof that the node is not math —
    /// [`is_math_group`](NodeRef::is_math_group) answers that question.
    ///
    /// The form is not deduced from the delimiters. It is the payload of the node's
    /// group class ([`GroupType::Math`](super::GroupType::Math)), stated once where
    /// the delimiter pair was registered, so this answers correctly for a pair an
    /// embedder registered or a definition introduced part-way through a document.
    /// The delimiters as written are available separately, from
    /// [`group_delimiters`](NodeRef::group_delimiters).
    ///
    /// Both forms parse identically, so nothing in the tree below a math group
    /// depends on which one this is.
    pub fn math_form(&self) -> Option<MathGroupForm> {
        self.group_type()?.math_form()
    }

    /// The macro name, when this node is a macro invocation (`\emph` → `"emph"`).
    ///
    /// The name is the spelling as written, without the escape character. `None` for
    /// every node that is not a macro-formed `Callable` — including environment and
    /// specials invocations, whose names are
    /// [`environment_name`](NodeRef::environment_name) and
    /// [`specials_name`](NodeRef::specials_name). Use [`name`](NodeRef::name) to read
    /// the spelling of a `Callable` node whatever its form.
    pub fn macro_name(&self) -> Option<&'t str> {
        if self.callable_type().is_some_and(|callable_type| callable_type.is_macro()) {
            self.name()
        } else {
            None
        }
    }

    /// The environment name, when this node is an environment invocation
    /// (`\begin{itemize}…\end{itemize}` → `"itemize"`).
    ///
    /// One node stands for the whole environment, so this is the name of the node
    /// itself, not of a `\begin` child. `None` for every node that is not an
    /// environment-formed `Callable` — including the macro invocations
    /// [`macro_name`](NodeRef::macro_name) answers for, and specials
    /// ([`specials_name`](NodeRef::specials_name)). The environment's content is its
    /// body slot, [`body`](NodeRef::body).
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

    /// The trigger spelling, when this node is a specials invocation (`~`, `---`).
    ///
    /// The spelling is the one that actually appeared in the source, not the
    /// canonical spelling under which the specials entry was registered. `None` for
    /// every node that is not a specials-formed `Callable` — see
    /// [`macro_name`](NodeRef::macro_name) and
    /// [`environment_name`](NodeRef::environment_name) for the other two invocation
    /// forms.
    pub fn specials_name(&self) -> Option<&'t str> {
        if self.callable_type().is_some_and(|callable_type| callable_type.is_specials()) {
            self.name()
        } else {
            None
        }
    }

    /// The whitespace that ended a macro's name, as text (`\emph  {x}` → `"  "`).
    ///
    /// This is the post-space of the invocation's trigger token, and nothing else:
    /// the whitespace a multi-character command name needs in order to end. Space
    /// after a single-character command such as `\\`, and space after a final
    /// argument, belongs to the surrounding content and is a sibling node, not part
    /// of the invocation. `\emph{x}` therefore answers `Some("")`.
    ///
    /// `None` for a node that is not a `Callable` at all. Environment and specials
    /// invocations are callables and answer `Some("")`: specials record no
    /// post-space, and an environment's whitespace is recorded per side in its own
    /// syntax record
    /// ([`environment_syntax`](LatexlikeInvocationSyntax::environment_syntax)).
    ///
    /// The value is read off the node's invocation-syntax payload
    /// ([`macro_syntax`](LatexlikeInvocationSyntax::macro_syntax)), which is what the
    /// source recomposer re-emits, so what this returns is what writing the tree back
    /// out will write.
    ///
    /// # Panics
    ///
    /// Panics if the payload records the post-space as a span that is not a valid
    /// `char`-boundary range of the node's own source — the same broken invariant
    /// [`chars`](NodeRef::chars) and [`group_delimiters`](NodeRef::group_delimiters)
    /// panic on, and one no parsed input can produce. Note that
    /// [`validate_tree`](crate::core::node::validate_tree) does *not* cover it: the
    /// invocation-syntax payload belongs to the language, so a program that builds
    /// callable nodes itself is responsible for the spans it records there.
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
        use crate::state::{GroupOverrides, ParsingState, ParsingStateDelta, TokenRulesOverrides};
        use crate::token::GroupRule;
        use alloc::sync::Arc;
        use alloc::vec::Vec;

        // Turbofish: the gated `groups` field is a projection through the language's
        // feature declarations, so it no longer drives type inference on its own.
        let mut groups: Vec<Arc<GroupRule<Latexlike>>> =
            default_token_rules::<Latexlike>().groups.rules;
        groups.push(Arc::new(GroupRule {
            group_type: GroupType::Math(MathGroupForm::Display),
            open: "«".into(),
            close: "»".into(),
        }));
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                groups: GroupOverrides { rules: Some(groups), ..GroupOverrides::default() },
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
