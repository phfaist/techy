//! [`CharsGroupArgumentParser`]: the node-staging chars-group parser
//! (ParserLibraryParity.md N4; pylatexenc's `LatexCharsGroupParser`) — a `{…}` group
//! whose **contents parse under a restricted state**, for `\label{…}`/`\cite{…}`-style
//! chars-only arguments.
//!
//! Deliberately distinct from
//! [`read_rigid_name_group`](super::read_rigid_name_group): the environment-name
//! reader is value-returning *scaffolding* (reconstructed, never recorded) — this parser **stages nodes** like any argument parser,
//! and the group's children are the argument's content (the braces are argument
//! syntax, exactly as in [`GroupArgumentParser`](super::GroupArgumentParser)'s class
//! form).
//!
//! # The restricted contents state
//!
//! Inside the group, commands and specials are **off** (a `\foo` or `~` reads as
//! ordinary characters), comments are a knob
//! ([`with_comments`](CharsGroupArgumentParser::with_comments), default on), and
//! nested groups are a knob
//! ([`with_nested_groups`](CharsGroupArgumentParser::with_nested_groups), default on)
//! — pylatexenc's `enable_macros=False, enable_specials=False, enable_math=False`
//! recipe, with one techy twist: there is no math *gate* in the core, so "math off"
//! is **data-driven** — with nested groups on, the contents keep only the base
//! state's group rules **of the entered class** (the latexlike `{…}` content class
//! keeps `{}`; the math delimiter pairs, being another class, drop away and `$` reads
//! as a plain character). With nested groups off, `enable_groups` is cleared
//! entirely: interior `{`/`}` become plain characters — and the *outer* close still
//! terminates the group, because the expected-close recognizer is ungated (the
//! verbatim-recipe precedent) — so the first close ends the group, as in pylatexenc.
//!
//! **Descent policy**: by default a nested group's
//! interior **restores the outer, unrestricted state** — the pylatexenc behavior,
//! wanted for shapes like `\cite{manual:{… \emph{Title} …}}` where the first level is
//! chars-only but braced values regain full parsing richness. The one-level-deep
//! [`ChildStateSpec`] (whose module docs name exactly this chars-except-groups case)
//! carries the revert; deeper levels inherit the outer state naturally.
//! [`with_restricted_descent`](CharsGroupArgumentParser::with_restricted_descent)
//! keeps the restriction at every depth instead (nested groups stay chars-only).
//!
//! Leading noise (whitespace, comments ahead of the `{`) scans under the **outer**
//! state per the noise-ownership doctrine — the restriction scopes the group's
//! interior only, which is why this parser exists rather than a plain state delta on
//! the whole argument ([`ArgumentSpec::parsing_state_delta`] covers the probe too).
//!
//! The argument is mandatory with **no** expression fallback: where no group of the
//! configured class opens, [`MissingMandatoryArgument`](super::MissingMandatoryArgument)
//! is diagnosed (tolerant) or aborts (strict), the argument reported absent, nothing
//! consumed.

use alloc::sync::Arc;
use core::fmt;

use crate::node::{ArgumentExt, ContentNodes};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::{Lang, ParsingStateDelta, TokenRulesOverrides};
use crate::token::TokenKind;

use super::argument_parsers::{missing_mandatory, scan_argument_noise, stage_pre_space};
use super::child_state::{ChildStateSpec, GroupChildState, InvocationChildState};
use super::{ConstructParserResult, ParseContext};

/// The chars-group argument parser (see the module docs): a mandatory group of the
/// configured class whose contents parse under a restricted state — commands and
/// specials off, comments and nested groups per the knobs.
pub struct CharsGroupArgumentParser<L: Lang> {
    group_type: L::GroupTypeId,
    comments: bool,
    nested_groups: bool,
    restricted_descent: bool,
}

impl<L: Lang> CharsGroupArgumentParser<L> {
    /// A chars-group argument delimited by any group rule of class `group_type`, with
    /// the defaults: comments on, nested groups on, nested interiors restored to the
    /// outer state.
    pub fn new(group_type: L::GroupTypeId) -> CharsGroupArgumentParser<L> {
        CharsGroupArgumentParser {
            group_type,
            comments: true,
            nested_groups: true,
            restricted_descent: false,
        }
    }

    /// Set whether comments stay recognized inside the group (default: `true`;
    /// pylatexenc's `enable_comments`).
    pub fn with_comments(mut self, comments: bool) -> Self {
        self.comments = comments;
        self
    }

