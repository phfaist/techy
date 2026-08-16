//! [`LatexlikeDriver`] and the preset's **behavior functions** — the driver's whole
//! behavior as public `LLL`-generic free functions
//! ([`math_group_interior_delta`], [`exit_math_context_delta`],
//! [`make_paragraph_break_node`]), with the driver as the ready-made assembly whose
//! hook bodies are precisely the one-line delegations to them.
//!
//! The layering is deliberate (the same whole-type-plus-behavior-functions layering
//! as [`LatexlikeLang`] itself): a struct cannot be partially overridden, so a
//! framework wanting preset-behavior-plus-one-custom-hook writes its own
//! [`ParseDriver`] whose other hooks are the same one-line delegations — the
//! struct contains no behavior the functions don't. The behavior functions also
//! serve **post-parse processing**: a transform synthesizing nodes that emulate "enter
//! math" / "exit math" derives coherent recorded states by feeding them the same
//! inputs the driver feeds ([`ParsingStateStack::from_node_ancestors`] supplies
//! the stack with no session anywhere).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::constructs::{FromInvocation, Invocation};
use crate::engine::{resolve_command_in_scopes, CommandResolution, ParseDriver};
use crate::error::Recovery;
use crate::node::{CallableData, NodeKind, ParsedArguments, ParsedSlots};
use crate::serialize::SerializableObject;
use crate::source::{IntoSourceResolver, SourceResolver};
use crate::spec::{CallableSpec, FrameRole};
use crate::state::{
    CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
    ParagraphOverrides, ParsingState, ParsingStateDelta, ParsingStateStack, SpecialsOverrides,
    TokenRulesOverrides, WhitespaceOverrides,
};
use crate::token::{GroupRule, Token};

use super::{
    Latexlike, LatexlikeCallableType, LatexlikeEvent, LatexlikeGroupType, LatexlikeLang,
    LatexlikeMode,
};

/// How [`LatexlikeDriver`] emits the node for a paragraph-break token (a whitespace
/// run containing two or more newlines,
/// [`ParagraphRules::enabled`](crate::token::ParagraphRules::enabled)).
///
/// This is a **driver emission policy**, deliberately not scope-stack data: the
/// tokenizer detects paragraph breaks within
/// leading whitespace, *before* the specials scan ever runs, so a package-registered
/// `"\n\n"` specials entry could never fire — correlating the node shape with package
/// contents would be dead configuration. The flag is driver-global; per-scope
/// suppression stays orthogonal (a state delta clearing the paragraphs gate, as
/// verbatim's features-disabled state does).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParagraphBreakStyle {
    /// A whitespace-only `Chars` node over the break's extent — the core default
    /// shape (and pylatexenc-legacy's), friendly to text extraction
    /// ([`content_as_chars`](crate::extract::content_as_chars) folds it into
    /// text).
    #[default]
    Chars,
    /// A [`Specials`](super::CallableType::Specials)-formed `Callable` node —
    /// pylatexenc-modern's paragraph-break shape. The node's *name* is the **actual
    /// whitespace run as written** (`"\n \t\n"` stays `"\n \t\n"`) — the specials
    /// name-as-written rule of the invocation-syntax payload
    /// ([`InvocationSyntaxData::Specials`](super::InvocationSyntaxData::Specials)); the
    /// node's *span* covers the same run. Identify paragraph-break nodes by **spec
    /// identity** — the stamped spec is the canonical [`ParagraphBreakSpec`],
    /// recognized by `Any`-downcast — never by a name spelling. The token level is
    /// unchanged (still
    /// [`ParagraphBreak`](crate::token::TokenKind::ParagraphBreak)), and the spec
    /// lives on no provider, so paragraph breaks do **not** appear in
    /// [`iter_symbols`](crate::scopes::ScopeStack::iter_symbols) enumerations.
    /// Extraction helpers treat the node as the non-text material it now is
    /// (`content_as_chars` reports it instead of folding it into text).
    Specials,
}

