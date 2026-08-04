//! The `latexlike` preset (S2): the familiar LaTeX behavior, assembled from the
//! generic core.
//!
//! The core has no privileged language concepts (no built-in math mode, `{`/`}`, `%`,
//! or `\`); this module is where the familiar vocabulary returns — as *preset data and
//! preset code* over the same extension surface any language uses:
//!
//! - [`Latexlike`] — the [`Lang`] ZST, with the preset's closed vocabularies
//!   [`GroupType`], [`CallableType`], [`Mode`], and [`Event`];
//! - the **language family**: the [`LatexlikeLang`] umbrella and the per-vocabulary
//!   role traits ([`LatexlikeGroupType`], [`LatexlikeCallableType`],
//!   [`LatexlikeMode`], [`LatexlikeEvent`]) — every generic preset component takes
//!   an `LLL: LatexlikeLang`, and a framework language with its own vocabularies or
//!   exts joins the family instead of forking the preset;
//! - [`LatexlikeDriver`] — the preset's [`ParseDriver`](crate::engine::ParseDriver):
//!   recovery policy, scope-stack command resolution, and the math-mode group plug;
//! - [`default_token_rules`] and [`base_package`] — the canonical seed data behind
//!   [`Latexlike::initial_state_data`];
//! - the callable spec types — [`MacroSpec`] and [`SpecialsSpec`] (declarative,
//!   with the preset's traceback vocabulary), and [`EnvironmentSpec`] (declared
//!   arguments plus body behavior via [`EnvironmentBehavior`]) with the
//!   `\begin`/`\end` composition ([`BeginSpec`]/[`EndSpec`], seeded in
//!   [`base_package`]);
//! - the argument-code factory [`argument_specs`] (`["o", "{"]` → configured
//!   [`ArgumentSpec`](crate::spec::ArgumentSpec)s; compact whole-spec strings via
//!   [`argument_specs_from_str`]) and the verbatim wiring —
//!   [`VerbatimBehavior`] for `verbatim`-style environment bodies, the `v` codes for
//!   `\verb`-style delimited verbatim arguments;
//! - `NodeRef` accessor sugar for latexlike trees
//!   ([`math_form`](crate::node::NodeRef::math_form),
//!   [`is_math_group`](crate::node::NodeRef::is_math_group), …) — inherent methods on
//!   latexlike-shaped `NodeRef`s.
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver, Mode};
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial(),
//! );
//! let result = language.parse("inline $x+y$ math").unwrap();
//! let math = result.tree.root().child(1).unwrap();
//! assert!(math.is_math_group());
//! assert_eq!(math.child(0).unwrap().parsing_state().mode(), Mode::Math);
//! ```
//!
//! **What the preset does not ship yet:** macro and environment *definitions*. The
//! standard spec database (pylatexenc's default-specs port) is a later phase; until
//! then embedders and tests register the specs they need in their own packages
//! ([`ParsingState::lang_initial_with_packages`](crate::state::ParsingState::lang_initial_with_packages)),
//! as [`MacroSpec`]/[`EnvironmentSpec`]/[`SpecialsSpec`] entries (or any custom
//! [`CallableSpec`]) — `\verb` and `verbatim` included: the machinery ships here, the
//! definitions with the database.

mod arguments;
mod driver;
mod environments;
mod lang;
mod node_ref;
mod spec;
#[cfg(test)]
mod test_support;

pub use arguments::{argument_specs, argument_specs_from_str, ArgumentCodeError};
pub use driver::{
    exit_math_context_delta, make_paragraph_break_node, math_group_interior_delta,
    LatexlikeDriver, ParagraphBreakStyle,
};
pub use environments::{
    BeginSpec, EndSpec, EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec,
    MalformedBegin, OrphanEnd, UnknownEnvironment, VerbatimBehavior,
};
pub use lang::{
    LatexlikeCallableType, LatexlikeEvent, LatexlikeGroupType, LatexlikeLang, LatexlikeMode,
};
pub use spec::{MacroSpec, SpecialsSpec};

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::node::BodySlotExt;
use crate::scopes::{Package, ScopeStack};
use crate::spec::CallableSpec;
use crate::state::{
    ClosedVocabulary, FinalizeError, Lang, NodeExtTypes, ParsingState, StateData,
};
use crate::token::{
    CommandRule, CommentRule, GroupRule, SpecialsMatch, TokenResult, TokenRules,
    TriggerChars, WhitespaceRules,
};

/// The form in which a math group appears: inline or display — the typed class
/// payload of [`GroupType::Math`], declared by the rule author at
/// [`GroupRule`] registration and read back off the node via
/// [`NodeRef::math_form`](crate::node::NodeRef::math_form) (no table, no string
/// matching, no state lookup — correct for embedder-registered and mid-parse-minted
/// delimiters alike).
///
/// The name is *form*, deliberately not "style": typesetting **style** (fonts, script
/// level, `\displaystyle`) is orthogonal — `$\displaystyle …$` renders display-*style*
/// math inside an inline-*form* group. The type names how the math group appears in
/// the source, not how its content is typeset.
///
/// **Exhaustive on purpose** (no `#[non_exhaustive]`): renderers match on the form
/// constantly, and a wildcard arm on every consumer would be a permanent tax against
/// a third form nobody can name. Adding one is a conscious breaking change.
///
/// # The payload-admission rule
///
/// Class payloads are not a dumping ground. A payload on a
/// [`GroupType`] class is admissible only when it is (a) **parse-behavior-invariant**
/// — parse wiring keeps a single arm (`Math(_)` matches once; interior delta,
/// visibility, and forbidden-char logic are form-blind); (b) **semantically
/// universal** for downstream consumers of the class; and (c) **declared at rule
/// registration**, never derived from delimiter spellings. Inline/display passes all
/// three; a hypothetical `Content(BraceKind)` fails (b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathGroupForm {
    /// `$…$` / `\(…\)` — inline math.
    Inline,
    /// `$$…$$` / `\[…\]` — display math.
    Display,
}

