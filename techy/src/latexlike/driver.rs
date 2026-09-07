//! The preset's parse-time behavior: [`LatexlikeDriver`] and the functions it is
//! assembled from.
//!
//! A driver is the object a parse consults for decisions the language data does not
//! settle by itself. [`LatexlikeDriver`] is the ready-made one: pass it to
//! [`Language::new`](crate::core::Language::new) with a seed parsing state and you
//! have a working LaTeX-like parser. Its own settings are few — the recovery policy,
//! the shape of paragraph-break nodes ([`ParagraphBreakStyle`]), and an optional
//! source resolver for `\input`-like references.
//!
//! Each decision it makes is also available on its own, as a public function generic
//! over the language family:
//!
//! - [`math_group_interior_delta`] — the state change that puts a math group's
//!   interior into math mode;
//! - [`exit_math_context_delta`] — the state change that restores the enclosing
//!   non-math context, as `\text{…}` needs;
//! - [`make_paragraph_break_node`] — the node a blank line becomes.
//!
//! Two uses follow. A driver of your own can keep the preset's behavior for the hooks
//! it does not care about by calling these, since a struct cannot be partially
//! overridden. And code working on an already-parsed tree can reproduce the states
//! the parse recorded — for instance when a transformation inserts nodes that enter
//! or leave math — by calling them with the same inputs; the enclosing states they
//! need come from [`ParsingStateStack::from_node_ancestors`], with no parsing session
//! involved.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::engine::{resolve_command_in_scopes, CommandResolution, ParseDriver};
use crate::error::Recovery;
use crate::node::{CallableData, NodeKind, ParsedArguments, ParsedSlots};
use crate::source::{IntoSourceResolver, Source, SourceResolver, SourceSpan};
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{
    CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
    ParagraphOverrides, ParsingState, ParsingStateDelta, ParsingStateStack, SpecialsOverrides,
    TokenRulesOverrides, WhitespaceOverrides,
};
use crate::token::{GroupRule, Token, TokenReader};

use super::{
    EnvironmentInvocation, Latexlike, LatexlikeCallableType, LatexlikeEvent,
    LatexlikeGroupType, LatexlikeInvocationSyntax, LatexlikeLang, LatexlikeMode,
};

/// What shape of node a paragraph break becomes.
///
/// A paragraph break is a run of whitespace containing two or more newlines, which
/// the tokenizer reports as one token while
/// [`ParagraphRules::enabled`](crate::core::token::ParagraphRules::enabled) holds.
/// The two variants are the two conventions in use: plain whitespace text, or a
/// distinct callable node. Set it with
/// [`LatexlikeDriver::with_paragraph_break_style`]; the default is
/// [`Chars`](ParagraphBreakStyle::Chars).
///
/// The choice belongs to the driver rather than to a package, because the tokenizer
/// finds paragraph breaks while it skips leading whitespace, before any registered
/// specials entry could match a `"\n\n"` trigger. Turning paragraph breaks off for
/// part of a document is a separate matter: a state change disables the paragraph
/// rules there, as a verbatim region's state does.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParagraphBreakStyle {
    /// A whitespace-only `Chars` node covering the break.
    ///
    /// This is the default, and the shape text extraction handles most simply:
    /// [`content_as_chars`](crate::extract::content_as_chars) folds it into the
    /// surrounding text like any other whitespace.
    #[default]
    Chars,
    /// A `Callable` node in the [`Specials`](super::CallableType::Specials)
    /// invocation form, covering the break.
    ///
    /// The node's name is the whitespace run exactly as written — `"\n \t\n"` stays
    /// `"\n \t\n"`, following the name-as-written rule for specials
    /// ([`InvocationSyntaxData::Specials`](super::InvocationSyntaxData::Specials)) —
    /// and its span covers that same run.
    ///
    /// Recognize these nodes by their spec, which is always
    /// [`ParagraphBreakSpec`], and never by matching the name against a spelling.
    ///
    /// Choosing this style changes nothing at the token level: the token is still a
    /// [`ParagraphBreak`](crate::core::token::TokenKind::ParagraphBreak). The spec is
    /// not registered on any provider, so paragraph breaks do not appear in a
    /// [`ScopeStack::iter_symbols`](crate::core::specs::ScopeStack::iter_symbols)
    /// listing. Extraction treats the node as non-text material:
    /// `content_as_chars` reports it rather than folding it into text.
    Specials,
}

/// The spec stamped on every paragraph-break node emitted under
/// [`ParagraphBreakStyle::Specials`].
///
/// It exists so that paragraph breaks are identifiable. The type is a
/// zero-sized unit, so identity by spec is identity by type: downcast the node's
/// [`spec`](crate::core::node::CallableData::spec) with `Any` to this type. Do not
/// test the node's name instead — that name is the whitespace run as it was written.
/// Every break node is stamped with this spec; the parse never invents a separate spec per
/// break.
///
/// It implements [`CallableSpec`] for every language of the family, takes no
/// arguments and parses no content, and is registered on no provider, since the
/// choice to emit these nodes is the driver's ([`ParagraphBreakStyle`]) and not part
/// of any package.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParagraphBreakSpec;