/// The canonical paragraph-break spec: the **definite, identifiable spec object**
/// stamped on every [`ParagraphBreakStyle::Specials`] break node, for every family
/// member (`impl<LLL: LatexlikeLang> CallableSpec<LLL>`). A consumer identifies
/// paragraph-break nodes by **spec identity**, which for this ZST is *type*
/// identity — `Any`-downcast the node's [`spec`](crate::node::CallableData::spec)
/// to `ParagraphBreakSpec` — never by a name spelling (the node's `name` is the
/// actual whitespace run, [`ParagraphBreakStyle::Specials`]). The paragraph-break
/// behavior function never mints an anonymous per-break spec.
///
/// Argument-less and content-less (the trait defaults); frame titles speak the
/// preset's specials vocabulary. It lives on no provider — paragraph breaks are a
/// driver emission policy ([`ParagraphBreakStyle`]), not scope-stack data.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParagraphBreakSpec;

// Does not participate in serialization yet — M5 gives it a real impl.
impl<LLL: LatexlikeLang> SerializableObject<LLL> for ParagraphBreakSpec {}

impl<LLL: LatexlikeLang> CallableSpec<LLL> for ParagraphBreakSpec {
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        super::spec::frame_title("specials", role, name)
    }
}

// --- the pillar functions ------------------------------------------------------------

/// The **math-interior** behavior function: the state delta a math-class group
/// descent applies to its interior — the interior parses in the language's
/// [`math_mode`](LatexlikeMode::math_mode), and, since LaTeX-like languages forbid
/// nested math, the math delimiters stop being *openers* inside it. `None` for
/// non-math rules ([`is_math`](LatexlikeGroupType::is_math) decides — form-blind:
/// inline and display share this one arm).
///
/// Derived from the **outer** `base` state (not the seed): the interior's group
/// rules are `base`'s minus its math-class rules (content groups, temporary
/// groups, commands, … stay exactly as the outer state has them), and the
/// forbidden characters derived from those *removed* rules
/// ([`LatexlikeLang::math_interior_forbidden_chars`] — no `'$'` literal anywhere)
/// are **merged into** `base`'s existing
/// [`forbidden_chars`](crate::token::ForbiddenCharsRules::chars), so an
/// embedder's own forbidden set survives into math and a stray `$` becomes a
/// diagnostic rather than the opener of an unclosed nested group.
///
/// # The math interior takes two components
///
/// This delta is **one half** of a math interior's state. The engine's group
/// descent adds the other: the descent invariant installs
/// [`expecting_group_close`](crate::token::TokenRules::expecting_group_close) for
/// the entered rule (via
/// [`ParserSession::group_interior_state`](crate::engine::ParserSession::group_interior_state),
/// where this function's delta merges in through the driver hook), which is what
/// keeps the group's own close delimiter recognizable after its opener class was
/// removed. Post-parse synthesis reproducing a math interior must apply **both**:
/// this delta *plus* a groups-block
/// [`expecting_close`](crate::state::GroupOverrides::expecting_close) override
/// naming the entered rule.
///
/// Pure in `(base, rule)` — the
/// [`group_interior_delta`](ParseDriver::group_interior_delta) memoization
/// contract.
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