/// The preset's group classes ([`Lang::GroupTypeId`]).
///
/// Classes classify **parse behavior**, not delimiter spellings: several delimiter
/// pairs share one class, and the node's [`GroupData`](crate::node::GroupData) records
/// the delimiters as written. There is deliberately a *single* math class: inline and
/// display math parse identically — same interior [`Mode::Math`], same definition
/// visibility — so parse wiring stays single-armed (`Math(_)`), while the
/// inline/display **form** rides as typed class payload ([`MathGroupForm`]), declared
/// at rule registration and exposed by
/// [`NodeRef::math_form`](crate::node::NodeRef::math_form).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupType {
    /// A plain content group (`{…}`, and the argument groups minted by argument
    /// parsers — e.g. the optional `[…]`): the interior continues in the surrounding
    /// mode.
    Content,
    /// A math group (`$…$`, `$$…$$`, `\(…\)`, `\[…\]`): the interior parses in
    /// [`Mode::Math`] (the driver's
    /// [`group_interior_delta`](crate::engine::ParseDriver::group_interior_delta)
    /// plug — form-blind: a single `Math(_)` wiring arm). The payload records the
    /// group's [`MathGroupForm`], declared where the delimiter rule is registered.
    Math(MathGroupForm),
    /// A verbatim region's group: the `\verb|…|` shape staged by the `v`
    /// argument codes ([`argument_specs`]), and the class of the terminator rules
    /// verbatim readers mint. The interior is **raw text** — it is read under a
    /// features-off derived state, never tokenized — so this class appears on no
    /// tokenizer-declared rule and never descends through the driver's
    /// `group_interior_delta`.
    Verbatim,
}

/// The preset's invocation forms ([`Lang::CallableTypeId`]): the familiar
/// macro/environment/specials trichotomy, closed per the core's callable-type
/// contract — new invocation *forms* are never registered at runtime, new *callables*
/// are (via the scope stack).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableType {
    /// A macro invocation (`\emph{…}`). Every command token resolves as a macro —
    /// `\begin` and `\end` themselves are ordinary macro entries of the
    /// [`base_package`] ([`BeginSpec`]/[`EndSpec`]) whose parsers dispatch the
    /// environment shape.
    Macro,
    /// An environment (`\begin{itemize}…\end{itemize}`): entered through
    /// [`BeginSpec`]'s composition, which resolves the *environment's* spec —
    /// normally an [`EnvironmentSpec`] — under this callable type by the name in the
    /// `\begin` name group, and stamps this type on the staged node.
    Environment,
    /// A specials invocation: a trigger character sequence (`~`, `&`, `---`).
    Specials,
}

/// The preset's parsing modes ([`Lang::ModeId`]): text vs. math.
///
/// The mode is first-class state data ([`ParsingState::mode`]) — the single source of
/// truth for "am I in math" (no `StateExt` flag): math groups *initiate* the change
/// through the driver's descent-delta plug, and definition visibility keys on it
/// ([`Package::set_visible_modes`]). Inline vs. display math is deliberately **not** a
/// mode (nor its own group class): it changes nothing about how the interior parses —
/// the form is class payload, [`MathGroupForm`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    /// Ordinary text content — the seed state's mode.
    #[default]
    Text,
    /// Math content (inside `$…$`, `$$…$$`, `\(…\)`, `\[…\]`).
    Math,
}

/// The preset's semantic transition events ([`Lang::Event`]).
///
/// Events split into **two classes** (the contract on
/// [`ParsingStateDelta::events`](crate::state::ParsingStateDelta::events)):
/// context-free events are consumed by [`Lang::finalize_transition`]; a
/// **context-dependent** event — one whose effect depends on the enclosing-state
/// stack — is lowered by the driver
/// ([`ParseDriver::resolve_state_event`](crate::engine::ParseDriver::resolve_state_event))
/// inside [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)
/// and never reaches `finalize_transition`; reaching it anyway (a bare
/// out-of-parse [`derived()`](ParsingState::derived) call) is a loud error.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// **Exit the math context** (context-dependent): restore the innermost
    /// enclosing *non-math* context — its whole
    /// [`TokenRules`] and its mode — found on the
    /// enclosing-state stack (else the outermost, seed context). The `\text{…}`
    /// recipe: an [`ArgumentSpec`](crate::spec::ArgumentSpec) state delta carrying
    /// this event makes the argument parse in the surrounding non-math context
    /// even deep inside display math — composably with every argument shape, and
    /// without clobbering embedder rule customizations the way a static
    /// rules-reset would. Lowered via
    /// [`exit_math_context_delta`]; deliberately **not**
    /// "restore text mode": the target is whatever the enclosing context is, never
    /// a conjured mode value.
    ExitMathContext,
}

// The preset's vocabularies are statically listable (Phase 7.8): generic tooling
// enumerates them (e.g. `ScopeStack::iter_symbols` once per `CallableType::ALL` entry).
// The enums are `#[non_exhaustive]`, so adding a variant means extending `ALL` in the
// same change (the `ClosedVocabulary` contract).

impl ClosedVocabulary for GroupType {
    const ALL: &'static [GroupType] = &[
        GroupType::Content,
        GroupType::Math(MathGroupForm::Inline),
        GroupType::Math(MathGroupForm::Display),
        GroupType::Verbatim,
    ];
}

