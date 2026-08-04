//! `NodeRef` accessor sugar for latexlike trees.
//!
//! These are **inherent methods** on `NodeRef<'_, Latexlike>` — the preset shares the
//! crate with `node`, so consumers need no extra import (decided at the 7.5
//! checkpoint; an out-of-crate language attaches its sugar through an extension trait
//! instead). The fuller extraction/view API lives with the node-tree helpers; this is
//! the minimal preset-vocabulary layer.

use crate::node::NodeRef;

use super::{CallableType, GroupType, Latexlike, MathGroupForm};

/// Latexlike accessor sugar (preset vocabulary over the generic accessors).
impl<'t> NodeRef<'t, Latexlike> {
    /// Whether this node is a math group ([`GroupType::Math`], any form).
    pub fn is_math_group(&self) -> bool {
        matches!(self.group_type(), Some(GroupType::Math(_)))
    }

    /// A math group's [`MathGroupForm`] — the typed class payload the delimiter rule
    /// declared at registration ([`GroupType::Math`]): no delimiter table, no string
    /// matching, no state lookup, correct for embedder-registered and
    /// mid-parse-minted delimiters alike. `None` for non-math nodes.
    pub fn math_form(&self) -> Option<MathGroupForm> {
        match self.group_type() {
            Some(GroupType::Math(form)) => Some(form),
            _ => None,
        }
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
    use super::super::test_support::{macro_package, with_package};
    use crate::engine::Language;
    use crate::error::Recovery;
    use alloc::vec::Vec;

    /// A latexlike `Language` seeded with the zero-argument macro `\emph`.
    fn language() -> Language<Latexlike> {
        with_package(Recovery::Strict, macro_package("testpkg", "emph", None))
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
