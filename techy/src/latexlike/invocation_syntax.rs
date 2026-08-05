//! The preset's invocation-syntax payload ([`Lang::InvocationSyntax`]): the
//! [`InvocationSyntaxData`] enum recording how a callable was invoked —
//! macro-formed, environment-formed, or specials-formed — plus the
//! environment-side machinery: the [`EnvironmentSyntax`] record contract and its
//! standard implementation [`StdEnvironmentSyntax`] over the per-side record
//! [`StdEnvironmentSideSyntax`].
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
    EnvironmentBeginSyntaxData, EnvironmentTerminatorSyntaxData, FromInvocation,
    Invocation, NameGroup,
};
use crate::source::{Source, Span, TextContent};
use crate::state::{InvocationSyntax, Lang};
use crate::token::{GroupRule, TokenKind};

use super::lang::{LatexlikeInvocationSyntax, LatexlikeLang};
use super::Latexlike;

/// The latexlike invocation-syntax payload ([`Lang::InvocationSyntax`]): the
/// recorded trigger-spelling facts (the *data*, hence the name — the
/// `CallableData`/`NodeData` family) of one callable invocation, by invocation
/// form.
///
/// - [`Macro`](InvocationSyntaxData::Macro) — a command-triggered invocation: the
///   escape character as written and the trigger token's own **syntactic
///   post-space** (the name-terminating whitespace of a multi-character command,
///   pylatexenc's `macro_post_space`; nothing beyond the token's own post-space is
///   ever claimed — whitespace after a single-character command or after a final
///   argument is ordinary sibling/region content, as in TeX). A `Spanned`
///   post-space is a sub-range of the node's span: trailing for zero-argument
///   callables, between the name and the first argument region otherwise.
///   Source recomposition ([`SourceRecomposer`](super::SourceRecomposer))
///   re-emits the recorded post-space **verbatim** — any smarter spacing policy
///   (normalizing, collapsing, or dropping the whitespace) belongs to a
///   converter built on techy, not to techy.
/// - [`Environment`](InvocationSyntaxData::Environment) — an environment-shaped
///   invocation: the begin/end syntax facts, in the `Env` record (default
///   [`StdEnvironmentSyntax`]).
/// - [`Specials`](InvocationSyntaxData::Specials) — a specials-formed invocation:
///   a **unit variant**, deliberately. The node's
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
/// `InvocationSyntaxData<StdEnvironmentSyntax<Flm>>`); the default anchors at the
/// preset lang ([`Latexlike`]). Scanning **tolerance** is a *parser* concern, not
/// a record concern: a family member wanting looser begin/end syntax swaps the
/// invocation/body parser through the parser-factory override
/// ([`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser))
/// — the record only records what its parser consumed.
///
/// [`Lang::InvocationSyntax`]: crate::state::Lang::InvocationSyntax
#[derive(Clone, Debug)]
pub enum InvocationSyntaxData<Env = StdEnvironmentSyntax<Latexlike>> {
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
    /// An environment-shaped invocation's begin/end syntax facts.
    Environment(Env),
    /// A specials-formed invocation: nothing to record beyond the node's `name`,
    /// which is the spelling as written (see the enum docs).
    Specials,
}

impl<L: Lang, Env: InvocationSyntax<L>> InvocationSyntax<L> for InvocationSyntaxData<Env> {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self {
        match self {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                InvocationSyntaxData::Macro {
                    escape_char: *escape_char,
                    post_space: post_space.materialized(source),
                }
            }
            InvocationSyntaxData::Environment(env) => {
                InvocationSyntaxData::Environment(env.materialized(source))
            }
            InvocationSyntaxData::Specials => InvocationSyntaxData::Specials,
        }
    }
}