    /// Set whether nested groups of the entered class stay recognized inside the
    /// group (default: `true`; pylatexenc's `enable_groups`). When `false`, interior
    /// open/close delimiters read as plain characters and the first close of the
    /// entered pairing terminates the group (module docs).
    pub fn with_nested_groups(mut self, nested_groups: bool) -> Self {
        self.nested_groups = nested_groups;
        self
    }

    /// Keep the restriction in force inside nested groups (default: `false` — nested
    /// interiors restore the outer, unrestricted state; module docs).
    pub fn with_restricted_descent(mut self, restricted_descent: bool) -> Self {
        self.restricted_descent = restricted_descent;
        self
    }

    /// The contents-restriction delta, built against `base` (the argument's state):
    /// commands/specials off, comments per knob, groups per knob — the group-rule
    /// list filtered to the entered class (the data-driven "math off"; module docs).
    fn contents_delta(
        &self,
        base: &crate::state::ParsingState<L>,
    ) -> ParsingStateDelta<L> {
        let mut rules = TokenRulesOverrides {
            enable_commands: Some(false),
            enable_specials: Some(false),
            enable_comments: Some(self.comments),
            ..TokenRulesOverrides::default()
        };
        if self.nested_groups {
            rules.groups = Some(
                base.rules()
                    .groups
                    .iter()
                    .filter(|rule| rule.group_type == self.group_type)
                    .cloned()
                    .collect(),
            );
        } else {
            rules.enable_groups = Some(false);
        }
        ParsingStateDelta::new().rules(rules)
    }
}

impl<L: Lang> ArgumentParser<L> for CharsGroupArgumentParser<L>
where
    ArgumentExt<L>: Default,
{
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        let mut noise = scan_argument_noise(cx)?;
        let open = match noise.next.clone() {
            Some(next) => match &next.kind {
                TokenKind::GroupOpen { rule, .. } if rule.group_type == self.group_type => {
                    Some((next.clone(), Arc::clone(rule)))
                }
                _ => None,
            },
            None => None,
        };
        let Some((open, rule)) = open else {
            return missing_mandatory(cx, noise, spec);
        };

        // The outer (unrestricted) state, kept for the default descent policy; the
        // restriction applies to the group's interior only.
        let outer = Arc::clone(&cx.state);
        let contents_state = cx.derived_state(&self.contents_delta(&outer))?;

        stage_pre_space(cx, &mut noise.nodes, open.pre_space)?;
        cx.tokens.move_past(&open, true);
        let child_states = if self.restricted_descent {
            ChildStateSpec::inherit()
        } else {
            // Nested interiors revert to the outer state (one level deep; deeper
            // levels inherit the outer state naturally — module docs). Invocations
            // cannot arise at the restricted first level (commands are off).
            ChildStateSpec {
                group: GroupChildState::Fixed(outer),
                invocation: InvocationChildState::Inherit,
            }
        };
        let (id, _delta) = cx.parse_group(contents_state, open.span, rule, child_states)?;

        let child_count = {
            let staged = cx.staged_nodes();
            staged.get(id).expect("the group was just staged").children().len() as u32
        };
        noise.nodes.push(id);
        Ok(Some(ParsedArgumentNodes::new(
            noise.nodes,
            ContentNodes::InChildrenOf(id, 0..child_count),
            Default::default(),
        )))
    }

    /// The argument is mandatory: absent is a diagnosed recovery, not a valid match.
    fn can_match_empty(&self) -> bool {
        false
    }
}