// The `SerializableObject`/`DeserializableObject` impls (the empty self-contained
// form: the spec is a unit) live in `super::serialize`.

impl<LLL: LatexlikeLang> CallableSpec<LLL> for ParagraphBreakSpec {
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        super::spec::frame_title("specials", role, name)
    }
}

// --- the pillar functions ------------------------------------------------------------

/// The state change a math group applies to its interior, or `None` if `rule` does
/// not open a math group.
///
/// The interior parses in the language's
/// [`math_mode`](LatexlikeMode::math_mode), and the math delimiters stop opening
/// groups there, because LaTeX-like languages do not nest math. Whether a rule
/// counts as math is [`is_math`](LatexlikeGroupType::is_math); inline and display
/// math take the same change.
///
/// The change is computed from `base`, the state in force just outside the group, and
/// not from the seed state. The interior keeps everything `base` had — content group
/// rules, temporary groups, command rules — minus the math rules. The characters the
/// interior forbids are derived from exactly the rules that were removed
/// ([`LatexlikeLang::math_interior_forbidden_chars`]) and are *added to* the
/// characters `base` already forbade
/// ([`ForbiddenCharsRules::chars`](crate::core::token::ForbiddenCharsRules::chars)),
/// so a program's own forbidden characters survive into math and a stray `$` inside
/// math is diagnosed instead of opening a group that never closes.
///
/// The result is a function of `(base, rule)` alone, which is what
/// [`group_interior_delta`](ParseDriver::group_interior_delta) allows the engine to
/// cache.
///
/// # This is half of a math interior's state
///
/// The other half comes from the engine's group descent, which records that the
/// entered rule's closing delimiter is expected
/// ([`TokenRules::expecting_group_close`](crate::core::token::TokenRules::expecting_group_close),
/// installed by
/// [`ParserSession::group_interior_state`](crate::core::ParserSession::group_interior_state),
/// where this change is merged in through the driver hook). That is what keeps `$`
/// recognizable as the group's own close after its opener rule was removed.
///
/// Code reconstructing a math interior's state outside a parse must therefore apply
/// both: this change, plus a
/// [`GroupOverrides::expecting_close`](crate::core::token::GroupOverrides::expecting_close)
/// override naming the entered rule.
pub fn math_group_interior_delta<LLL: LatexlikeLang>(
    base: &ParsingState<LLL>,
    rule: &Arc<GroupRule<LLL>>,
) -> Option<ParsingStateDelta<LLL>> {
    if !rule.group_type.is_math() {
        return None;
    }
    let rules = base.rules();
    let mut kept: Vec<Arc<GroupRule<LLL>>> = Vec::with_capacity(rules.group_rules().len());
    let mut removed: Vec<Arc<GroupRule<LLL>>> = Vec::new();
    for group_rule in rules.group_rules() {
        if group_rule.group_type.is_math() {
            removed.push(Arc::clone(group_rule));
        } else {
            kept.push(Arc::clone(group_rule));
        }
    }
    // Merge the derived set into the *current* forbidden chars (at the
    // transition), never a fresh set — an embedder's forbidden chars must survive
    // into math.
    let mut forbidden = String::from(rules.forbidden_chars());
    for c in LLL::math_interior_forbidden_chars(&removed).chars() {
        if !forbidden.contains(c) {
            forbidden.push(c);
        }
    }
    Some(
        ParsingStateDelta::new().mode(LLL::ModeId::math_mode()).rules(TokenRulesOverrides {
            groups: GroupOverrides { rules: Some(kept), ..GroupOverrides::default() },
            forbidden_chars: ForbiddenCharsOverrides { chars: Some(forbidden.into()) },
            ..TokenRulesOverrides::default()
        }),
    )
}