/// The standard-site constructor ([`FromInvocation`]): a
/// [`Command`](TokenKind::Command) trigger records its
/// [`Macro`](InvocationSyntaxData::Macro) facts straight off the token; every
/// other trigger (a specials token, a paragraph-break token at the preset's
/// specials site) records [`Specials`](InvocationSyntaxData::Specials). The
/// [`Environment`](InvocationSyntaxData::Environment) arm is never minted here —
/// environment-shaped composition stages through
/// [`stage_node`](crate::constructs::ParseContext::stage_node) itself with
/// [`environment_form`](LatexlikeInvocationSyntax::environment_form).
impl<L: Lang, Env> FromInvocation<L> for InvocationSyntaxData<Env> {
    fn from_invocation(invocation: &Invocation<'_, '_, L>) -> Self {
        match &invocation.token.kind {
            TokenKind::Command { escape_char, post_space, .. } => {
                InvocationSyntaxData::Macro {
                    escape_char: *escape_char,
                    post_space: TextContent::Spanned(*post_space),
                }
            }
            _ => InvocationSyntaxData::Specials,
        }
    }
}

impl<LLL: LatexlikeLang, Env: EnvironmentSyntax<LLL>> LatexlikeInvocationSyntax<LLL>
    for InvocationSyntaxData<Env>
{
    type Env = Env;

    fn macro_form(escape_char: char, post_space: TextContent) -> Self {
        InvocationSyntaxData::Macro { escape_char, post_space }
    }

    fn environment_form(env: Env) -> Self {
        InvocationSyntaxData::Environment(env)
    }

    fn specials_form() -> Self {
        InvocationSyntaxData::Specials
    }

    fn macro_syntax(&self) -> Option<(char, &TextContent)> {
        match self {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                Some((*escape_char, post_space))
            }
            _ => None,
        }
    }

    fn environment_syntax(&self) -> Option<&Env> {
        match self {
            InvocationSyntaxData::Environment(env) => Some(env),
            _ => None,
        }
    }

    fn is_specials(&self) -> bool {
        matches!(self, InvocationSyntaxData::Specials)
    }
}

/// One side of the **standard** environment record's begin/end syntax
/// ([`StdEnvironmentSyntax`]'s component type) — the spelling facts of a
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
pub struct StdEnvironmentSideSyntax<L: Lang> {
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

impl<L: Lang> StdEnvironmentSideSyntax<L> {
    /// Resolve this side's spelling around `name` (the environment name as
    /// written): escape char + command word + post-space + open delimiter + name +
    /// close delimiter. `source` resolves the span-backed fields (the carrying
    /// node's own source).
    fn write(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        format!(
            "{}{}{}{}{}{}",
            self.escape_char,
            self.command_word.resolve(source),
            self.post_space.resolve(source),
            self.name_group_rule.open,
            name,
            self.name_group_rule.close,
        )
    }