impl ClosedVocabulary for CallableType {
    const ALL: &'static [CallableType] =
        &[CallableType::Macro, CallableType::Environment, CallableType::Specials];
}

impl ClosedVocabulary for Mode {
    const ALL: &'static [Mode] = &[Mode::Text, Mode::Math];
}

/// The preset's slot ext: marks whether a slot is **the body** of its callable
/// ([`BodySlotExt`]). Minted by the preset's environment machinery via
/// [`BodySlotExt::make_body`]; a non-body slot's value comes from
/// [`not_body`](BodyMarker::not_body). Deliberately no `Default` — an ext value is
/// minted, never conjured (population is initialization, [`NodeExtTypes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyMarker {
    body: bool,
}

impl BodyMarker {
    /// The marker of a slot that is *not* the body.
    pub fn not_body() -> BodyMarker {
        BodyMarker { body: false }
    }
}

impl BodySlotExt for BodyMarker {
    fn is_body(&self) -> bool {
        self.body
    }

    fn make_body() -> BodyMarker {
        BodyMarker { body: true }
    }
}

/// The preset's node-ext bundle ([`Lang::NodeExts`]): no per-node and no per-argument
/// data (`NodeExt`/`ArgumentExt` are `()`), while **`SlotExt` is claimed** for the
/// [`BodyMarker`] — the preset marks environment body slots through the generic
/// [`BodySlotExt`] mechanism, so [`NodeRef::body`](crate::node::NodeRef::body) selects
/// the marked slot rather than relying on slot positions.
#[derive(Debug, Clone, Copy)]
pub struct LatexlikeNodeExts;

impl NodeExtTypes for LatexlikeNodeExts {
    type NodeExt = ();
    type ArgumentExt = ();
    type SlotExt = BodyMarker;
}

/// The latexlike language bundle: a ZST implementing [`Lang`] with the preset's
/// vocabularies ([`GroupType`], [`CallableType`], [`Mode`]), the canonical seed
/// ([`default_token_rules`] + the [`base_package`] on the scope stack), and the
/// scope-stack specials scan. Parse-time behavior lives on [`LatexlikeDriver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Latexlike;

impl Lang for Latexlike {
    type GroupTypeId = GroupType;
    type CallableTypeId = CallableType;
    type ModeId = Mode;
    type StateExt = ();
    type Event = Event;
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = LatexlikeNodeExts;
    type Driver = LatexlikeDriver;

    /// The canonical latexlike seed: [`default_token_rules`], a scope stack holding
    /// the [`base_package`] (standard specials), [`Mode::Text`].
    ///
    /// Coherence contract: `finalize_transition` is not customized (nothing to
    /// normalize yet — math groups only set the mode), so the seed is trivially
    /// finalize-coherent; a test pins `lang_initial().derived(&empty) == lang_initial()`
    /// data-equivalence mechanically.
    fn initial_state_data() -> StateData<Self> {
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(base_package()));
        StateData {
            rules: default_token_rules(),
            scopes,
            mode: Mode::Text,
            ext: (),
        }
    }

    /// The two-class event contract's loud arm ([`Event`]): the preset's
    /// [`ExitMathContext`](Event::ExitMathContext) is **context-dependent** — it
    /// is lowered by the driver
    /// ([`ParseDriver::resolve_state_event`](crate::engine::ParseDriver::resolve_state_event)) (via
    /// [`exit_math_context_delta`]) inside a driven parse and never reaches this
    /// hook there. Reaching it here means a bare out-of-parse
    /// [`derived()`](ParsingState::derived) call (or a mis-wired driver): the
    /// transition is refused, never silently dropped.
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Event],
    ) -> Result<(), FinalizeError> {
        let _ = (new, prev);
        for event in events {
            match event {
                Event::ExitMathContext => {
                    return Err(FinalizeError::new(
                        "the ExitMathContext event needs the enclosing-state stack: \
                         derive through a parse context (cx.derive_state), where the \
                         driver lowers it via exit_math_context_delta",
                    ));
                }
            }
        }
        Ok(())
    }

    /// The standard scope-stack fold: every provider is consulted innermost-first,
    /// the longest match wins ([`ScopeStack::scan_specials`]).
    fn scan_specials<'s>(
        state: &ParsingState<Self>,
        content: &'s str,
        pos: usize,
    ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
        state.scopes().scan_specials(state, content, pos)
    }

    /// The trigger-character union over the state's providers
    /// ([`ScopeStack::specials_trigger_chars`]), cached per frozen state.
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        data.scopes.specials_trigger_chars()
    }

    /// The preset carries no per-node ext data (`NodeExt = ()`): the mint is the
    /// empty one-liner.
    fn make_node_ext(
        _kind: &crate::node::NodeKind<Self>,
        _span: &crate::source::SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) {
    }
}

/// [`Latexlike`] opts into its own language family with zero code: its
/// vocabularies are the preset enums (which implement the role traits), and every
/// [`LatexlikeLang`] behavior default is exactly the preset behavior.
impl LatexlikeLang for Latexlike {}