/// The state change that leaves the math context: restore the innermost enclosing
/// non-math state's mode and token rules.
///
/// This is what the [`Event::ExitMathContext`](super::Event::ExitMathContext) event
/// asks for, and how `\text{…}` gets its argument parsed as text however deep inside
/// math it appears. `stack` is the enclosing states, innermost first; the target is
/// the first entry whose mode is not math ([`LatexlikeMode::is_math`]).
///
/// The target is an enclosing context that really exists, so nothing here invents a
/// "text mode" or resets the rules to a fixed set: extra group rules, forbidden
/// characters, and disabled features a program had customized where the math was
/// entered all come back exactly as they were.
///
/// If every enclosing state is math, the outermost entry is used instead. If the
/// stack is empty there is nothing to restore, and the returned change is empty.
///
/// Two fields of [`TokenRules`](crate::core::token::TokenRules) are deliberately left
/// untouched — they are never `Some` in the returned overrides:
/// [`expecting_group_close`](crate::core::token::TokenRules::expecting_group_close)
/// and
/// [`temporary_group_rules`](crate::core::token::TokenRules::temporary_group_rules).
/// Both describe what the *target* state was in the middle of expecting — which
/// closing delimiter its own group descent was waiting for, which scoped delimiters
/// were live in it — rather than its lexical context, and copying them here would
/// plant one region's expectations in another. A state derived from this change
/// inherits both from its own base as usual, and the next group descent installs its
/// own expectation.
///
/// During a parse, [`LatexlikeDriver`] calls this from
/// [`resolve_state_event`](ParseDriver::resolve_state_event) with the session's live
/// stack. Outside a parse, pass a stack recovered from a parsed node with
/// [`ParsingStateStack::from_node_ancestors`]. Duplicate `Arc`-equal entries in the
/// stack cannot change the answer.
pub fn exit_math_context_delta<LLL: LatexlikeLang>(
    stack: &ParsingStateStack<LLL>,
) -> ParsingStateDelta<LLL> {
    let target = stack.iter().find(|state| !state.mode().is_math()).or_else(|| stack.outermost());
    let Some(target) = target else {
        return ParsingStateDelta::new();
    };
    let rules = target.rules();
    // Exhaustive literal on purpose, at BOTH levels: every TokenRulesOverrides
    // block is spelled out, and every field inside each block override is spelled
    // out (no `..Default::default()`/`..disable()` anywhere) — a new field at
    // either level breaks this build until the restore-or-exclude decision is
    // made for it.
    ParsingStateDelta::new().mode(target.mode()).rules(TokenRulesOverrides {
        whitespace: WhitespaceOverrides {
            enabled: Some(rules.whitespace.enabled),
            chars: Some(rules.whitespace.chars.clone()),
        },
        paragraphs: ParagraphOverrides { enabled: Some(rules.paragraphs.enabled) },
        groups: GroupOverrides {
            enabled: Some(rules.groups.enabled),
            rules: Some(rules.groups.rules.clone()),
            // Transient gate — in-flight structural expectation, never restored
            // (user ruling amendment, 2026-08-04).
            temporary: None,
            // Transient gate — see above.
            expecting_close: None,
        },
        commands: CommandOverrides {
            enabled: Some(rules.commands.enabled),
            rules: Some(rules.commands.rules.clone()),
        },
        comments: CommentOverrides {
            enabled: Some(rules.comments.enabled),
            rules: Some(rules.comments.rules.clone()),
        },
        specials: SpecialsOverrides { enabled: Some(rules.specials.enabled) },
        forbidden_chars: ForbiddenCharsOverrides {
            chars: Some(rules.forbidden_chars.chars.clone()),
        },
    })
}

/// Builds the node a paragraph break becomes, in the given
/// [`ParagraphBreakStyle`].
///
/// `break_span` is the break token's span as the token reader reported it. Under
/// [`Chars`](ParagraphBreakStyle::Chars) the result is a whitespace `Chars` node over
/// that span. Under [`Specials`](ParagraphBreakStyle::Specials) it is a `Callable`
/// node whose name is the whitespace run that span covers and whose spec is a
/// [`ParagraphBreakSpec`]. `state` is the parsing state in force and is not consulted
/// by this implementation.
///
/// The node is returned childless, for the parser to stage. Both shapes refer to the
/// break's own source, so this is for use during a parse; a transformation adding
/// paragraph-break-like material to a finished tree builds its `Chars` nodes
/// directly.
pub fn make_paragraph_break_node<LLL: LatexlikeLang>(
    style: ParagraphBreakStyle,
    state: &ParsingState<LLL>,
    break_span: &SourceSpan<LLL::SourceOrigin>,
) -> NodeKind<LLL> {
    let _ = state;
    match style {
        ParagraphBreakStyle::Chars => NodeKind::chars(break_span.span()),
        // The canonical ZST is minted per break rather than cached: the
        // allocation is negligible (once per paragraph break, cold next to a
        // parse), and identity is type identity (downcast), so distinct `Arc`s
        // are indistinguishable to consumers.
        ParagraphBreakStyle::Specials => {
            let spec: Arc<dyn CallableSpec<LLL>> = Arc::new(ParagraphBreakSpec);
            // Name-as-written: the actual whitespace run the break span covers.
            let name = break_span.content();
            // A paragraph break is a specials-formed callable, so its payload is
            // whatever the family member's specials form answers
            // ([`LatexlikeInvocationSyntax::specials_form`]; the preset enum: the
            // unit `Specials`, recording nothing beyond the node's `name`).
            NodeKind::callable(CallableData {
                callable_type: LLL::CallableTypeId::specials_callable(),
                name: name.into(),
                spec,
                arguments: ParsedArguments::empty(),
                slots: ParsedSlots::empty(),
                invocation_syntax: LLL::InvocationSyntax::specials_form(),
            })
        }
    }
}