/// The **exit-math** behavior function: the state delta lowering the exit-math-context event
/// ([`LatexlikeEvent::exit_math_context`], the preset's
/// [`Event::ExitMathContext`](super::Event::ExitMathContext)) — scan `stack`
/// (innermost-first) for the **first non-math enclosing state**
/// ([`LatexlikeMode::is_math`] on the state's mode) and restore that context:
/// its [`TokenRules`](crate::token::TokenRules) **minus the transient gates**
/// (below) plus its mode.
///
/// The delta is defined by *exiting the math context*, deliberately **never** by
/// seeking or constructing a "text mode": the restore target is whatever the
/// enclosing context actually is — mode value included — so embedder rule
/// customizations in force where math was entered (extra group rules, forbidden
/// characters, disabled features) come back exactly, where any static reset would
/// clobber them. When *every* enclosing state is math, the fallback target is the
/// **outermost** (seed-side) entry; on an empty stack there is nothing to restore
/// and the returned delta is empty (a no-op).
///
/// **Excluded from the restore** (never `Some` in the returned overrides):
/// [`expecting_group_close`](crate::token::TokenRules::expecting_group_close) and
/// [`temporary_group_rules`](crate::token::TokenRules::temporary_group_rules) — they
/// describe *in-flight structural expectations* of the abandoned context (which
/// close the found state's own group descent was waiting for; which
/// scoped-lifecycle delimiters were live there), not lexical context; restoring
/// them would plant another scope's expectations into the new one. The derived
/// state inherits both from its base as usual, and a following group descent
/// installs its own expectation through the descent invariant.
///
/// In-parse this is [`LatexlikeDriver`]'s
/// [`resolve_state_event`](ParseDriver::resolve_state_event) lowering, fed the
/// session's live stack; post-parse synthesis feeds a stack recovered from a
/// parsed node ([`ParsingStateStack::from_node_ancestors`]) — same signature, no
/// session. Scan semantics per [`ParsingStateStack`]: `Arc`-equal duplicate
/// entries cannot change the answer.
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

/// The **paragraph-break** behavior function: the node kind for a paragraph-break token in
/// the given [`ParagraphBreakStyle`] — the core default whitespace `Chars` shape,
/// or a `Specials`-formed `Callable` named by the actual whitespace run (see
/// [`ParagraphBreakStyle::Specials`] for the exact contract; the stamped spec is
/// the canonical [`ParagraphBreakSpec`]). `source_content` is the parsed source's
/// content — the bytes the token's span indexes into — which the `Specials` shape
/// records its name-as-written from.
///
/// **Parse-side only**: the returned kind is built around a live token (the
/// span-backed `Chars` shape resolves against the token's source; the node is
/// staged childless by the core). Post-parse synthesis of paragraph-break-like
/// material stages `Chars` nodes directly and never mints tokens.
pub fn make_paragraph_break_node<LLL: LatexlikeLang>(
    style: ParagraphBreakStyle,
    state: &ParsingState<LLL>,
    token: &Token<'_, LLL>,
    source_content: &str,
) -> NodeKind<LLL> {
    let _ = state;
    match style {
        ParagraphBreakStyle::Chars => NodeKind::chars(token.span),
        // The canonical ZST is minted per break rather than cached: the
        // allocation is negligible (once per paragraph break, cold next to a
        // parse), and identity is type identity (downcast), so distinct `Arc`s
        // are indistinguishable to consumers.
        ParagraphBreakStyle::Specials => {
            let spec: Arc<dyn CallableSpec<LLL>> = Arc::new(ParagraphBreakSpec);
            // Name-as-written: the actual whitespace run under the token's span.
            let name = &source_content[token.span.range()];
            // The preset's specials staging site consults the standard
            // constructor ([`FromInvocation`]) like every std site — over a
            // synthetic invocation bundling the break token — so a family
            // member's payload records whatever its constructor answers for a
            // specials-formed trigger (the preset enum: the unit `Specials`).
            let invocation = Invocation {
                callable_type: LLL::CallableTypeId::specials_callable(),
                name,
                spec: &spec,
                token,
            };
            let invocation_syntax = LLL::InvocationSyntax::from_invocation(&invocation);
            NodeKind::callable(CallableData {
                callable_type: invocation.callable_type,
                name: invocation.name.into(),
                spec,
                arguments: ParsedArguments::empty(),
                slots: ParsedSlots::empty(),
                invocation_syntax,
            })
        }
    }
}

// --- the canned assembly -------------------------------------------------------------