/// The preset's canonical [`TokenRules`]: `\` + letters commands (single non-letter
/// characters form single-character commands like `\&` by the tokenizer's standard
/// rule), `{…}` content groups, the four math delimiter pairs (`$…$`, `$$…$$`,
/// `\(…\)`, `\[…\]` — all class [`GroupType::Math`]; `$` vs. `$$` at a close position
/// is disambiguated by the tokenizer's expected-close rule), `%` comments, standard
/// whitespace with multi-newline paragraph breaks, and specials enabled (recognition
/// itself lives in the scope stack's providers).
///
/// `[`/`]` are deliberately **not** group delimiters: in LaTeX they are plain
/// characters outside optional-argument positions (`a [b] c` is plain text), and the
/// optional-argument parser recognizes them through a temporary group rule
/// ([`TokenRules::temporary_groups`]) exactly where an optional argument may sit
/// (decided at the 7.5 checkpoint).
///
/// Whitespace is the **ASCII** set (space, tab, `\n`, `\r`, vertical tab, form feed) —
/// deliberately *not* Unicode-aware (unlike pylatexenc's `str.isspace()`). A Unicode
/// space (NBSP U+00A0, U+2028, …) is ordinary content here, so e.g. an NBSP after
/// `\emph` becomes a content char rather than being swallowed as post-macro space
/// (decided for determinism and a fixed char-set model).
///
/// Each math rule declares its [`MathGroupForm`] as class payload
/// ([`GroupType::Math`]): `$…$`/`\(…\)` are [`Inline`](MathGroupForm::Inline),
/// `$$…$$`/`\[…\]` are [`Display`](MathGroupForm::Display) — read back from parsed
/// nodes via [`NodeRef::math_form`](crate::node::NodeRef::math_form), with no
/// delimiter table anywhere.
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]): the group classes
/// come from the language's own vocabulary through its role-trait constructors
/// ([`content_group`](LatexlikeGroupType::content_group)) and its
/// [`math_group_rules`](LatexlikeLang::math_group_rules) behavior default — a
/// family member adopts or overrides the delimiter table without forking this
/// function.
pub fn default_token_rules<LLL: LatexlikeLang>() -> TokenRules<LLL> {
    let mut groups = vec![Arc::new(GroupRule {
        group_type: LLL::GroupTypeId::content_group(),
        open: "{".into(),
        close: "}".into(),
    })];
    groups.extend(LLL::math_group_rules());

    TokenRules {
        enable_whitespace: true,
        whitespace: WhitespaceRules { chars: " \t\n\r\u{000B}\u{000C}".into() },
        enable_multi_newline_paragraphs: true,
        enable_groups: true,
        groups,
        temporary_groups: Vec::new(),
        enable_commands: true,
        commands: vec![Arc::new(CommandRule {
            escape_char: '\\',
            name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
        })],
        enable_comments: true,
        comments: vec![Arc::new(CommentRule { start: "%".into() })],
        enable_specials: true,
        forbidden_chars: "".into(),
        expecting_group_close: None,
    }
}