    fn materialized(&self, source: &Source<L::SourceOrigin>) -> StdEnvironmentSideSyntax<L> {
        StdEnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.materialized(source),
            post_space: self.post_space.materialized(source),
            // Source-independent — exempt from materialization.
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> Clone for StdEnvironmentSideSyntax<L> {
    fn clone(&self) -> Self {
        StdEnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.clone(),
            post_space: self.post_space.clone(),
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> fmt::Debug for StdEnvironmentSideSyntax<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdEnvironmentSideSyntax")
            .field("escape_char", &self.escape_char)
            .field("command_word", &self.command_word)
            .field("post_space", &self.post_space)
            .field("name_group_rule", &self.name_group_rule)
            .finish()
    }
}

/// The environment-syntax **record contract** of an `Env` payload type (the
/// [`Environment`](InvocationSyntaxData::Environment) arm): a constructor from
/// the parsed facts, plus the type's own re-emission.
///
/// The record does **no scanning**: the driving composition (the preset's
/// `\begin` invocation parser) owns all scanning — the begin trigger, the rigid
/// name group, arguments, and the body whose parser consumes the terminator —
/// and hands the collected facts to [`from_parsed`] exactly once, at staging
/// time. (A record that scanned its own sides — a mutate-in-place accumulator —
/// would not work: the body parser is the terminator consumer, so end-side scanning
/// delegation would be illusory, and the accumulator shape would lock custom
/// `Env` types into the standard flow's shape.) Scanning **tolerance** is
/// likewise a parser concern: swap the invocation/body parser through the
/// parser-factory override
/// ([`make_invocation_parser`](crate::spec::CallableSpec::make_invocation_parser));
/// the record records what its parser consumed.
///
/// Re-emission stays a **writer pair** ([`write_begin`]/[`write_end`]) — the
/// recompose stage's `Concat` head/tail and the parse-law checker's
/// prefix/suffix pins each need the two sides separately — and is the accuracy
/// rule made concrete: what `from_parsed` recorded is exactly what the
/// writers emit.
///
/// The data bounds and materialization come from the core
/// [`InvocationSyntax`] supertrait (the name-group rule `Arc` is
/// source-independent and exempt).
///
/// [`from_parsed`]: EnvironmentSyntax::from_parsed
/// [`write_begin`]: EnvironmentSyntax::write_begin
/// [`write_end`]: EnvironmentSyntax::write_end
pub trait EnvironmentSyntax<L: LatexlikeLang>: InvocationSyntax<L> {
    /// Build the record from the parsed facts: the begin side's
    /// [`EnvironmentBeginSyntaxData`] (validated command trigger + matched rigid
    /// name group) and the terminator facts the body parser reported back —
    /// [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) for a tokenized
    /// terminator, [`Literal`](EnvironmentTerminatorSyntaxData::Literal) for a
    /// raw (verbatim) body's one-token literal terminator, `None` when the body
    /// closed without consuming one (mismatch, malformed terminator, end of
    /// input) — the end side then stays empty.
    fn from_parsed(
        begin: EnvironmentBeginSyntaxData<L>,
        terminator: Option<EnvironmentTerminatorSyntaxData<L>>,
    ) -> Self;

    /// The begin-side spelling as recorded, resolved around `name` (the
    /// environment's name as written); `source` (the carrying node's own source)
    /// resolves span-backed fields. What a source recomposer emits for the begin
    /// syntax.
    fn write_begin(&self, name: &str, source: &Source<L::SourceOrigin>) -> String;

    /// The end-side spelling as recorded — the empty string when the end side is
    /// empty (the body closed without consuming a terminator: reemitting nothing
    /// reproduces the recovered input).
    fn write_end(&self, name: &str, source: &Source<L::SourceOrigin>) -> String;
}

/// The standard environment-syntax record: per-side facts in
/// [`StdEnvironmentSideSyntax`] — begin always present, end filled from the
/// terminator facts at construction
/// ([`from_parsed`](EnvironmentSyntax::from_parsed)), or left empty on the
/// recovery paths (mismatch, malformed terminator, end of input).
pub struct StdEnvironmentSyntax<L: Lang> {
    /// The `\begin{name}` side's facts.
    pub begin: StdEnvironmentSideSyntax<L>,
    /// The `\end{name}` side's facts; `None` until the terminator is consumed —
    /// and permanently for a body that closed without one.
    pub end: Option<StdEnvironmentSideSyntax<L>>,
}

// Diagonal deliberately (not for all `(L, L2)` pairs): a lang's environment
// record materializes against that lang's own source-origin type; a broader impl
// would only sanction cross-lang payload reuse.
impl<L: Lang> InvocationSyntax<L> for StdEnvironmentSyntax<L> {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self {
        StdEnvironmentSyntax {
            begin: self.begin.materialized(source),
            end: self.end.as_ref().map(|end| end.materialized(source)),
        }
    }
}

impl<L: LatexlikeLang> EnvironmentSyntax<L> for StdEnvironmentSyntax<L> {
    /// Transcription per terminator arm:
    ///
    /// - the begin side transcribes the begin facts verbatim (spans stay
    ///   span-backed);
    /// - a [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) terminator
    ///   transcribes the end side the same way;
    /// - a [`Literal`](EnvironmentTerminatorSyntaxData::Literal) terminator (the
    ///   verbatim path — the whole `\end{name}` spelling was one expected-close
    ///   token, so no tokenized facts exist) synthesizes a **standard-shaped**
    ///   end side from the begin side's facts and the preset's `end` command
    ///   word: same escape character and name-group rule, owned command word,
    ///   empty post-space — exactly the spelling the literal terminator was
    ///   composed from;
    /// - `None` leaves the end side empty.
    fn from_parsed(
        begin: EnvironmentBeginSyntaxData<L>,
        terminator: Option<EnvironmentTerminatorSyntaxData<L>>,
    ) -> Self {
        let transcribe_side = |escape_char: char,
                               command_word: Span,
                               post_space: Span,
                               name_group: &NameGroup<L>| {
            StdEnvironmentSideSyntax {
                escape_char,
                command_word: TextContent::Spanned(command_word),
                post_space: TextContent::Spanned(post_space),
                name_group_rule: Arc::clone(&name_group.rule),
            }
        };
        let begin_side = transcribe_side(
            begin.escape_char,
            begin.command_word,
            begin.post_space,
            &begin.name_group,
        );
        let end = match &terminator {
            Some(EnvironmentTerminatorSyntaxData::Scanned {
                escape_char,
                command_word,
                post_space,
                name_group,
            }) => Some(transcribe_side(
                *escape_char,
                *command_word,
                *post_space,
                name_group,
            )),
            Some(EnvironmentTerminatorSyntaxData::Literal { .. }) => {
                Some(StdEnvironmentSideSyntax {
                    escape_char: begin_side.escape_char,
                    command_word: TextContent::from(String::from(
                        super::environments::END_COMMAND_NAME,
                    )),
                    post_space: TextContent::empty(),
                    name_group_rule: Arc::clone(&begin_side.name_group_rule),
                })
            }
            None => None,
        };
        StdEnvironmentSyntax { begin: begin_side, end }
    }

    fn write_begin(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        self.begin.write(name, source)
    }

    fn write_end(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        match &self.end {
            Some(end) => end.write(name, source),
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

    use super::super::test_support::{macro_package, with_package, with_packages};
    use super::super::{
        CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
        VerbatimBehavior,
    };
    use super::*;
    use crate::constructs::{ConstructParser, GroupArgumentParser, StdInvocationParser};
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::node::{
        BuildId, NodeRef, ParsedArguments, ParsedSlots,
    };
    use crate::latexlike::check_latexlike_tree_invariants;
    use crate::scopes::Package;
    use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
    use crate::state::{ParsingState, ParsingStateDelta, TokenRulesOverrides};
    use crate::token::CommandRule;

    fn parse_ok(language: &Language<Latexlike>, input: &str) -> ParseResult<Latexlike> {
        let result = language.parse(input).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn payload<'t>(node: NodeRef<'t, Latexlike>) -> &'t InvocationSyntaxData {
        node.invocation_syntax().expect("a callable node")
    }

    /// A resolution source over `content` (equal content, so span-backed payload
    /// fields resolve identically to the parse's own source).
    fn src(content: &str) -> Source {
        Source::new(content)
    }

    // --- the macro arm -----------------------------------------------------------------

    #[test]
    fn macros_record_escape_char_and_post_space() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));

        // Multi-char command with post-space: recorded exactly (the token's own).
        let result = parse_ok(&language, "\\emph  x");
        let emph = result.tree.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '\\');
                assert_eq!(post_space.resolve(&src("\\emph  x")), "  ");
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
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '@');
                assert_eq!(post_space.resolve(&src("@emph x")), " ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
    }