// --- the canned assembly -------------------------------------------------------------

/// The ready-made LaTeX-like parse driver: the second half of a working parser,
/// paired with a seed parsing state.
///
/// Build one with [`new`](LatexlikeDriver::new), which takes the one setting that has
/// no sensible default — whether the parse is strict or tolerant
/// ([`Recovery`]) — and hand it to [`Language::new`](crate::core::Language::new):
///
/// ```
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::{Latexlike, LatexlikeDriver};
///
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Tolerant),
///     ParsingState::lang_initial().expect("seed state"),
/// );
/// let result = language.parse(r"a $x$ b").unwrap();
/// assert!(result.tree.root().child(1).unwrap().is_math_group());
/// ```
///
/// The type is generic over the language family ([`LatexlikeLang`]) and defaults to
/// [`Latexlike`], so a language of your own can use this driver unchanged.
///
/// # What it decides
///
/// - **Recovery.** Whether the parse stops at the first problem or records a
///   diagnostic and continues ([`recovery`](LatexlikeDriver::recovery)).
/// - **Commands.** A command token is looked up in the state's scope stack under the
///   language's [macro form](LatexlikeCallableType::macro_callable). `\begin` and
///   `\end` are ordinary entries of the [`builtin_package`](super::builtin_package)
///   and resolve the same way.
/// - **Math groups.** Entering a math group puts its interior into math mode
///   ([`math_group_interior_delta`]), and the exit-math-context event restores the
///   surrounding context ([`exit_math_context_delta`]).
/// - **Paragraph breaks.** A blank line becomes a node in the driver's
///   [`ParagraphBreakStyle`]
///   ([`with_paragraph_break_style`](LatexlikeDriver::with_paragraph_break_style)).
/// - **External sources.** `\input`-like references are resolved by the driver's
///   [`SourceResolver`], if one was set with
///   [`with_source_resolver`](LatexlikeDriver::with_source_resolver). Without one,
///   the driver resolves nothing.
///
/// # Adjusting it
///
/// Those four settings are the whole of this type's configuration; it holds no other
/// behavior. Each decision above is also a public function of this module, and each
/// hook here is a one-line call to one of them. So a parser that needs one decision
/// changed writes its own [`ParseDriver`] and calls the same functions for the rest,
/// rather than trying to subclass this one. Behavior that belongs to a whole
/// language, rather than to one parse, is set on [`LatexlikeLang`] instead — the
/// math-delimiter table, for example.
///
/// The remaining [`ParseDriver`] hooks keep their trait defaults.
pub struct LatexlikeDriver<LLL: LatexlikeLang = Latexlike> {
    /// Whether a parse under this driver stops at the first problem or records a
    /// diagnostic and continues.
    pub recovery: Recovery,
    /// What shape of node a paragraph break becomes. Defaults to
    /// [`ParagraphBreakStyle::Chars`].
    pub paragraph_break_style: ParagraphBreakStyle,
    /// The resolver for `\input`-like external source references, returned by
    /// [`ParseDriver::source_resolver`]. `None` by default, meaning nothing is
    /// resolved; set it with
    /// [`with_source_resolver`](LatexlikeDriver::with_source_resolver).
    // Private, unlike the two settings above: a resolver is set through the builder
    // method so the `IntoSourceResolver` conversion applies. Value-level `dyn` — an
    // embedding-environment capability consumed on the cold path; cf. the asymmetry
    // note on `StdParseDriver`.
    source_resolver: Option<Arc<dyn SourceResolver<LLL::SourceOrigin>>>,
    // The family member this driver drives. A driver carries no per-language data;
    // the parameter exists so the hook signatures speak `LLL`, and the `fn() -> LLL`
    // spelling keeps the driver `Send + Sync` whatever the marker type is.
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> LatexlikeDriver<LLL> {
    /// Creates a driver with the given recovery policy.
    ///
    /// Paragraph breaks come out as [`Chars`](ParagraphBreakStyle::Chars) nodes and
    /// no source resolver is set; change either with
    /// [`with_paragraph_break_style`](LatexlikeDriver::with_paragraph_break_style)
    /// and [`with_source_resolver`](LatexlikeDriver::with_source_resolver).
    ///
    /// There is no `Default` implementation, because whether a parse is strict or
    /// tolerant is a decision the calling program has to make.
    pub fn new(recovery: Recovery) -> LatexlikeDriver<LLL> {
        LatexlikeDriver {
            recovery,
            paragraph_break_style: ParagraphBreakStyle::default(),
            source_resolver: None,
            lang: PhantomData,
        }
    }

    /// Emits paragraph-break nodes in the given style, replacing the driver's
    /// current [`paragraph_break_style`](LatexlikeDriver::paragraph_break_style).
    pub fn with_paragraph_break_style(
        mut self,
        style: ParagraphBreakStyle,
    ) -> LatexlikeDriver<LLL> {
        self.paragraph_break_style = style;
        self
    }

    /// Uses `resolver` for `\input`-like external source references.
    ///
    /// Without this, the driver resolves nothing and a source reference fails. The
    /// resolver is what [`ParseDriver::source_resolver`] returns, and what
    /// [`input_macro_spec`](super::input_macro_spec) needs in order to parse a
    /// referenced source into the same tree.
    ///
    /// Accepts a resolver by value, which is shared internally, or an `Arc` that is
    /// already shared, which is used as it is.
    pub fn with_source_resolver<M>(
        mut self,
        resolver: impl IntoSourceResolver<LLL::SourceOrigin, M>,
    ) -> LatexlikeDriver<LLL> {
        self.source_resolver = Some(resolver.into_source_resolver());
        self
    }
}

// Manual impls: a derive would spuriously demand `LLL: Clone`/`LLL: Debug`, and the
// `dyn SourceResolver` carries no `Debug` bound — the resolver field is shown by
// presence only.

impl<LLL: LatexlikeLang> Clone for LatexlikeDriver<LLL> {
    fn clone(&self) -> Self {
        LatexlikeDriver {
            recovery: self.recovery,
            paragraph_break_style: self.paragraph_break_style,
            source_resolver: self.source_resolver.clone(),
            lang: PhantomData,
        }
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for LatexlikeDriver<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LatexlikeDriver")
            .field("recovery", &self.recovery)
            .field("paragraph_break_style", &self.paragraph_break_style)
            .finish_non_exhaustive()
    }
}

impl<LLL: LatexlikeLang> ParseDriver<LLL> for LatexlikeDriver<LLL> {
    fn recovery(&self) -> Recovery {
        self.recovery
    }