/// The preset's parse-behavior object ([`Lang::Driver`](crate::state::Lang::Driver)),
/// generic over the language family (`LLL`, [`LatexlikeLang`]; defaulting to
/// [`Latexlike`]): carries the tolerant-parsing policy, resolves command tokens
/// through the state's scope stack (under the language's
/// [macro role](LatexlikeCallableType::macro_callable) — `\begin`/`\end` resolve
/// like any other command to the [`builtin_package`](super::builtin_package)'s dispatch
/// entries), plugs math-class group interiors into math mode through the
/// descent-delta channel, lowers the exit-math-context event over the
/// enclosing-state stack, emits paragraph-break nodes per its
/// [`ParagraphBreakStyle`], and exposes an optional [`SourceResolver`] for
/// `\input`-like external references
/// ([`with_source_resolver`](LatexlikeDriver::with_source_resolver); the default is
/// none — the driver resolves nothing).
///
/// **The ready-made assembly of the preset's behavior functions**: every
/// behavior-carrying hook body is precisely a one-line delegation to the matching
/// public behavior function
/// ([`math_group_interior_delta`], [`exit_math_context_delta`],
/// [`make_paragraph_break_node`];
/// [`resolve_command_in_scopes`](crate::engine::resolve_command_in_scopes) + the
/// macro role for resolution) — the struct contains no behavior those functions
/// don't. A framework wanting different behavior for one hook writes its own
/// [`ParseDriver`] composing the same functions; the three settings here
/// ([`recovery`](LatexlikeDriver::recovery),
/// [`paragraph_break_style`](LatexlikeDriver::paragraph_break_style), the source
/// resolver) are orthogonal *configuration*, deliberately not behavior overrides.
///
/// The recovery policy is the driver's one mandatory setting — strict vs. tolerant must
/// be an explicit [`new`](LatexlikeDriver::new) argument (there is deliberately no
/// `Default`).
///
/// Construct-provision and the remaining hooks keep their trait defaults; preset
/// helper methods (e.g. package loading by name) arrive with the standard spec
/// database.
pub struct LatexlikeDriver<LLL: LatexlikeLang = Latexlike> {
    /// The tolerant-parsing policy to drive under.
    pub recovery: Recovery,
    /// How paragraph-break tokens become nodes (default:
    /// [`ParagraphBreakStyle::Chars`]).
    pub paragraph_break_style: ParagraphBreakStyle,
    /// The [`SourceResolver`] behind
    /// [`ParseDriver::source_resolver`] (`None` — the default — resolves nothing);
    /// set via [`with_source_resolver`](LatexlikeDriver::with_source_resolver).
    /// Private (the two policy settings above stay `pub`); value-level `dyn`
    /// deliberately (an embedding-environment capability consumed on the cold
    /// path) — see the asymmetry note on
    /// [`StdParseDriver`](crate::engine::StdParseDriver).
    source_resolver: Option<Arc<dyn SourceResolver<LLL::SourceOrigin>>>,
    /// The family member this driver drives (a driver carries no per-language
    /// data — the parameter exists so the hook signatures speak `LLL`; the
    /// `fn() -> LLL` spelling keeps the driver `Send + Sync` independent of the
    /// marker type).
    lang: PhantomData<fn() -> LLL>,
}

impl<LLL: LatexlikeLang> LatexlikeDriver<LLL> {
    /// A driver with the given recovery policy (and the default
    /// [`ParagraphBreakStyle::Chars`], no source resolver).
    pub fn new(recovery: Recovery) -> LatexlikeDriver<LLL> {
        LatexlikeDriver {
            recovery,
            paragraph_break_style: ParagraphBreakStyle::default(),
            source_resolver: None,
            lang: PhantomData,
        }
    }

    /// Emit paragraph-break nodes in the given style.
    pub fn with_paragraph_break_style(
        mut self,
        style: ParagraphBreakStyle,
    ) -> LatexlikeDriver<LLL> {
        self.paragraph_break_style = style;
        self
    }

    /// Use `resolver` for `\input`-like external source references — exposed through
    /// [`ParseDriver::source_resolver`]. Takes a resolver by value (shared internally)
    /// or an already-shared `Arc` (passed through, no double-wrap).
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