    // --- the specials arm --------------------------------------------------------------

    #[test]
    fn specials_record_the_unit_arm_and_the_name_as_written() {
        // minilatex supplies the `---`/`~` typography specials.
        let language = with_packages(
            Recovery::Strict,
            [super::super::minidefs::minilatex_package(), macro_package("t", "emph", None)],
        );
        let result = parse_ok(&language, "a---b ~ c");
        let ligature = result.tree.root().child(1).unwrap();
        assert!(matches!(payload(ligature), InvocationSyntaxData::Specials));
        // Option 1: the name IS the invocation spelling as written.
        assert_eq!(ligature.specials_name(), Some("---"));
        assert_eq!(ligature.post_space(), Some(""));

        let tilde = result.tree.root().child(3).unwrap();
        assert!(matches!(payload(tilde), InvocationSyntaxData::Specials));
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
            InvocationSyntaxData::Environment(env) => env,
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
        let source = src(content);
        assert_eq!(syntax.begin.command_word.resolve(&source), "begin");
        assert_eq!(syntax.begin.post_space.resolve(&source), " ");
        assert_eq!(&*syntax.begin.name_group_rule.open, "{");
        assert_eq!(&*syntax.begin.name_group_rule.close, "}");

        // End side: filled from the terminator the body parser consumed.
        let end = syntax.end.as_ref().expect("a consumed terminator");
        assert_eq!(end.escape_char, '\\');
        assert_eq!(end.command_word.resolve(&source), "end");
        assert_eq!(end.post_space.resolve(&source), "");

        // The spelling writers reemit both sides exactly.
        assert_eq!(syntax.write_begin("itemize", &source), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", &source), "\\end{itemize}");

        // The sugar: environment-formed callables answer empty post-space.
        assert_eq!(env.post_space(), Some(""));
    }

    #[test]
    fn verbatim_environments_synthesize_std_end_facts_from_the_literal() {
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
        let source = src(content);
        assert_eq!(end.command_word.resolve(&source), "end");
        assert!(end.command_word.is_owned());
        assert_eq!(end.post_space.resolve(&source), "");
        assert_eq!(syntax.write_end("verbatim", &source), "\\end{verbatim}");
    }

    #[test]
    fn a_body_without_a_terminator_leaves_the_end_side_empty() {
        let language = env_language();

        // End of input inside the body.
        let result = language.parse("\\begin{itemize}x").unwrap();
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);
        assert!(syntax.end.is_none());
        assert_eq!(syntax.write_end("itemize", &src("\\begin{itemize}x")), "");

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
        let empty = src("");
        assert_eq!(syntax.begin.command_word.resolve(&empty), "begin");
        assert_eq!(syntax.begin.post_space.resolve(&empty), " ");
        let end = syntax.end.as_ref().unwrap();
        assert!(end.command_word.is_owned());
        // The writers now resolve with no source at all (source-independent
        // byte-faithful reconstruction).
        assert_eq!(syntax.write_begin("itemize", &empty), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", &empty), "\\end{itemize}");

        // The macro arm likewise.
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "\\emph  x");
        let owned = result.tree.materialize();
        let emph = owned.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntaxData::Macro { post_space, .. } => {
                assert!(post_space.is_owned());
                assert_eq!(post_space.resolve(&src("")), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
        assert_eq!(emph.post_space(), Some("  "));
    }

    // --- the role-trait impl -----------------------------------------------------------

    #[test]
    fn the_enum_satisfies_the_fifth_role_trait_coherence_contracts() {
        type Syntax = InvocationSyntaxData;
        let macro_form: Syntax =
            LatexlikeInvocationSyntax::<Latexlike>::macro_form('\\', TextContent::from(" ".to_string()));
        let (escape_char, post_space) =
            LatexlikeInvocationSyntax::<Latexlike>::macro_syntax(&macro_form).unwrap();
        assert_eq!(escape_char, '\\');
        assert_eq!(post_space.resolve(&src("")), " ");
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
        // minilatex supplies the `---` specials the childless-shape probe uses.
        let language = with_packages(
            Recovery::Strict,
            [super::super::minidefs::minilatex_package(), package],
        );

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
            InvocationSyntaxData::Macro { escape_char: '\\', .. }
        ));
        let after = result.tree.root().child(1).unwrap();
        assert_eq!(after.chars(), Some("\nrest"));
    }

    // --- the () payload ----------------------------------------------------------------

    #[test]
    fn the_unit_payload_records_nothing_and_satisfies_both_traits() {
        // materialized: the identity. (from_invocation for `()` is exercised by
        // every TrivialLang parse across the core suites.)
        #[allow(clippy::unit_cmp)]
        {
            assert_eq!(
                crate::state::InvocationSyntax::<Latexlike>::materialized(
                    &(),
                    &Source::new("abc")
                ),
                ()
            );
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