    fn source_resolver(&self) -> Option<&dyn SourceResolver<LLL::SourceOrigin>> {
        self.source_resolver.as_deref()
    }

    /// Runs the language's start-of-parse checks,
    /// [`LatexlikeLang::check_parse_start`]. For [`Latexlike`](super::Latexlike),
    /// that is the warning about a provider whose commands no escape character in
    /// force can reach.
    fn observe_parse_start(
        &self,
        source: &Arc<Source<LLL::SourceOrigin>>,
        initial_state: &Arc<ParsingState<LLL>>,
        diagnostics: &mut crate::error::Diagnostics<LLL::SourceOrigin>,
    ) {
        LLL::check_parse_start(source, initial_state, diagnostics);
    }

    /// Looks the command token up in the state's scope stack under the language's
    /// [macro form](LatexlikeCallableType::macro_callable), using the standard
    /// [`resolve_command_in_scopes`](crate::core::specs::resolve_command_in_scopes).
    ///
    /// A hit dispatches to the definition found. A clean miss names the providers
    /// that were searched, as the detail of the unresolvable-command report. A
    /// provider that failed operationally is reported separately, as
    /// [`Failed`](CommandResolution::Failed). All three are resolution values: this
    /// implementation never returns `Err`.
    fn resolve_command(
        &self,
        state: &ParsingState<LLL>,
        token: &Token<LLL>,
        tokens: &dyn TokenReader<'_, LLL>,
    ) -> Result<CommandResolution<LLL>, crate::error::ParseError<LLL::SourceOrigin>> {
        Ok(resolve_command_in_scopes(
            state,
            token,
            tokens,
            LLL::CallableTypeId::macro_callable(),
        ))
    }

    /// Calls [`make_paragraph_break_node`] with the driver's
    /// [`paragraph_break_style`](LatexlikeDriver::paragraph_break_style).
    fn make_paragraph_break_node(
        &self,
        state: &ParsingState<LLL>,
        break_span: &SourceSpan<LLL::SourceOrigin>,
    ) -> NodeKind<LLL> {
        make_paragraph_break_node(self.paragraph_break_style, state, break_span)
    }

    /// Calls [`math_group_interior_delta`], which answers `None` for every group
    /// class that is not math. Verbatim regions never reach this hook at all: their
    /// content is read as raw text rather than tokenized, see
    /// [`GroupType::Verbatim`](super::GroupType::Verbatim).
    fn group_interior_delta(
        &self,
        base: &ParsingState<LLL>,
        rule: &Arc<GroupRule<LLL>>,
    ) -> Option<ParsingStateDelta<LLL>> {
        math_group_interior_delta(base, rule)
    }