    /// One-line delegation to the language's
    /// [`check_parse_start`](LatexlikeLang::check_parse_start) behavior default —
    /// the parse-initialization checks (for [`Latexlike`](super::Latexlike): the
    /// all-escape-shadowed provider warning).
    fn observe_parse_start(
        &self,
        source: &Arc<crate::source::Source<LLL::SourceOrigin>>,
        seed: &Arc<ParsingState<LLL>>,
        diagnostics: &mut crate::error::Diagnostics<LLL::SourceOrigin>,
    ) {
        LLL::check_parse_start(source, seed, diagnostics);
    }

    /// Resolve a command token under the language's
    /// [macro role](LatexlikeCallableType::macro_callable) through the state's
    /// scope stack, via the standard
    /// [`resolve_command_in_scopes`](crate::engine::resolve_command_in_scopes): a hit
    /// dispatches; a clean miss reports the searched providers as the
    /// unresolvable-command detail; an operational provider failure is a distinct
    /// [`Failed`](CommandResolution::Failed) resolution. Every outcome is a
    /// resolution value — this implementation never answers the abort channel
    /// (`Ok(...)` wrapping is its whole use of the `Result`).
    fn resolve_command(
        &self,
        state: &ParsingState<LLL>,
        token: &Token<'_, LLL>,
    ) -> Result<CommandResolution<LLL>, crate::error::ParseError<LLL::SourceOrigin>> {
        Ok(resolve_command_in_scopes(state, token, LLL::CallableTypeId::macro_callable()))
    }

    /// One-line delegation to the [`make_paragraph_break_node`] behavior function
    /// with the driver's [`paragraph_break_style`](LatexlikeDriver::paragraph_break_style).
    fn make_paragraph_break_node(
        &self,
        state: &ParsingState<LLL>,
        token: &Token<'_, LLL>,
        source_content: &str,
    ) -> NodeKind<LLL> {
        make_paragraph_break_node(self.paragraph_break_style, state, token, source_content)
    }

    /// One-line delegation to the [`math_group_interior_delta`] behavior function
    /// (`None` for non-math classes — verbatim rules never reach a tokenizer
    /// descent at all, see
    /// [`GroupType::Verbatim`](super::GroupType::Verbatim)).
    fn group_interior_delta(
        &self,
        base: &ParsingState<LLL>,
        rule: &Arc<GroupRule<LLL>>,
    ) -> Option<ParsingStateDelta<LLL>> {
        math_group_interior_delta(base, rule)
    }

    /// One-line delegation to the [`exit_math_context_delta`] behavior function for the
    /// exit-math-context event
    /// ([`is_exit_math_context`](LatexlikeEvent::is_exit_math_context)); every
    /// other event is context-free (`Ok(None)`) and stays for
    /// [`finalize_transition`](crate::state::Lang::finalize_transition). This
    /// implementation never answers the abort channel (`Ok(...)` wrapping is its
    /// whole use of the `Result`).
    fn resolve_state_event(
        &self,
        event: &LLL::Event,
        stack: &ParsingStateStack<LLL>,
    ) -> Result<Option<ParsingStateDelta<LLL>>, crate::error::ParseError<LLL::SourceOrigin>>
    {
        Ok(event.is_exit_math_context().then(|| exit_math_context_delta(stack)))
    }
}

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
        use crate::source::Span;
        use crate::token::TokenKind;

        let state = ParsingState::<Latexlike>::lang_initial().expect("seed state");
        let content = "a\n \t\nb";
        let token: Token<'static, Latexlike> =
            Token::new(TokenKind::ParagraphBreak, Span::new(1, 5), Span::empty(1));

        let chars =
            make_paragraph_break_node(ParagraphBreakStyle::Chars, &state, &token, content);
        assert!(matches!(chars, NodeKind::Chars { .. }));

        let specials =
            make_paragraph_break_node(ParagraphBreakStyle::Specials, &state, &token, content);
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
