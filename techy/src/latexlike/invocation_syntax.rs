//! The preset's invocation-syntax payload ([`Lang::InvocationSyntax`]): the
//! [`InvocationSyntax`] enum recording how a callable was invoked — macro-formed,
//! environment-formed, or specials-formed — plus the environment-side machinery:
//! the [`EnvironmentSyntax`] scanning/recording contract and its standard
//! implementation [`StdEnvironmentSyntax`] over the per-side record
//! [`EnvironmentSideSyntax`].
//!
//! The payload is what makes the preset's **recomposition accuracy** a recorded
//! fact rather than a reconstruction: a macro records its escape character and the
//! trigger token's syntactic post-space; an environment records the begin/end
//! scaffolding facts per side; a specials invocation records nothing beyond what
//! [`CallableData`](crate::node::CallableData) already carries — its `name` *is*
//! the invocation spelling as written. Recomposition reads raw node payload only,
//! so reemitting the exact input bytes needs exactly these recordings.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::constructs::{
    read_rigid_name_group, ConstructParserResult, FromInvocation, Invocation, NameGroup,
    ParseContext,
};
use crate::source::{Span, TextContent};
use crate::state::{InvocationSyntaxData, Lang};
use crate::token::{GroupRule, Token, TokenKind};

use super::lang::{LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang};
use super::Latexlike;

/// The latexlike invocation-syntax payload ([`Lang::InvocationSyntax`]): the
/// trigger-spelling facts of one callable invocation, by invocation form.
///
/// - [`Macro`](InvocationSyntax::Macro) — a command-triggered invocation: the
///   escape character as written and the trigger token's own **syntactic
///   post-space** (the name-terminating whitespace of a multi-character command,
///   pylatexenc's `macro_post_space`; nothing beyond the token's own post-space is
///   ever claimed — whitespace after a single-character command or after a final
///   argument is ordinary sibling/region content, as in TeX). A `Spanned`
///   post-space is a sub-range of the node's span: trailing for zero-argument
///   callables, between the name and the first argument region otherwise.
/// - [`Environment`](InvocationSyntax::Environment) — an environment-shaped
///   invocation: the begin/end scaffolding facts, in the `Env` record (default
///   [`StdEnvironmentSyntax`]).
/// - [`Specials`](InvocationSyntax::Specials) — a specials-formed invocation: a
///   **unit variant**, deliberately. The node's
///   [`name`](crate::node::CallableData::name) is the invocation spelling **as
///   written**, matching the macro rule (`\foo` and `\fooooo` both record the name
///   as written even when spec-resolved by prefix) — paragraph-break `Specials`
///   nodes record the actual whitespace run as `name`, and identification is by
///   **spec identity** (the canonical
///   [`ParagraphBreakSpec`](super::ParagraphBreakSpec) object), never by a
///   canonical name spelling.
///
/// The `Env` parameter is the single customization entry for environment-syntax
/// recording: a language family member picks its record type by choosing its
/// [`Lang::InvocationSyntax`] (e.g.
/// `InvocationSyntax<StdEnvironmentSyntax<Flm>>`); the default anchors at the
/// preset lang ([`Latexlike`]). A same-record/different-tolerance variant is a
/// newtype over [`StdEnvironmentSyntax`].
///
/// [`Lang::InvocationSyntax`]: crate::state::Lang::InvocationSyntax
#[derive(Clone, Debug)]
pub enum InvocationSyntax<Env = StdEnvironmentSyntax<Latexlike>> {
    /// A command-triggered (macro-formed) invocation's spelling facts.
    Macro {
        /// The escape character as written (`\` in `\frac`; a language with
        /// several command rules records whichever fired).
        escape_char: char,
        /// The trigger token's own syntactic post-space (see the enum docs) —
        /// span-backed when parsed, owned after
        /// [`materialize`](crate::node::NodeTree::materialize).
        post_space: TextContent,
    },
    /// An environment-shaped invocation's begin/end scaffolding facts.
    Environment(Env),
    /// A specials-formed invocation: nothing to record beyond the node's `name`,
    /// which is the spelling as written (see the enum docs).
    Specials,
}