impl<L: Lang> fmt::Debug for CharsGroupArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CharsGroupArgumentParser")
            .field("group_type", &self.group_type)
            .field("comments", &self.comments)
            .field("nested_groups", &self.nested_groups)
            .field("restricted_descent", &self.restricted_descent)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::error::Recovery;
    use crate::latexlike::{
        argument_specs, CallableType, Latexlike, LatexlikeDriver, GroupType, MacroSpec,
    };
    use crate::extract::content_as_chars;
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A language defining `\label` with the given chars-group parser as its one
    /// argument, plus `\emph{…}` for the nested-richness tests.
    fn language(parser: CharsGroupArgumentParser<Latexlike>, recovery: Recovery) -> Language<Latexlike> {
        let mut package = Package::new("chars-group-tests");
        package.insert(
            CallableType::Macro,
            "label",
            Arc::new(MacroSpec::new(vec![Arc::new(ArgumentSpec::new(Arc::new(parser)))])),
        );
        package.insert(
            CallableType::Macro,
            "emph",
            Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
        );
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([package]),
        )
    }

    fn parser() -> CharsGroupArgumentParser<Latexlike> {
        CharsGroupArgumentParser::new(GroupType::Content)
    }

    fn parse_ok(
        parser: CharsGroupArgumentParser<Latexlike>,
        input: &str,
    ) -> crate::engine::ParseResult<Latexlike> {
        let result = language(parser, Recovery::Strict).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn label_node(result: &crate::engine::ParseResult<Latexlike>) -> NodeRef<'_, Latexlike> {
        let node = result.tree.root().child(0).expect("the label node");
        assert_eq!(node.macro_name(), Some("label"));
        node
    }

    #[test]
    fn contents_read_as_plain_characters() {
        // Commands, specials, and math delimiters are all inert inside — the exact
        // raw text is the content, and the strict parse stays clean.
        let result = parse_ok(parser(), r"\label{sec:in$t~o \emph x} rest");
        let label = label_node(&result);
        let content = label.argument_content_nodes(0).unwrap();
        assert_eq!(content_as_chars(content).unwrap(), r"sec:in$t~o \emph x");
        assert!(content.iter().all(|node| !node.is_callable()));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" rest"));
    }

    #[test]
    fn nested_groups_restore_the_outer_state_by_default() {
        // The `\cite{key:value,manual:{…}}` shape: chars at the first level, full
        // parsing richness inside braced values (pylatexenc's descent behavior).
        let result = parse_ok(parser(), r"\label{manual:{a \emph{x} b}}");
        let label = label_node(&result);
        let content: Vec<_> = label.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0].chars(), Some("manual:"));
        let braced = content[1];
        assert!(braced.is_group());
        let emph = braced.child(1).unwrap();
        assert!(emph.is_callable());
        assert_eq!(emph.macro_name(), Some("emph"));
    }

    #[test]
    fn restricted_descent_keeps_the_restriction_at_depth() {
        let result = parse_ok(
            parser().with_restricted_descent(true),
            r"\label{manual:{a \emph{x} b}}",
        );
        let label = label_node(&result);
        let braced = label.argument_content_nodes(0).unwrap().get(1).unwrap();
        assert!(braced.is_group());
        // `\emph` reads as characters; the nested `{x}` still groups (same class).
        assert!(braced.children().iter().all(|node| !node.is_callable()));
        assert!(braced.children().iter().any(|node| node.is_group()));
        assert_eq!(content_as_chars(braced.children()).unwrap(), r"a \emphx b");
    }

    #[test]
    fn nested_groups_off_ends_at_the_first_close() {
        // pylatexenc's `enable_groups=False` behavior: interior braces are plain
        // characters and the first close terminates the group.
        let result = parse_ok(parser().with_nested_groups(false), r"\label{a{b}");
        let label = label_node(&result);
        assert_eq!(
            content_as_chars(label.argument_content_nodes(0).unwrap()).unwrap(),
            "a{b"
        );
        assert_eq!(label.span().range(), 0..11);
    }

    #[test]
    fn comments_knob_gates_interior_comments() {
        // Default: `%` starts a comment inside (a node, skipped by chars flattening).
        let result = parse_ok(parser(), "\\label{a%c\nb}");
        let label = label_node(&result);
        let content = label.argument_content_nodes(0).unwrap();
        assert!(content.iter().any(|node| node.comment().is_some()));
        assert_eq!(content_as_chars(content).unwrap(), "ab");

        // Off: `%` is an ordinary character.
        let result = parse_ok(parser().with_comments(false), "\\label{a%c\nb}");
        let label = label_node(&result);
        let content = label.argument_content_nodes(0).unwrap();
        assert!(content.iter().all(|node| node.comment().is_none()));
        assert_eq!(content_as_chars(content).unwrap(), "a%c\nb");
    }

    #[test]
    fn leading_noise_scans_under_the_outer_state() {
        // The restriction scopes the interior only: a comment ahead of the `{` is
        // region noise even with interior comments off.
        let result = parse_ok(parser().with_comments(false), "\\label %n\n{x}");
        let label = label_node(&result);
        let region: Vec<_> = label.argument_nodes(0).unwrap().iter().collect();
        assert!(region.iter().any(|node| node.comment().is_some()));
        assert_eq!(
            content_as_chars(label.argument_content_nodes(0).unwrap()).unwrap(),
            "x"
        );
    }

    #[test]
    fn missing_group_is_diagnosed_with_nothing_consumed() {
        let err = language(parser(), Recovery::Strict).parse(r"\label x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        let result = language(parser(), Recovery::Tolerant).parse(r"\label x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let label = label_node(&result);
        assert!(!label.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("x"));
    }
}