    /// Calls [`exit_math_context_delta`] for the exit-math-context event
    /// ([`is_exit_math_context`](LatexlikeEvent::is_exit_math_context)).
    ///
    /// Every other event answers `Ok(None)`: its effect does not depend on the
    /// enclosing states, and it is handled by
    /// [`Lang::finalize_transition`](crate::core::Lang::finalize_transition) instead.
    /// This implementation never returns `Err`.
    fn resolve_state_event(
        &self,
        event: &LLL::Event,
        stack: &ParsingStateStack<LLL>,
    ) -> Result<Option<ParsingStateDelta<LLL>>, crate::error::ParseError<LLL::SourceOrigin>>
    {
        Ok(event.is_exit_math_context().then(|| exit_math_context_delta(stack)))
    }
}

// --- the preset's driver extension ----------------------------------------------------

/// The driver hooks that speak the preset's own vocabulary, on top of
/// [`ParseDriver`].
///
/// The core driver trait knows nothing about environments, since an environment is a
/// preset concept — the `\begin{name} … \end{name}` composition that
/// [`BeginSpec`](super::BeginSpec) puts together. The hooks that need to talk about
/// one live here instead.
///
/// [`LatexlikeLang`] requires this trait of a language's driver, so the preset's
/// parsers reach these hooks directly on
/// [`ParseContext::driver`](crate::core::constructs::ParseContext::driver), with no
/// downcast. Every method has a default that reproduces the behavior the preset's own
/// driver has, so opting a custom driver in is one line:
///
/// ```ignore
/// impl LatexlikeParseDriver<MyLang> for MyDriver {}
/// ```
pub trait LatexlikeParseDriver<LLL: LatexlikeLang>: ParseDriver<LLL> {
    /// Decides which state changes made inside an environment's body survive the
    /// environment, and reach the content around it.
    ///
    /// A body's state changes are ordinarily confined to the body: `\def` inside a
    /// `center` is gone at `\end{center}`. This hook is how a language lets some of
    /// them out — the `\gdef` shape. It is the environment counterpart of the core's
    /// group-level [`GroupAfterEffectsFn`](crate::core::constructs::GroupAfterEffectsFn),
    /// and mirrors its contract.
    ///
    /// The arguments, in order:
    ///
    /// 1. The invocation ([`EnvironmentInvocation`]) and, next, the resolved spec.
    ///    Together they are what a policy keys on — the environment's name, how it was
    ///    spelled, which definition it resolved to — where the group hook keys on a
    ///    matched [`GroupRule`]. A language may let `\gdef` escape a `center` but
    ///    nothing escape an `equation`.
    /// 2. The state the body started in: the invocation's own state with the
    ///    behavior's
    ///    [`body_state_delta`](super::EnvironmentBehavior::body_state_delta) applied,
    ///    before anything in the body changed it.
    /// 3. The state the body ended in
    ///    ([`EnvironmentBody::exit_state`](crate::core::constructs::EnvironmentBody::exit_state)).
    ///    This is the only place the definitions the body made can be inspected
    ///    (`state.scopes().retrieve_spec(…)`); it is discarded once the invocation
    ///    finishes, and this hook is its last reader.
    /// 4. The body's accumulated changes
    ///    ([`EnvironmentBody::after_effects`](crate::core::constructs::EnvironmentBody::after_effects)),
    ///    passed by value so a hook can filter it in place and return the same box
    ///    rather than cloning.
    ///
    /// The return value is what the environment contributes to the content around it.
    /// `Ok(None)`, the default, means nothing escapes, which is what every driver that
    /// does not override this method reports for every environment.
    ///
    /// The hook is called for every environment, including one whose body accumulated
    /// nothing: a policy that keys on the invocation alone still gets to run, and
    /// "the body changed nothing" is itself worth reporting.
    ///
    /// What escapes composes outward exactly as a group's does. The enclosing content
    /// applies the returned change to its own state *and* adds it to its own
    /// accumulated changes, so the next environment or group out sees it as argument
    /// 4 and may let it escape again.
    ///
    /// # What a hook can tell apart
    ///
    /// Argument 4 is a single merged change — token-rule overrides resolved
    /// last-writer-wins, scope operations and events concatenated in the order they
    /// were applied — and it records no provenance, so it cannot say which construct
    /// in the body contributed what.
    ///
    /// A `\gdef`-versus-`\def` distinction is therefore made structurally, by the
    /// language tagging its own operations: `\gdef` emits a
    /// [`ScopeOp::Define`](crate::core::specs::ScopeOp) against a globally named
    /// scope and `\def` against a local one, and the hook keeps the globally targeted
    /// operations and drops the rest. Rule, mode and extension overrides have no such
    /// tag and were already merged, so for those the only honest answers are all or
    /// nothing. [`GroupAfterEffectsFn`](crate::core::constructs::GroupAfterEffectsFn)
    /// describes the mechanics in full.
    ///
    /// A hook must be deterministic and free of side effects, like the core's
    /// descent-state and group-after-effect callbacks: an answer that depends on the
    /// order the hook happens to be called in would be fragile.
    ///
    /// # Errors
    ///
    /// `Err` aborts the parse under any recovery policy. It propagates exactly like a
    /// construct parser's own `Err`, and the traceback is attached at the call site,
    /// since a driver hook has no access to the session. Return
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure in the
    /// hook's own code, and
    /// [`ImplementationError`](crate::core::constructs::ImplementationError) for a
    /// violated library contract. A hook that cannot fail simply wraps its answer in
    /// `Ok(…)`.
    fn environment_after_effects(
        &self,
        invocation: &EnvironmentInvocation<'_, LLL>,
        spec: &Arc<dyn CallableSpec<LLL>>,
        initial: &Arc<ParsingState<LLL>>,
        exit: &Arc<ParsingState<LLL>>,
        record: Option<Box<ParsingStateDelta<LLL>>>,
    ) -> Result<
        Option<Box<ParsingStateDelta<LLL>>>,
        crate::error::ParseError<LLL::SourceOrigin>,
    > {
        let _ = (invocation, spec, initial, exit, record);
        Ok(None)
    }
}

/// The preset's driver takes the trait defaults: under it, nothing an environment's
/// body defines or changes escapes the environment.
impl<LLL: LatexlikeLang> LatexlikeParseDriver<LLL> for LatexlikeDriver<LLL> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::latexlike::{Event, GroupType, MathGroupForm, Mode};
    use crate::state::ParsingStateDelta;
    use alloc::vec;