impl<Env: InvocationSyntaxData> InvocationSyntaxData for InvocationSyntax<Env> {
    fn materialized(&self, source_content: &str) -> Self {
        match self {
            InvocationSyntax::Macro { escape_char, post_space } => InvocationSyntax::Macro {
                escape_char: *escape_char,
                post_space: post_space.materialized(source_content),
            },
            InvocationSyntax::Environment(env) => {
                InvocationSyntax::Environment(env.materialized(source_content))
            }
            InvocationSyntax::Specials => InvocationSyntax::Specials,
        }
    }
}

/// The standard-site constructor ([`FromInvocation`]): a
/// [`Command`](TokenKind::Command) trigger records its
/// [`Macro`](InvocationSyntax::Macro) facts straight off the token; every other
/// trigger (a specials token, a paragraph-break token at the preset's specials
/// site) records [`Specials`](InvocationSyntax::Specials). The
/// [`Environment`](InvocationSyntax::Environment) arm is never minted here —
/// environment-shaped composition stages through the canonical
/// [`stage_node`](crate::constructs::ParseContext::stage_node) door with
/// [`environment_form`](LatexlikeInvocationSyntax::environment_form).
impl<L: Lang, Env> FromInvocation<L> for InvocationSyntax<Env> {
    fn from_invocation(invocation: &Invocation<'_, '_, L>) -> Self {
        match &invocation.token.kind {
            TokenKind::Command { escape_char, post_space, .. } => InvocationSyntax::Macro {
                escape_char: *escape_char,
                post_space: TextContent::Spanned(*post_space),
            },
            _ => InvocationSyntax::Specials,
        }
    }
}

impl<LLL: LatexlikeLang, Env: EnvironmentSyntax<LLL>> LatexlikeInvocationSyntax<LLL>
    for InvocationSyntax<Env>
{
    type Env = Env;

    fn macro_form(escape_char: char, post_space: TextContent) -> Self {
        InvocationSyntax::Macro { escape_char, post_space }
    }

    fn environment_form(env: Env) -> Self {
        InvocationSyntax::Environment(env)
    }

    fn specials_form() -> Self {
        InvocationSyntax::Specials
    }

    fn macro_syntax(&self) -> Option<(char, &TextContent)> {
        match self {
            InvocationSyntax::Macro { escape_char, post_space } => {
                Some((*escape_char, post_space))
            }
            _ => None,
        }
    }

    fn environment_syntax(&self) -> Option<&Env> {
        match self {
            InvocationSyntax::Environment(env) => Some(env),
            _ => None,
        }
    }

    fn is_specials(&self) -> bool {
        matches!(self, InvocationSyntax::Specials)
    }
}

/// One side of an environment's recorded scaffolding — the spelling facts of a
/// `\begin{name}`-shaped or `\end{name}`-shaped command-plus-name-group, as
/// written:
///
/// the escape character, the command word (`begin`/`end` as written), the command
/// token's own syntactic post-space (`\begin {itemize}`'s tolerated inline
/// whitespace — recorded, no longer normalized away), and the **name-group rule**
/// — the [`GroupRule`] `Arc` cloned from the matched token, whose `open`/`close`
/// strings are the exact delimiter bytes as written (a malformed begin takes the
/// chars-recovery path, so a recorded name group never exists in
/// delimiter-diverged form) *and* which records the group's class, which byte
/// recording would lose. The rule `Arc` is source-independent, hence exempt from
/// materialization. The environment's *name* is not here — it is the node's
/// [`name`](crate::node::CallableData::name).
pub struct EnvironmentSideSyntax<L: Lang> {
    /// The escape character as written.
    pub escape_char: char,
    /// The command word as written (`begin`, `end`), sans escape character.
    pub command_word: TextContent,
    /// The command token's own syntactic post-space (between the command word and
    /// the name group; empty when the name group follows immediately).
    pub post_space: TextContent,
    /// The name group's rule — the `Arc` off the matched token (exact delimiter
    /// bytes + group class; source-independent).
    pub name_group_rule: Arc<GroupRule<L>>,
}

impl<L: Lang> EnvironmentSideSyntax<L> {
    /// Resolve this side's spelling around `name` (the environment name as
    /// written): escape char + command word + post-space + open delimiter + name +
    /// close delimiter. `source_content` resolves the span-backed fields.
    fn write(&self, name: &str, source_content: &str) -> String {
        format!(
            "{}{}{}{}{}{}",
            self.escape_char,
            self.command_word.resolve(source_content),
            self.post_space.resolve(source_content),
            self.name_group_rule.open,
            name,
            self.name_group_rule.close,
        )
    }