/// The seed package `"base"`: the environment dispatch pair and the standard
/// specials of pylatexenc's default context (bar the paragraph-break special; see
/// below).
///
/// - The [`Macro`](CallableType::Macro) entries `begin` ([`BeginSpec`] — the
///   environment composition) and `end` ([`EndSpec`] — orphan-`\end` diagnostics):
///   ordinary definitions, decided at the 7.6 checkpoint — data in the scope stack,
///   not driver code, so they are shadowable and unloadable like anything else.
/// - The specials: alignment `&` and non-breaking space `~` (visible in every mode),
///   plus the text-only typography ligatures ``` `` ```, `''`, `--`, `---` (visible in
///   [`Mode::Text`] only — they carry no math meaning, so inside `$…$` they stay plain
///   chars) — each a zero-argument [`SpecialsSpec`] callable sharing one instance
///   (many-to-one is the package flyweight contract). The multi-character triggers ride
///   the scope-stack scan's longest-match rule (`---` beats `--`).
///
/// pylatexenc's default context also ships a `\n\n` paragraph-break special; the
/// preset deliberately omits it — a multi-newline break is a whitespace chars node
/// here (via
/// [`enable_multi_newline_paragraphs`](TokenRules::enable_multi_newline_paragraphs)),
/// not a specials node.
///
/// Seeded onto the stack by [`Latexlike::initial_state_data`]; drop it with an
/// [`Unload`](crate::scopes::ScopeOp::Unload) op naming `"base"` (which also removes
/// `\begin`/`\end`), or shadow single entries by pushing a provider above it.
pub fn base_package() -> Package<Latexlike> {
    let mut package = Package::new("base");
    package.insert(CallableType::Macro, environments::BEGIN_COMMAND_NAME, Arc::new(BeginSpec));
    package.insert(CallableType::Macro, environments::END_COMMAND_NAME, Arc::new(EndSpec));
    let spec: Arc<dyn CallableSpec<Latexlike>> = Arc::new(SpecialsSpec::default());
    // Alignment `&` and non-breaking space `~` occur in text and math alike.
    // ### PhF -- We should not include & here
    for trigger in ["&", "~"] {
        package.insert_specials(CallableType::Specials, trigger, Arc::clone(&spec));
    }
    // Typography ligatures are text-mode only (no math meaning): inside `$…$` they stay
    // plain chars (7.5 review — per-entry mode visibility).
    for trigger in ["``", "''", "--", "---"] {
        package.insert_specials_in_modes(
            CallableType::Specials,
            trigger,
            Arc::clone(&spec),
            Some(vec![Mode::Text]),
        );
    }

    // ### PhF --- The "base" package should:
    // ###       1) be renamed to something internal, like "_base" or "_builtin" or "_primitive".  Collects the absolutely necessary things that MUST be preloaded in any latexlike parsing situation.
    // ###       2) NOT contain the specials above.  No reason that any latexlike parser should count those as specials.  Should be included in techy::latexlike::minidefs/minilatex package instead.

    package
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{macro_package, parse_shapes, root_shapes, strict, tolerant};
    use crate::engine::Language;
    use crate::error::Severity;
    use crate::node::check_tree_invariants;
    use crate::scopes::ScopeOp;
    use crate::state::ParsingStateDelta;

    // --- seed & default rules ---------------------------------------------------------

    #[test]
    fn the_seed_state_has_the_canonical_defaults() {
        let language = strict();
        let seed = language.initial_state();
        assert_eq!(seed.mode(), Mode::Text);
        assert_eq!(seed.scopes().provider_names().collect::<Vec<_>>(), ["base"]);
        assert_eq!(seed.rules(), &default_token_rules());
    }

    #[test]
    fn the_seed_is_finalize_coherent() {
        // The initial_state_data() contract: deriving with an empty delta must be
        // data-equivalent to the seed itself (pins the coherence obligation).
        let seed = ParsingState::<Latexlike>::lang_initial();
        let rederived = seed.derived(&ParsingStateDelta::new()).unwrap();
        assert_eq!(seed.rules(), rederived.rules());
        assert_eq!(seed.mode(), rederived.mode());
        assert_eq!(
            seed.scopes().provider_names().collect::<Vec<_>>(),
            rederived.scopes().provider_names().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn default_rules_tokenize_the_latex_shapes() {
        assert_eq!(
            parse_shapes("hello {world}!"),
            ["chars(hello )", "group(Content { })", "chars(!)"]
        );
    }

    #[test]
    fn brackets_are_plain_characters() {
        // `[`/`]` are not group delimiters in the default rules (7.5 checkpoint):
        // outside optional-argument positions they are plain text, as in LaTeX.
        assert_eq!(parse_shapes("a [b] c"), ["chars(a [b] c)"]);
    }

    #[test]
    fn comments_parse_to_comment_nodes() {
        assert_eq!(
            parse_shapes("a% note\nb"),
            ["chars(a)", "comment( note)", "chars(b)"]
        );
    }

    #[test]
    fn paragraph_breaks_split_content() {
        // Multi-newline paragraph break: the default paragraph node is a
        // whitespace-only chars node over the full break token.
        assert_eq!(
            parse_shapes("a\n\nb"),
            ["chars(a)", "chars(\n\n)", "chars(b)"]
        );
    }

    #[test]
    fn paragraph_breaks_can_emit_specials_nodes() {
        // ParagraphBreakStyle::Specials (7.9): pylatexenc-modern's paragraph shape —
        // a Specials-formed callable named "\n\n".
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict)
                .with_paragraph_break_style(ParagraphBreakStyle::Specials),
            ParsingState::lang_initial(),
        );
        let result = language.parse("a\n\nb").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(root_shapes(&result), ["chars(a)", "Specials(\n\n)", "chars(b)"]);
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n\n"));
        assert_eq!(break_node.span().range(), 1..3);

        // The name is canonical "\n\n" (the vocabulary key); the span covers the
        // actual whitespace run of the break token.
        let result = language.parse("a\n \t\nb").unwrap();
        check_tree_invariants(&result.tree);
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n\n"));
        assert_eq!(break_node.span().content(), "\n \t\n");
    }

    // --- math modes -------------------------------------------------------------------

    #[test]
    fn math_group_interiors_parse_in_math_mode() {
        let result = strict().parse("a $x+y$ b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(
            root_shapes(&result),
            ["chars(a )", "group(Math(Inline) $ $)", "chars( b)"]
        );

        let math = result.tree.root().child(1).unwrap();
        let interior = math.child(0).unwrap();
        assert_eq!(interior.chars(), Some("x+y"));
        // Mode entry: the interior's recorded state is math.
        assert_eq!(interior.parsing_state().mode(), Mode::Math);
        // The group node itself and the following content are back in text mode
        // (structural reversion — the outer Arc is restored).
        assert_eq!(math.parsing_state().mode(), Mode::Text);
        let after = result.tree.root().child(2).unwrap();
        assert_eq!(after.parsing_state().mode(), Mode::Text);
    }

    #[test]
    fn content_groups_inside_math_stay_in_math_mode() {
        // `{…}` does not exit math mode: its interior inherits the surrounding state.
        let result = strict().parse("${a}$").unwrap();
        let math = result.tree.root().child(0).unwrap();
        let brace = math.child(0).unwrap();
        assert_eq!(brace.group_type(), Some(GroupType::Content));
        assert_eq!(brace.child(0).unwrap().parsing_state().mode(), Mode::Math);
    }

    #[test]
    fn dollar_dollar_boundaries_close_before_they_open() {
        // `$a$$b$` is two inline groups (the expected-close disambiguation), not a
        // display group — pylatexenc parity.
        let result = strict().parse("$a$$b$").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(
            root_shapes(&result),
            ["group(Math(Inline) $ $)", "group(Math(Inline) $ $)"]
        );
        let first = result.tree.root().child(0).unwrap();
        let second = result.tree.root().child(1).unwrap();
        assert_eq!(first.child(0).unwrap().chars(), Some("a"));
        assert_eq!(second.child(0).unwrap().chars(), Some("b"));
    }

    #[test]
    fn stray_dollar_in_math_is_forbidden_not_a_nested_open() {
        // Inside math LaTeX forbids nested math: a lone `$` cannot open a nested group.
        // Strict aborts on the forbidden `$`; tolerant recovers it as a char and the
        // enclosing group still closes normally (7.5 review — #9).
        assert!(strict().parse("$$a$b$$").is_err());

        let result = tolerant().parse("$$a$b$$").unwrap();
        check_tree_invariants(&result.tree);
        // One display group at the root — it closes on the trailing `$$`, never leaving
        // an unclosed nested inline group.
        assert_eq!(root_shapes(&result), ["group(Math(Display) $$ $$)"]);
        let display = result.tree.root().child(0).unwrap();
        let interior: String = display.children().iter().filter_map(|child| child.chars()).collect();
        assert_eq!(interior, "a$b");
        // Exactly one diagnostic: the forbidden `$`.
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn display_math_delimiters() {
        assert_eq!(parse_shapes("$$ab$$"), ["group(Math(Display) $$ $$)"]);
        assert_eq!(parse_shapes(r"\[x\]"), [r"group(Math(Display) \[ \])"]);
        assert_eq!(parse_shapes(r"\(x\)"), [r"group(Math(Inline) \( \))"]);
    }

    // --- scope stack: commands, visibility, specials ----------------------------------

    /// A `recovery` language seeded with the zero-argument macro `\alpha` in a
    /// package `"alphapkg"`, optionally math-only (package-level visibility).
    fn with_alpha(recovery: crate::error::Recovery, math_only: bool) -> Language<Latexlike> {
        let modes = math_only.then(|| vec![Mode::Math]);
        test_support::with_package(recovery, macro_package("alphapkg", "alpha", modes))
    }

    #[test]
    fn commands_resolve_through_the_scope_stack() {
        let language = with_alpha(crate::error::Recovery::Strict, false);
        let result = language.parse(r"\alpha x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(root_shapes(&result), ["Macro(alpha)", "chars(x)"]);
    }

    #[test]
    fn mode_visibility_gates_package_definitions() {
        let language = with_alpha(crate::error::Recovery::Tolerant, true);

        // Inside math: the package is visible, `\alpha` resolves.
        let math = language.parse(r"$\alpha$").unwrap();
        let group = math.tree.root().child(0).unwrap();
        assert_eq!(group.child(0).unwrap().summary(), "Macro(alpha)");
        assert!(math.diagnostics.is_empty());

        // In text mode the package answers "not here": unresolvable, recovered as
        // chars under the tolerant policy, with the searched providers as detail.
        let text = language.parse(r"\alpha").unwrap();
        assert_eq!(root_shapes(&text), [r"chars(\alpha)"]);
        assert_eq!(text.diagnostics.len(), 1);
        let message = text.diagnostics.iter().next().unwrap().message();
        assert!(message.contains("searched providers: alphapkg, base"), "{message}");
    }

    #[test]
    fn unknown_commands_abort_strict_and_recover_tolerant() {
        let err = strict().parse(r"a \foo b").unwrap_err();
        assert!(err.to_string().contains("cannot resolve command ‘\\foo’"), "{err}");

        let result = tolerant().parse(r"a \foo b").unwrap();
        assert_eq!(
            root_shapes(&result),
            ["chars(a )", r"chars(\foo )", "chars(b)"]
        );
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn unresolvable_command_diagnostic_carries_span_severity_and_detail() {
        // The searched-providers detail rides the strict abort error too — not only the
        // tolerant diagnostic (guards the strict path from silently dropping it).
        let err = strict().parse(r"a \foo b").unwrap_err();
        assert!(
            err.to_string().contains("searched providers: base"),
            "strict error lost the searched-providers detail: {err}"
        );

        // The tolerant diagnostic is Error-severity and spans the unresolved command
        // token exactly (guards against a zero-width or post-space-shifted span).
        let result = tolerant().parse(r"a \foo b").unwrap();
        let diag = result.diagnostics.iter().next().unwrap();
        assert_eq!(diag.severity(), Severity::Error);
        assert_eq!(diag.span().content(), r"\foo ");
        assert!(
            diag.message().contains("searched providers: base"),
            "{}",
            diag.message()
        );
    }

    #[test]
    fn base_specials_parse_out_of_the_box() {
        assert_eq!(
            parse_shapes("a~b & c"),
            ["chars(a)", "Specials(~)", "chars(b )", "Specials(&)", "chars( c)"]
        );
    }

    #[test]
    fn ligature_specials_take_the_longest_match() {
        assert_eq!(
            parse_shapes("x---y--z"),
            ["chars(x)", "Specials(---)", "chars(y)", "Specials(--)", "chars(z)"]
        );
        assert_eq!(
            parse_shapes("``q''"),
            ["Specials(``)", "chars(q)", "Specials('')"]
        );
        // `` !` `` and `` ?` `` were dropped from the base package (PhF, July 2026 —
        // see the `base_package` note): plain characters now.
        assert_eq!(parse_shapes("!`Si?`"), ["chars(!`Si?`)"]);
    }

    #[test]
    fn base_ligature_specials_are_text_only() {
        // Inside math the text-only typography ligatures stay plain chars, while the
        // universal specials `~`/`&` still fire (7.5 review — per-entry mode visibility).
        let result = strict().parse("$a~b---c$").unwrap();
        check_tree_invariants(&result.tree);
        let math = result.tree.root().child(0).unwrap();
        assert!(matches!(math.group_type(), Some(GroupType::Math(_))));
        let interior: Vec<String> =
            math.children().iter().map(|node| node.summary()).collect();
        assert_eq!(interior, ["chars(a)", "Specials(~)", "chars(b---c)"]);

        // In text mode the same ligature fires as before.
        assert_eq!(parse_shapes("a---b"), ["chars(a)", "Specials(---)", "chars(b)"]);
    }

    #[test]
    fn trigger_chars_without_a_match_stay_plain() {
        // `!`, `?`, `'`, `` ` `` are trigger *first characters*, but alone (no
        // following backtick / quote pair) the scan declines and they remain chars.
        assert_eq!(parse_shapes("a!b?c'd`e"), ["chars(a!b?c'd`e)"]);
    }

    // --- exit-math event: the \text recipe (E4) ---------------------------------------

    use crate::constructs::GroupArgumentParser;
    use crate::spec::ArgumentSpec;

    /// The `\text` recipe: a mandatory content-group argument whose state delta
    /// carries the exit-math-context **event** (never a static rules reset).
    fn text_argument() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(
            ArgumentSpec::new_unnamed(GroupArgumentParser::new(GroupType::Content))
                .with_state_delta(ParsingStateDelta::new().event(Event::ExitMathContext)),
        )
    }

    #[test]
    fn exit_math_event_restores_text_context_inside_math() {
        let mut package = Package::new("mydefs");
        package.insert(
            CallableType::Macro,
            "text",
            Arc::new(super::MacroSpec::new(vec![text_argument()])),
        );
        let language = test_support::with_package(crate::error::Recovery::Strict, package);

        let result = language.parse(r"\[ x \text{if $y<0$} \]").unwrap();
        check_tree_invariants(&result.tree);
        let math = result.tree.root().child(0).unwrap();
        let text = math.child(1).unwrap();
        assert_eq!(text.macro_name(), Some("text"));

        // The argument parses back in the enclosing text context…
        let argument = text.argument_content_nodes(0).unwrap();
        assert_eq!(argument.get(0).unwrap().parsing_state().mode(), Mode::Text);
        // …with the math delimiters restored as openers: the nested `$…$` is a
        // math group again…
        let nested = argument.get(1).unwrap();
        assert!(nested.is_math_group());
        assert_eq!(nested.child(0).unwrap().parsing_state().mode(), Mode::Math);
        // …and the restore is scoped to the argument: after `\text{…}` the
        // enclosing display math continues in math mode.
        let after = math.child(2).unwrap();
        assert_eq!(after.parsing_state().mode(), Mode::Math);
    }

    #[test]
    fn exit_math_event_restores_the_innermost_non_math_context_not_the_seed() {
        // `\wrap`'s argument installs an embedder-customized context (forbidden
        // `!`, a custom `«»` content pair). `\text` inside math entered from that
        // context must restore THAT context — custom rules and forbidden set
        // included — never the seed and never a static reset.
        let custom_group = Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "«".into(),
            close: "»".into(),
        });
        let mut wrap_groups = default_token_rules::<Latexlike>().groups;
        wrap_groups.push(Arc::clone(&custom_group));
        let wrap_argument = Arc::new(
            ArgumentSpec::new_unnamed(GroupArgumentParser::new(GroupType::Content))
                .with_state_delta(ParsingStateDelta::new().rules(
                    crate::state::TokenRulesOverrides {
                        groups: Some(wrap_groups),
                        forbidden_chars: Some("!".into()),
                        ..crate::state::TokenRulesOverrides::default()
                    },
                )),
        );
        let mut package = Package::new("mydefs");
        package.insert(
            CallableType::Macro,
            "wrap",
            Arc::new(super::MacroSpec::new(vec![wrap_argument])),
        );
        package.insert(
            CallableType::Macro,
            "text",
            Arc::new(super::MacroSpec::new(vec![text_argument()])),
        );
        let language = test_support::with_package(crate::error::Recovery::Strict, package);

        let result = language.parse(r"\wrap{$a\text{«q»}b$}").unwrap();
        check_tree_invariants(&result.tree);
        let wrap = result.tree.root().child(0).unwrap();
        let math = wrap.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert!(math.is_math_group());

        // Math entry merged the derived chars into the *embedder's* forbidden set.
        let in_math = math.child(0).unwrap();
        assert_eq!(in_math.chars(), Some("a"));
        let math_forbidden = &*in_math.parsing_state().rules().forbidden_chars;
        assert!(math_forbidden.contains('!') && math_forbidden.contains('$'));

        // The \text argument came back to the wrap-argument context: text mode,
        // the embedder's forbidden set (no `$`), and the custom `«»` pair parsing
        // as a group again.
        let text = math.child(1).unwrap();
        assert_eq!(text.macro_name(), Some("text"));
        let restored_group = text.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert_eq!(restored_group.group_delimiters(), Some(("«", "»")));
        let restored_state = restored_group.parsing_state();
        assert_eq!(restored_state.mode(), Mode::Text);
        assert_eq!(&*restored_state.rules().forbidden_chars, "!");

        // After the argument, the math context resumes untouched.
        let after = math.child(2).unwrap();
        assert_eq!(after.chars(), Some("b"));
        assert_eq!(after.parsing_state().mode(), Mode::Math);
    }

    #[test]
    fn text_argument_close_expectation_survives_a_foreign_restored_expectation() {
        // Discriminating shape (S4 review should-fix): in the sibling test the
        // restored context's `expecting_group_close` and the `\text` argument's
        // own close share one spelling (`}`), so a regression where the
        // argument-group descent failed to re-install its own close-expectation
        // over the restored one would be invisible. Here the outer `\wrap`
        // argument is a `«»`-delimited group, so the restored (first non-math)
        // context *expects `»`* — different from the `\text` argument's `{`/`}`.
        let custom_group = Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "«".into(),
            close: "»".into(),
        });
        let wrap_argument = Arc::new(ArgumentSpec::new_unnamed(GroupArgumentParser::new(
            GroupType::Content,
        )));
        let mut package = Package::new("mydefs");
        package.insert(
            CallableType::Macro,
            "wrap",
            Arc::new(super::MacroSpec::new(vec![wrap_argument])),
        );
        package.insert(
            CallableType::Macro,
            "text",
            Arc::new(super::MacroSpec::new(vec![text_argument()])),
        );

        let mut groups = default_token_rules::<Latexlike>().groups;
        groups.push(Arc::clone(&custom_group));
        let seed = ParsingState::lang_initial_with_packages([package])
            .derived(&ParsingStateDelta::new().rules(crate::state::TokenRulesOverrides {
                groups: Some(groups),
                ..crate::state::TokenRulesOverrides::default()
            }))
            .unwrap();
        let language =
            Language::new(LatexlikeDriver::new(crate::error::Recovery::Strict), seed);

        let result = language.parse(r"\wrap«$$a\text{y}b$$»").unwrap();
        check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

        // The trailing `»` closed the wrap argument: the region node is the
        // `«»`-delimited group, with the display math inside it.
        let wrap = result.tree.root().child(0).unwrap();
        assert_eq!(wrap.macro_name(), Some("wrap"));
        let wrap_group = wrap.argument_nodes(0).unwrap().get(0).unwrap();
        assert_eq!(wrap_group.group_delimiters(), Some(("«", "»")));
        let math = wrap.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert!(math.is_math_group());
        assert_eq!(math.math_form(), Some(MathGroupForm::Display));

        let text = math.child(1).unwrap();
        assert_eq!(text.macro_name(), Some("text"));
        // The restored context really is the `«»` interior: the `\text`
        // argument's state (recorded on its group node) expects `»`…
        let argument_group = text.argument_nodes(0).unwrap().get(0).unwrap();
        let restored_state = argument_group.parsing_state();
        assert_eq!(restored_state.mode(), Mode::Text);
        assert_eq!(
            restored_state
                .rules()
                .expecting_group_close
                .as_ref()
                .map(|rule| &*rule.close),
            Some("»")
        );
        // …yet the argument group still closes on its OWN `}` (the descent
        // invariant re-installed the entered rule's close over the restored
        // expectation), with the content in the restored text context.
        assert_eq!(argument_group.group_delimiters(), Some(("{", "}")));
        let content = text.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert_eq!(content.chars(), Some("y"));
        assert_eq!(content.parsing_state().mode(), Mode::Text);

        // After the argument the display math resumes in math mode.
        let after = math.child(2).unwrap();
        assert_eq!(after.chars(), Some("b"));
        assert_eq!(after.parsing_state().mode(), Mode::Math);
    }

    #[test]
    fn exit_math_event_in_bare_derived_is_refused_loudly() {
        // The two-class contract's loud arm on the preset: out of any parse the
        // context does not exist, so finalize_transition refuses the event.
        let seed = ParsingState::<Latexlike>::lang_initial();
        let error =
            seed.derived(&ParsingStateDelta::new().event(Event::ExitMathContext)).unwrap_err();
        let finalize_error = error.finalize_error.as_ref().unwrap();
        assert!(finalize_error.message().contains("ExitMathContext"));
    }

    #[test]
    fn the_generic_driver_serves_a_foreign_family_member() {
        // A foreign Lang adopting the preset vocabularies joins the family with
        // zero role code and reuses LatexlikeDriver<LLL>, default_token_rules,
        // the pillars, and the NodeRef sugar (MacroSpec/EnvironmentSpec stay
        // Latexlike-monomorphic until the invocation-syntax stage — the generic
        // StdCallableSpec carries the \text recipe here).
        use crate::spec::StdCallableSpec;

        #[derive(Debug, Clone, Copy)]
        struct Flavored;
        impl Lang for Flavored {
            type GroupTypeId = GroupType;
            type CallableTypeId = CallableType;
            type ModeId = Mode;
            type StateExt = ();
            type Event = Event;
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type Driver = LatexlikeDriver<Flavored>;

            fn initial_state_data() -> StateData<Self> {
                StateData {
                    rules: default_token_rules(),
                    scopes: ScopeStack::new(),
                    mode: Mode::Text,
                    ext: (),
                }
            }
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &Arc<ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) {
            }
        }
        impl super::LatexlikeLang for Flavored {}

        let text_spec: StdCallableSpec<Flavored> = StdCallableSpec::new([
            ArgumentSpec::new_unnamed(GroupArgumentParser::new(GroupType::Content))
                .with_state_delta(ParsingStateDelta::new().event(Event::ExitMathContext)),
        ]);
        let mut package: Package<Flavored> = Package::new("defs");
        package.insert(CallableType::Macro, "text", Arc::new(text_spec));

        let language: Language<Flavored> = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]),
        );
        let result = language.parse(r"a $x\text{ b }y$ c").unwrap();
        check_tree_invariants(&result.tree);

        let math = result.tree.root().child(1).unwrap();
        // The generic NodeRef sugar works on the foreign tree…
        assert!(math.is_math_group());
        assert_eq!(math.math_form(), Some(MathGroupForm::Inline));
        // …the math plug entered math mode…
        assert_eq!(math.child(0).unwrap().parsing_state().mode(), Mode::Math);
        // …and the exit-math event restored the enclosing text context inside
        // the \text argument (whitespace tokenization included).
        let text = math.child(1).unwrap();
        let argument = text.argument_content_nodes(0).unwrap();
        assert_eq!(argument.get(0).unwrap().parsing_state().mode(), Mode::Text);
    }

    #[test]
    fn the_base_package_is_unloadable_by_name() {
        let seed = ParsingState::<Latexlike>::lang_initial()
            .derived(
                &ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "base".into() }),
            )
            .unwrap();
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            seed,
        );
        let result = language.parse("a~b --- c").unwrap();
        assert_eq!(root_shapes(&result), ["chars(a~b --- c)"]);
    }
}