    fn driver(recovery: Recovery) -> LatexlikeDriver {
        LatexlikeDriver::<Latexlike>::new(recovery)
    }

    #[test]
    fn the_recovery_knob_is_explicit() {
        assert_eq!(driver(Recovery::Strict).recovery, Recovery::Strict);
        assert_eq!(driver(Recovery::Tolerant).recovery, Recovery::Tolerant);
    }

    #[test]
    fn the_default_paragraph_break_style_is_chars() {
        assert_eq!(driver(Recovery::Strict).paragraph_break_style, ParagraphBreakStyle::Chars);
        assert_eq!(
            driver(Recovery::Strict)
                .with_paragraph_break_style(ParagraphBreakStyle::Specials)
                .paragraph_break_style,
            ParagraphBreakStyle::Specials
        );
    }

    #[test]
    fn the_driver_resolves_no_sources_unless_configured() {
        use crate::source::MapResolver;

        let bare = driver(Recovery::Strict);
        assert!(ParseDriver::<Latexlike>::source_resolver(&bare).is_none());

        let configured = driver(Recovery::Strict).with_source_resolver(MapResolver::new());
        assert!(ParseDriver::<Latexlike>::source_resolver(&configured).is_some());
    }

    #[test]
    fn math_rules_enter_math_mode_content_rules_do_not() {
        let driver = driver(Recovery::Strict);
        let state = ParsingState::<Latexlike>::lang_initial().expect("seed state");

        let math = Arc::new(GroupRule {
            group_type: GroupType::Math(MathGroupForm::Inline),
            open: "$".into(),
            close: "$".into(),
        });
        let delta = driver.group_interior_delta(&state, &math).unwrap();
        let derived = state.derived(&delta).unwrap();
        assert_eq!(derived.mode(), Mode::Math);

        let content = Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "{".into(),
            close: "}".into(),
        });
        assert!(driver.group_interior_delta(&state, &content).is_none());
    }

    // --- the pillars directly ----------------------------------------------------

    #[test]
    fn math_interior_pillar_derives_forbidden_chars_from_the_removed_rules() {
        // A custom single-char math pair: the interior's forbidden set gains the
        // pair's delimiters (derived, not a '$' literal) merged into the outer
        // set, and every math rule is removed while content rules stay.
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state");
        let mut groups = seed.rules().groups.rules.clone();
        let custom = Arc::new(GroupRule {
            group_type: GroupType::Math(MathGroupForm::Display),
            open: "«".into(),
            close: "»".into(),
        });
        groups.push(Arc::clone(&custom));
        let outer = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                groups: GroupOverrides { rules: Some(groups), ..GroupOverrides::default() },
                forbidden_chars: ForbiddenCharsOverrides { chars: Some("!".into()) },
                ..TokenRulesOverrides::default()
            }))
            .unwrap();

        let delta = math_group_interior_delta(&outer, &custom).unwrap();
        let interior = outer.derived(&delta).unwrap();
        assert_eq!(interior.mode(), Mode::Math);
        // The embedder's forbidden set survives; the derived chars merge in.
        let forbidden = interior.rules().forbidden_chars();
        for c in ['!', '$', '«', '»'] {
            assert!(forbidden.contains(c), "missing {c:?} in {forbidden:?}");
        }
        // Every math rule removed; the content rule kept.
        assert!(interior.rules().groups.rules.iter().all(|rule| !rule.group_type.is_math()));
        assert!(interior.rules().groups.rules.iter().any(|rule| &*rule.open == "{"));
    }

    #[test]
    fn exit_math_pillar_restores_the_first_non_math_context() {
        let seed = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));
        // A distinguishable enclosing text context: custom forbidden chars, plus
        // in-flight transients (an expected close and a temporary rule) that the
        // restore must NOT carry over (user ruling amendment, 2026-08-04).
        let brace_rule = Arc::clone(&seed.rules().groups.rules[0]);
        let temporary = Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "[".into(),
            close: "]".into(),
        });
        let text_context = Arc::new(
            seed.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                forbidden_chars: ForbiddenCharsOverrides { chars: Some("!".into()) },
                groups: GroupOverrides {
                    temporary: Some(alloc::vec![Arc::clone(&temporary)]),
                    expecting_close: Some(Some(Arc::clone(&brace_rule))),
                    ..GroupOverrides::default()
                },
                ..TokenRulesOverrides::default()
            }))
            .unwrap(),
        );
        let math_rule = Arc::clone(&text_context.rules().groups.rules[1]);
        assert!(math_rule.group_type.is_math());
        let math_interior = Arc::new(
            text_context
                .derived(&math_group_interior_delta(&text_context, &math_rule).unwrap())
                .unwrap(),
        );

        // Innermost-first: math interior, the custom text context, the seed. The
        // scan restores the FIRST non-math context — not the seed.
        let stack = ParsingStateStack::from_states(vec![
            Arc::clone(&math_interior),
            Arc::clone(&text_context),
            Arc::clone(&seed),
        ]);
        let delta = exit_math_context_delta(&stack);
        // The transient gates are excluded from the restore — never `Some`.
        assert!(delta.rules.groups.expecting_close.is_none());
        assert!(delta.rules.groups.temporary.is_none());
        // Every lexical field IS restored.
        assert!(delta.rules.groups.rules.is_some());
        assert!(delta.rules.forbidden_chars.chars.is_some());
        let restored = math_interior.derived(&delta).unwrap();
        assert_eq!(restored.mode(), Mode::Text);
        assert_eq!(restored.rules().forbidden_chars(), "!");
        assert_eq!(restored.rules().groups.rules, text_context.rules().groups.rules);
        // The found context's in-flight expectations did not come back: the
        // derived state keeps its base's transients as usual (the base inherited
        // the text context's expectation structurally; nothing was *restored*).
        assert_eq!(
            restored.rules().groups.expecting_close,
            math_interior.rules().groups.expecting_close
        );
        assert_eq!(
            restored.rules().groups.temporary,
            math_interior.rules().groups.temporary
        );

        // All-math stack: the outermost entry is the fallback.
        let all_math = ParsingStateStack::from_states(vec![Arc::clone(&math_interior)]);
        let delta = exit_math_context_delta(&all_math);
        let restored = math_interior.derived(&delta).unwrap();
        assert_eq!(restored.rules(), math_interior.rules());

        // Empty stack: the empty delta (a no-op derivation).
        let empty: ParsingStateStack<Latexlike> = ParsingStateStack::new();
        let delta = exit_math_context_delta(&empty);
        let unchanged = math_interior.derived(&delta).unwrap();
        assert_eq!(unchanged.rules(), math_interior.rules());
        assert_eq!(unchanged.mode(), math_interior.mode());
    }

    #[test]
    fn the_driver_lowers_exactly_the_exit_math_event() {
        let seed = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));
        let stack = ParsingStateStack::from_states(vec![Arc::clone(&seed)]);
        let driver = driver(Recovery::Strict);
        // The exit-math event lowers to the pillar's delta…
        let lowered =
            driver.resolve_state_event(&Event::ExitMathContext, &stack).unwrap();
        assert!(lowered.is_some());
        assert_eq!(lowered.unwrap().mode, Some(Mode::Text));
        // (No other preset event exists yet; non-exit events would answer None.)
    }

    #[test]
    fn paragraph_break_pillar_is_the_driver_behavior() {
        let state = ParsingState::<Latexlike>::lang_initial().expect("seed state");
        let source: Arc<Source> = Arc::new(Source::new("a\n \t\nb"));
        let break_span = SourceSpan::new(&source, 1..5);

        let chars =
            make_paragraph_break_node(ParagraphBreakStyle::Chars, &state, &break_span);
        assert!(matches!(chars, NodeKind::Chars { .. }));

        let specials =
            make_paragraph_break_node(ParagraphBreakStyle::Specials, &state, &break_span);
        let NodeKind::Callable(data) = specials else {
            panic!("expected a Callable kind")
        };
        assert_eq!(data.callable_type, super::super::CallableType::Specials);
        // Name-as-written: the actual whitespace run, never a canonical spelling.
        assert_eq!(&*data.name, "\n \t\n");
        // The stamped spec is the canonical ZST — identity by downcast.
        assert!((&*data.spec as &dyn core::any::Any)
            .downcast_ref::<ParagraphBreakSpec>()
            .is_some());
        // The payload came through the standard constructor: the unit arm.
        use super::super::LatexlikeInvocationSyntax;
        assert!(LatexlikeInvocationSyntax::<Latexlike>::is_specials(
            &data.invocation_syntax
        ));
    }
}