    fn materialized(&self, source_content: &str) -> EnvironmentSideSyntax<L> {
        EnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.materialized(source_content),
            post_space: self.post_space.materialized(source_content),
            // Source-independent — exempt from materialization.
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> Clone for EnvironmentSideSyntax<L> {
    fn clone(&self) -> Self {
        EnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.clone(),
            post_space: self.post_space.clone(),
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> fmt::Debug for EnvironmentSideSyntax<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvironmentSideSyntax")
            .field("escape_char", &self.escape_char)
            .field("command_word", &self.command_word)
            .field("post_space", &self.post_space)
            .field("name_group_rule", &self.name_group_rule)
            .finish()
    }
}

/// The environment-syntax contract of an `Env` payload type (the
/// [`Environment`](InvocationSyntax::Environment) arm): **scanning and payload
/// construction, consolidated** in the accumulator shape — [`parse_begin`] scans
/// the begin-side scaffolding and returns the accumulator with the begin side
/// filled and the end side empty; [`parse_end`] fills the end side from the facts
/// the body parser (the terminator consumer) reported back; the intermediate
/// state doubles as the synthesis constructor's shape. The type also owns its own
/// re-emission ([`write_begin`]/[`write_end`]) — the accuracy doctrine made
/// literal: what `parse_begin`/`parse_end` record is exactly what the writers
/// emit.
///
/// The data bounds and materialization come from the [`InvocationSyntaxData`]
/// supertrait (the name-group rule `Arc` is source-independent and exempt).
///
/// # The verbatim caveat
///
/// A raw (verbatim) body consumes its terminator as **one literal token** — the
/// whole `\end{name}` spelling is the expected-close string — so no tokenized end
/// scan exists to delegate: that path records standard-shaped end facts via the
/// one std-facts method the trait keeps, [`record_std_end_facts`].
///
/// [`parse_begin`]: EnvironmentSyntax::parse_begin
/// [`parse_end`]: EnvironmentSyntax::parse_end
/// [`write_begin`]: EnvironmentSyntax::write_begin
/// [`write_end`]: EnvironmentSyntax::write_end
/// [`record_std_end_facts`]: EnvironmentSyntax::record_std_end_facts
pub trait EnvironmentSyntax<L: LatexlikeLang>: InvocationSyntaxData {
    /// The begin-side scan, entered at the reader position right past the consumed
    /// begin trigger (`trigger`): read the environment's name scaffolding per this
    /// type's syntax — the standard record reads the **rigid** name group
    /// ([`read_rigid_name_group`]) — and return the name info the composition
    /// needs plus the accumulator (begin side filled, end side empty).
    ///
    /// `Ok(None)` = no name scaffolding at the position (malformed begin);
    /// **nothing is consumed** and the composition applies its own recovery.
    fn parse_begin(
        cx: &mut ParseContext<'_, '_, L>,
        trigger: &Token<'_, L>,
    ) -> ConstructParserResult<L, Option<(NameGroup<L>, Self)>>;

    /// Fill the end side from the tokenized terminator facts the body parser
    /// reported back
    /// ([`EnvironmentTerminatorFacts::Scanned`](crate::constructs::EnvironmentTerminatorFacts)).
    fn parse_end(&mut self, end: EnvironmentSideSyntax<L>);

    /// The **one std-facts method** — the verbatim path (see the trait docs):
    /// record a standard-shaped end side synthesized from the begin side's facts
    /// (same escape character and name-group rule, empty post-space) and
    /// `command_word` (the terminator's command word, `end`).
    fn record_std_end_facts(&mut self, command_word: &str);

    /// The begin-side spelling as recorded, resolved around `name` (the
    /// environment's name as written); `source_content` resolves span-backed
    /// fields. What a source recomposer emits for the begin scaffolding.
    fn write_begin(&self, name: &str, source_content: &str) -> String;

    /// The end-side spelling as recorded — the empty string when the end side is
    /// empty (the body closed without consuming a terminator: reemitting nothing
    /// reproduces the recovered input).
    fn write_end(&self, name: &str, source_content: &str) -> String;
}

/// The standard environment-syntax record: strict rigid scanning (the begin
/// composition's canonical shape), per-side facts in
/// [`EnvironmentSideSyntax`] — begin always present, end filled by
/// [`parse_end`](EnvironmentSyntax::parse_end) /
/// [`record_std_end_facts`](EnvironmentSyntax::record_std_end_facts) (or left
/// empty on the recovery paths: mismatch, malformed terminator, end of input).
pub struct StdEnvironmentSyntax<L: Lang> {
    /// The `\begin{name}` side's facts.
    pub begin: EnvironmentSideSyntax<L>,
    /// The `\end{name}` side's facts; `None` until the terminator is consumed —
    /// and permanently for a body that closed without one.
    pub end: Option<EnvironmentSideSyntax<L>>,
}

impl<L: Lang> InvocationSyntaxData for StdEnvironmentSyntax<L> {
    fn materialized(&self, source_content: &str) -> Self {
        StdEnvironmentSyntax {
            begin: self.begin.materialized(source_content),
            end: self.end.as_ref().map(|end| end.materialized(source_content)),
        }
    }
}

impl<L: LatexlikeLang> EnvironmentSyntax<L> for StdEnvironmentSyntax<L> {
    /// The rigid begin scan: the name group must be the immediately next token, of
    /// the language's [content class](LatexlikeGroupType::content_group)
    /// ([`read_rigid_name_group`]'s contract); the begin side records the trigger
    /// token's facts and the matched name-group rule.
    fn parse_begin(
        cx: &mut ParseContext<'_, '_, L>,
        trigger: &Token<'_, L>,
    ) -> ConstructParserResult<L, Option<(NameGroup<L>, Self)>> {
        let Some(name_group) = read_rigid_name_group(cx, L::GroupTypeId::content_group())?
        else {
            return Ok(None);
        };
        let (escape_char, post_space) = match &trigger.kind {
            TokenKind::Command { escape_char, post_space, .. } => (*escape_char, *post_space),
            // The begin composition dispatches off a command token; a non-command
            // trigger (custom composition) records a degenerate empty spelling.
            _ => ('\u{0}', Span::empty(trigger.span.end())),
        };
        let begin = EnvironmentSideSyntax {
            escape_char,
            command_word: TextContent::Spanned(Span::new(
                trigger.span.start() + escape_char.len_utf8(),
                post_space.start(),
            )),
            post_space: TextContent::Spanned(post_space),
            name_group_rule: Arc::clone(&name_group.rule),
        };
        let syntax = StdEnvironmentSyntax { begin, end: None };
        Ok(Some((name_group, syntax)))
    }

    fn parse_end(&mut self, end: EnvironmentSideSyntax<L>) {
        self.end = Some(end);
    }

    /// Standard end facts off the begin side: same escape character and name-group
    /// rule, owned `command_word`, empty post-space — exactly the spelling the
    /// verbatim path's literal terminator was composed from.
    fn record_std_end_facts(&mut self, command_word: &str) {
        self.end = Some(EnvironmentSideSyntax {
            escape_char: self.begin.escape_char,
            command_word: TextContent::from(String::from(command_word)),
            post_space: TextContent::empty(),
            name_group_rule: Arc::clone(&self.begin.name_group_rule),
        });
    }

    fn write_begin(&self, name: &str, source_content: &str) -> String {
        self.begin.write(name, source_content)
    }

    fn write_end(&self, name: &str, source_content: &str) -> String {
        match &self.end {
            Some(end) => end.write(name, source_content),
            None => String::new(),
        }
    }
}

impl<L: Lang> Clone for StdEnvironmentSyntax<L> {
    fn clone(&self) -> Self {
        StdEnvironmentSyntax { begin: self.begin.clone(), end: self.end.clone() }
    }
}

impl<L: Lang> fmt::Debug for StdEnvironmentSyntax<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdEnvironmentSyntax")
            .field("begin", &self.begin)
            .field("end", &self.end)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::super::test_support::{macro_package, with_package};
    use super::super::{
        CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
        VerbatimBehavior,
    };
    use super::*;
    use crate::constructs::{ConstructParser, GroupArgumentParser, StdInvocationParser};
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::node::{
        check_tree_invariants, BuildId, NodeRef, ParsedArguments, ParsedSlots,
    };
    use crate::scopes::Package;
    use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
    use crate::state::{ParsingState, ParsingStateDelta, TokenRulesOverrides};
    use crate::token::CommandRule;

    fn parse_ok(language: &Language<Latexlike>, input: &str) -> ParseResult<Latexlike> {
        let result = language.parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn payload<'t>(node: NodeRef<'t, Latexlike>) -> &'t InvocationSyntax {
        node.invocation_syntax().expect("a callable node")
    }

    // --- the macro arm -----------------------------------------------------------------

    #[test]
    fn macros_record_escape_char_and_post_space() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));

        // Multi-char command with post-space: recorded exactly (the token's own).
        let result = parse_ok(&language, "\\emph  x");
        let emph = result.tree.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntax::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '\\');
                assert_eq!(post_space.resolve("\\emph  x"), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
        // The sugar reads the same fact.
        assert_eq!(emph.post_space(), Some("  "));

        // No post-space (`{` follows the name directly): recorded empty.
        let result = parse_ok(&language, "\\emph{x}");
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.post_space(), Some(""));
    }

    #[test]
    fn macros_record_the_escape_char_as_written() {
        // A second command rule with the `@` escape: the payload records whichever
        // escape fired, not a canonical `\`.
        let seed = ParsingState::<Latexlike>::lang_initial_with_packages([macro_package(
            "t", "emph", None,
        )]);
        let mut commands = seed.rules().commands.clone();
        commands.push(Arc::new(CommandRule {
            escape_char: '@',
            name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
        }));
        let seed = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                commands: Some(commands),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Strict), seed);

        let result = parse_ok(&language, "@emph x");
        let emph = result.tree.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntax::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '@');
                assert_eq!(post_space.resolve("@emph x"), " ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
    }

    // --- the specials arm --------------------------------------------------------------

    #[test]
    fn specials_record_the_unit_arm_and_the_name_as_written() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "a---b ~ c");
        let ligature = result.tree.root().child(1).unwrap();
        assert!(matches!(payload(ligature), InvocationSyntax::Specials));
        // Option 1: the name IS the invocation spelling as written.
        assert_eq!(ligature.specials_name(), Some("---"));
        assert_eq!(ligature.post_space(), Some(""));

        let tilde = result.tree.root().child(3).unwrap();
        assert!(matches!(payload(tilde), InvocationSyntax::Specials));
        assert_eq!(tilde.specials_name(), Some("~"));
    }

    // --- the environment arm -----------------------------------------------------------

    fn env_language() -> Language<Latexlike> {
        let mut package = Package::new("t");
        package.insert(CallableType::Environment, "itemize", EnvironmentSpec::new(vec![]));
        package.insert(
            CallableType::Environment,
            "verbatim",
            EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
        );
        package.insert(CallableType::Environment, "A", EnvironmentSpec::new(vec![]));
        package.insert(CallableType::Environment, "B", EnvironmentSpec::new(vec![]));
        with_package(Recovery::Tolerant, package)
    }

    fn env_payload<'t>(node: NodeRef<'t, Latexlike>) -> &'t StdEnvironmentSyntax<Latexlike> {
        match payload(node) {
            InvocationSyntax::Environment(env) => env,
            other => panic!("expected the Environment arm, got {other:?}"),
        }
    }

    #[test]
    fn environments_record_begin_and_end_facts() {
        let language = env_language();
        let content = "\\begin {itemize}x\\end{itemize}";
        let result = parse_ok(&language, content);
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);

        // Begin side: escape char, command word, the *recorded* (no longer
        // normalized-away) post-space, and the name-group rule's exact bytes.
        assert_eq!(syntax.begin.escape_char, '\\');
        assert_eq!(syntax.begin.command_word.resolve(content), "begin");
        assert_eq!(syntax.begin.post_space.resolve(content), " ");
        assert_eq!(&*syntax.begin.name_group_rule.open, "{");
        assert_eq!(&*syntax.begin.name_group_rule.close, "}");

        // End side: filled from the terminator the body parser consumed.
        let end = syntax.end.as_ref().expect("a consumed terminator");
        assert_eq!(end.escape_char, '\\');
        assert_eq!(end.command_word.resolve(content), "end");
        assert_eq!(end.post_space.resolve(content), "");

        // The spelling writers reemit both sides exactly.
        assert_eq!(syntax.write_begin("itemize", content), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", content), "\\end{itemize}");

        // The sugar: environment-formed callables answer empty post-space.
        assert_eq!(env.post_space(), Some(""));
    }

    #[test]
    fn verbatim_environments_record_std_end_facts_from_the_literal() {
        let language = env_language();
        let content = "\\begin{verbatim}\na % b\n\\end{verbatim}";
        let result = parse_ok(&language, content);
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);

        // The raw body consumed its terminator as one literal token; the record
        // holds standard-shaped end facts (begin's escape char + rule, owned
        // command word, empty post-space).
        let end = syntax.end.as_ref().expect("the literal terminator was consumed");
        assert_eq!(end.escape_char, '\\');
        assert_eq!(end.command_word.resolve(content), "end");
        assert!(end.command_word.is_owned());
        assert_eq!(end.post_space.resolve(content), "");
        assert_eq!(syntax.write_end("verbatim", content), "\\end{verbatim}");
    }

    #[test]
    fn a_body_without_a_terminator_leaves_the_end_side_empty() {
        let language = env_language();

        // End of input inside the body.
        let result = language.parse("\\begin{itemize}x").unwrap();
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);
        assert!(syntax.end.is_none());
        assert_eq!(syntax.write_end("itemize", "\\begin{itemize}x"), "");

        // A name mismatch unwinds B without consuming `\end{A}`: B's end side is
        // empty, while A found and recorded its own terminator.
        let content = "\\begin{A}x\\begin{B}y\\end{A}";
        let result = language.parse(content).unwrap();
        let outer = result.tree.root().child(0).unwrap();
        assert_eq!(outer.environment_name(), Some("A"));
        assert!(env_payload(outer).end.is_some());
        let inner = outer.body().unwrap().iter().nth(1).unwrap();
        assert_eq!(inner.environment_name(), Some("B"));
        assert!(env_payload(inner).end.is_none());
    }

    // --- materialize-through -----------------------------------------------------------

    #[test]
    fn materialize_resolves_the_payload_through_the_bound_trait() {
        let language = env_language();
        let content = "\\begin {itemize}x\\end{itemize}";
        let result = parse_ok(&language, content);
        let owned = result.tree.materialize();

        let env = owned.root().child(0).unwrap();
        let syntax = env_payload(env);
        assert!(syntax.begin.command_word.is_owned());
        assert!(syntax.begin.post_space.is_owned());
        assert_eq!(syntax.begin.command_word.resolve(""), "begin");
        assert_eq!(syntax.begin.post_space.resolve(""), " ");
        let end = syntax.end.as_ref().unwrap();
        assert!(end.command_word.is_owned());
        // The writers now resolve with no source at all (source-independent
        // byte-faithful reconstruction).
        assert_eq!(syntax.write_begin("itemize", ""), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", ""), "\\end{itemize}");

        // The macro arm likewise.
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "\\emph  x");
        let owned = result.tree.materialize();
        let emph = owned.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntax::Macro { post_space, .. } => {
                assert!(post_space.is_owned());
                assert_eq!(post_space.resolve(""), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
        assert_eq!(emph.post_space(), Some("  "));
    }

    // --- the role-trait impl -----------------------------------------------------------

    #[test]
    fn the_enum_satisfies_the_fifth_role_trait_coherence_contracts() {
        type Syntax = InvocationSyntax;
        let macro_form: Syntax =
            LatexlikeInvocationSyntax::<Latexlike>::macro_form('\\', TextContent::from(" ".to_string()));
        let (escape_char, post_space) =
            LatexlikeInvocationSyntax::<Latexlike>::macro_syntax(&macro_form).unwrap();
        assert_eq!(escape_char, '\\');
        assert_eq!(post_space.resolve(""), " ");
        assert!(!LatexlikeInvocationSyntax::<Latexlike>::is_specials(&macro_form));

        let specials: Syntax = LatexlikeInvocationSyntax::<Latexlike>::specials_form();
        assert!(LatexlikeInvocationSyntax::<Latexlike>::is_specials(&specials));
        assert!(LatexlikeInvocationSyntax::<Latexlike>::macro_syntax(&specials).is_none());
        assert!(
            LatexlikeInvocationSyntax::<Latexlike>::environment_syntax(&specials).is_none()
        );
    }

    // --- stage_invocation (the staging shorthand) ---------------------------------------

    /// A rest-of-line takeover: consumes through the end of the line and claims the
    /// extent via `end_pos: Some(..)` — the consumed-extent-outruns-children case.
    #[derive(Debug)]
    struct RestOfLineSpec;

    impl CallableSpec<Latexlike> for RestOfLineSpec {
        fn requires_content(&self) -> bool {
            true
        }

        fn make_invocation_parser<'a, 's>(
            &'a self,
            invocation: crate::constructs::Invocation<'a, 's, Latexlike>,
        ) -> Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>
        where
            's: 'a,
        {
            struct RestOfLineParser<'a, 's> {
                invocation: crate::constructs::Invocation<'a, 's, Latexlike>,
            }
            impl ConstructParser<Latexlike> for RestOfLineParser<'_, '_> {
                type Output = BuildId;
                fn parse(
                    &mut self,
                    cx: &mut crate::constructs::ParseContext<'_, '_, Latexlike>,
                ) -> crate::constructs::ConstructParserResult<
                    Latexlike,
                    (BuildId, Option<ParsingStateDelta<Latexlike>>),
                > {
                    // Consume raw bytes through the end of the line.
                    let source = Arc::clone(&cx.source);
                    let content = source.content();
                    let start = cx.tokens.pos();
                    let end = content[start..]
                        .find('\n')
                        .map(|i| start + i)
                        .unwrap_or(content.len());
                    cx.tokens.move_to_pos(end);
                    let id = cx.stage_invocation(
                        &self.invocation,
                        ParsedArguments::empty(),
                        ParsedSlots::empty(),
                        Vec::new(),
                        Some(end),
                    )?;
                    Ok((id, None))
                }
            }
            Box::new(RestOfLineParser { invocation })
        }

        fn stack_frame_title(&self, role: FrameRole, name: &str) -> alloc::string::String {
            super::super::spec::frame_title("macro", role, name)
        }
    }

    #[test]
    fn stage_invocation_applies_the_std_and_explicit_end_rules() {
        // end_pos: None — the std rule: last child's span end…
        let mut package = Package::new("t");
        package.insert(
            CallableType::Macro,
            "emph",
            MacroSpec::new(vec![Arc::new(ArgumentSpec::new_unnamed(Arc::new(
                GroupArgumentParser::new(super::super::GroupType::Content),
            )))]),
        );
        package.insert(CallableType::Macro, "title", Arc::new(RestOfLineSpec));
        let language = with_package(Recovery::Strict, package);

        let result = parse_ok(&language, "\\emph{ab} x");
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.span().range(), 0..9);

        // …else the trigger's end (childless shapes, post-space included).
        let result = parse_ok(&language, "a---b");
        let ligature = result.tree.root().child(1).unwrap();
        assert_eq!(ligature.span().range(), 1..4);

        // end_pos: Some — the consumed extent outruns the (empty) child list.
        let content = "\\title The Title\nrest";
        let result = parse_ok(&language, content);
        let title = result.tree.root().child(0).unwrap();
        assert_eq!(title.name(), Some("title"));
        assert_eq!(title.span().range(), 0..content.find('\n').unwrap());
        assert_eq!(title.child_count(), 0);
        // The payload still transcribed from the bundle.
        assert!(matches!(
            payload(title),
            InvocationSyntax::Macro { escape_char: '\\', .. }
        ));
        let after = result.tree.root().child(1).unwrap();
        assert_eq!(after.chars(), Some("\nrest"));
    }

    // --- the () payload ----------------------------------------------------------------

    #[test]
    fn the_unit_payload_records_nothing_and_satisfies_both_traits() {
        use crate::state::InvocationSyntaxData;
        // materialized: the identity. (from_invocation for `()` is exercised by
        // every TrivialLang parse across the core suites.)
        #[allow(clippy::unit_cmp)]
        {
            assert_eq!(InvocationSyntaxData::materialized(&(), "abc"), ());
        }
    }

    // --- Debug still renders the parser bundle (regression for the swap) ---------------

    #[test]
    fn std_invocation_parser_debug_mentions_the_bundle() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "\\emph x");
        // Debug of the payload renders the arm.
        let emph = result.tree.root().child(0).unwrap();
        let rendered = alloc::format!("{:?}", payload(emph));
        assert!(rendered.contains("Macro"), "{rendered}");
        let _ = StdInvocationParser::new(crate::constructs::Invocation {
            callable_type: CallableType::Macro,
            name: "emph",
            spec: &(Arc::new(MacroSpec::new(vec![])) as Arc<dyn CallableSpec<Latexlike>>),
            token: &crate::token::Token::new(
                crate::token::TokenKind::EndOfStream,
                Span::empty(0),
                Span::empty(0),
            ),
        });
    }
}
