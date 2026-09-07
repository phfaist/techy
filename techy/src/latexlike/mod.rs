//! The LaTeX-like language preset: the familiar meaning of `\`, `{`, `}`, `%`, math
//! modes, and environments.
//!
//! techy's parsing engine has no built-in LaTeX behavior. This module supplies it, as
//! preset data and preset code written against the same public extension points a
//! language of your own would use. [`Latexlike`] declares the language;
//! [`LatexlikeDriver`] carries the parse-time policy.
//!
//! # Parsing a document
//!
//! A [`Language`](crate::core::Language) pairs the driver with the parsing state a
//! parse starts from, and parses any number of documents:
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::{Latexlike, LatexlikeDriver, Mode};
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial().expect("seed state"),
//! );
//! let result = language.parse("inline $x+y$ math").unwrap();
//! let math = result.tree.root().child(1).unwrap();
//! assert!(math.is_math_group());
//! assert_eq!(math.child(0).unwrap().parsing_state().mode(), Mode::Math);
//! ```
//!
//! [`ParsingState::lang_initial`](crate::core::ParsingState::lang_initial) builds the
//! canonical starting state: the preset's token rules ([`default_token_rules`]) and a
//! scope stack holding the [`builtin_package`].
//!
//! # Definitions are yours to supply
//!
//! The preset defines the syntax, not a library of macros. `\emph`, `\textbf`,
//! `itemize` and the rest are undefined until a program registers them: the only
//! preloaded definitions are the `\begin` and `\end` pair of [`builtin_package`], and
//! the standard definitions database (a port of pylatexenc's default specs) is planned
//! but not yet available.
//!
//! Register what you need in your own [`Package`] and
//! seed the parsing state with it
//! ([`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)),
//! as [`MacroSpec`], [`EnvironmentSpec`], and [`SpecialsSpec`] entries — or as any
//! custom [`CallableSpec`](crate::core::specs::CallableSpec). `\verb` and `verbatim`
//! are no exception: the machinery for them is here ([`VerbatimBehavior`] and the `v`
//! argument codes), the definitions are not. The guide chapter [Defining macros,
//! environments, and specials](crate::guide::specs) walks through registration, and
//! [`minidefs`] is a toy package of familiar definitions to load explicitly while
//! prototyping.
//!
//! An unregistered command is not a syntax error: a strict parse stops at it, and a
//! tolerant one records a diagnostic and keeps the characters as text
//! ([`Recovery`](crate::error::Recovery)).
//!
//! # The preset's vocabulary
//!
//! Node *kinds* stay the core's ([`NodeKind`](crate::core::node::NodeKind): characters,
//! group, callable, comment, list). What makes a tree latexlike are the four closed
//! vocabularies the preset stamps on those nodes:
//!
//! - [`CallableType`] — the invocation forms: a macro (`\emph{…}`), an environment
//!   (`\begin{itemize}…\end{itemize}`), or specials, a trigger character sequence such
//!   as `~` or `---`.
//! - [`GroupType`] — the group classes: a content group (`{…}`), a math group, or a
//!   verbatim region. A math group also records its [`MathGroupForm`], inline or
//!   display.
//! - [`Mode`] — text or math. A math group's interior parses in [`Mode::Math`], and
//!   definition visibility can be restricted to one mode.
//! - [`Event`] — the one state-transition event, "leave the math context": what an
//!   argument that must parse in the surrounding text context (the `\text{…}` shape)
//!   asks for.
//!
//! After a parse, the preset's accessors on [`NodeRef`](crate::core::node::NodeRef)
//! read that vocabulary back off a node —
//! [`macro_name`](crate::core::node::NodeRef::macro_name),
//! [`environment_name`](crate::core::node::NodeRef::environment_name),
//! [`specials_name`](crate::core::node::NodeRef::specials_name),
//! [`is_math_group`](crate::core::node::NodeRef::is_math_group),
//! [`math_form`](crate::core::node::NodeRef::math_form), and
//! [`post_space`](crate::core::node::NodeRef::post_space).
//!
//! # What else is here
//!
//! - **Defining callables** — [`MacroSpec`], [`SpecialsSpec`], and [`EnvironmentSpec`]
//!   (declared arguments plus body behavior, [`EnvironmentBehavior`]), configured by
//!   the argument codes [`argument_specs`] and [`argument_specs_from_str`]
//!   (`["o", "m"]` for an optional argument then a mandatory one). [`BeginSpec`] and
//!   [`EndSpec`] are the definitions of `\begin` and `\end` themselves.
//! - **Verbatim content** — [`VerbatimBehavior`] for `verbatim`-style environment
//!   bodies, and the `v` argument codes for `\verb`-style delimited arguments.
//! - **Including other sources** — [`input_macro_spec`], an `\input`-shaped definition
//!   that parses the referenced source into the same tree at the point of invocation.
//!   It is never preloaded and needs a
//!   [`SourceResolver`](crate::source::SourceResolver) on the driver.
//! - **Writing source back out** — [`source_recomposer`] re-emits a parsed tree as
//!   source text.
//! - **Serialization** — [`serialize`]: [`Latexlike`], its vocabularies, and its spec
//!   types convert to and from techy's format-independent value model.
//! - **Extending the preset** — the generic items here are written over a *family* of
//!   languages ([`LatexlikeLang`], conventionally the type parameter `LLL`) rather than
//!   over [`Latexlike`] alone, so a language with its own vocabularies, node data, or
//!   driver ([`LatexlikeParseDriver`]) reuses the preset's rules, specs, and parse
//!   behavior instead of copying them.
//!
//! The guide chapter [Language syntax](crate::guide::language_syntax) describes what
//! the preset accepts, and [Learn techy by example](crate::guide::learn_by_example)
//! parses, defines, and reads a document from beginning to end.

mod arguments;
mod driver;
mod environments;
mod input;
#[cfg(test)]
mod invariants;
mod invocation_syntax;
mod lang;
pub mod minidefs;
mod node_ref;
mod recompose;
pub mod serialize;
#[cfg(test)]
mod serialize_tests;
mod spec;
#[cfg(test)]
mod span_tiling_tests;
#[cfg(test)]
mod test_support;

pub use arguments::{
    argument_specs, argument_specs_from_str, argument_specs_named, ArgumentCodeError,
};
pub use driver::{
    exit_math_context_delta, make_paragraph_break_node, math_group_interior_delta,
    LatexlikeDriver, LatexlikeParseDriver, ParagraphBreakSpec, ParagraphBreakStyle,
};
pub use environments::{
    BeginSpec, EndSpec, EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec,
    MalformedBegin, OrphanEnd, UnknownEnvironment, VerbatimBehavior,
};
pub use input::{input_macro_spec, InputMacroSpec};
pub use invocation_syntax::{
    EnvironmentSyntax, EnvironmentSyntaxError, InvocationSyntaxData,
    StdEnvironmentSideSyntax, StdEnvironmentSyntax,
};
pub use lang::{
    LatexlikeCallableType, LatexlikeEvent, LatexlikeGroupType, LatexlikeInvocationSyntax,
    LatexlikeLang, LatexlikeMode,
};
pub use recompose::{source_recomposer, SourceRecomposeError, SourceRecomposer};
pub use spec::{MacroSpec, SpecialsSpec};

// The latexlike span-tiling oracle (the core span-tiling law + the payload pins) — the
// preset-side sibling of core's `check_tree_invariants` mechanism (in-crate test
// utility, never public; D-plan-12 Option B).
#[cfg(test)]
pub(crate) use invariants::check_latexlike_tree_invariants;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::node::BodySlotExt;
use crate::scopes::{Package, ScopeStack};
use crate::serialize::{DeserializableValue, SerializableValue};
use crate::state::{
    AllLangFeatures, ClosedVocabulary, FinalizeError, Lang, NodeExtTypes, ParsingState,
    StateData,
};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsMatch, SpecialsRules, SpecialsScanError,
    StdTokenization, TokenRules, TriggerChars, WhitespaceRules,
};

/// The form in which a math group appears: inline or display.
///
/// The form is the payload of the [`GroupType::Math`] class. Whoever registers a
/// delimiter pair states it there, on the [`GroupRule`], so reading it back off a
/// parsed node ([`NodeRef::math_form`](crate::core::node::NodeRef::math_form)) needs no
/// table of delimiter spellings and works for delimiters an embedder registered or a
/// parse introduced mid-document.
///
/// The name is *form*, not "style": typesetting style (fonts, script level,
/// `\displaystyle`) is a separate matter, and `$\displaystyle …$` renders
/// display-*style* math inside an inline-*form* group. The type names how the math
/// group appears in the source, not how its content is typeset.
///
/// Both forms parse identically, so the form never affects parsing; it is there for
/// the consumers of the tree. The enum is not `#[non_exhaustive]` — consumers match on
/// it constantly — so adding a third form would be a breaking change.
// The payload-admission rule for `GroupType` class payloads: a payload is admissible
// only when it is (a) parse-behavior-invariant — the parse wiring keeps a single arm
// (`Math(_)` matches once; interior delta, visibility, and forbidden-char logic ignore
// the form); (b) semantically universal for downstream consumers of the class; and
// (c) declared at rule registration, never derived from delimiter spellings.
// Inline/display passes all three; a hypothetical `Content(BraceKind)` fails (b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(SerializableValue, DeserializableValue)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MathGroupForm {
    /// `$…$` / `\(…\)` — inline math.
    #[serial(name = "inline")]
    #[cfg_attr(feature = "serde", serde(rename = "inline"))]
    Inline,
    /// `$$…$$` / `\[…\]` — display math.
    #[serial(name = "display")]
    #[cfg_attr(feature = "serde", serde(rename = "display"))]
    Display,
}

/// The preset's group classes: the parse behavior a group's delimiters select.
///
/// A class says how the group's interior parses, not how the group is spelled: several
/// delimiter pairs can share one class, and the node's
/// [`GroupData`](crate::core::node::GroupData) records the delimiters as written.
///
/// Inline and display math share the single [`Math`](GroupType::Math) class, because
/// the two parse identically — same interior [`Mode::Math`], same definition
/// visibility. Which of the two a group is appears as the class payload
/// [`MathGroupForm`], stated where the delimiter rule is registered and read back with
/// [`NodeRef::math_form`](crate::core::node::NodeRef::math_form).
///
/// This is the preset's [`Lang::GroupTypeId`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(SerializableValue, DeserializableValue)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GroupType {
    /// A content group: `{…}`, and the argument groups argument parsers produce, such
    /// as an optional `[…]`. Its interior continues in the surrounding mode.
    #[serial(name = "content")]
    #[cfg_attr(feature = "serde", serde(rename = "content"))]
    Content,
    /// A math group (`$…$`, `$$…$$`, `\(…\)`, `\[…\]`): its interior parses in
    /// [`Mode::Math`], the state change the preset driver derives for it
    /// ([`math_group_interior_delta`], which treats both forms the same). The payload
    /// records the group's [`MathGroupForm`], stated where the delimiter rule is
    /// registered.
    #[serial(name = "math")]
    #[cfg_attr(feature = "serde", serde(rename = "math"))]
    Math(MathGroupForm),
    /// A verbatim region's group: the `\verb|…|` shape the `v` argument codes stage
    /// ([`argument_specs`]), and the class of the terminator rules verbatim readers
    /// create.
    ///
    /// Its interior is raw text, read under a state with the parsing features turned
    /// off rather than tokenized. This class therefore appears on no rule the tokenizer
    /// follows, and the driver's interior-state hook never sees it.
    #[serial(name = "verbatim")]
    #[cfg_attr(feature = "serde", serde(rename = "verbatim"))]
    Verbatim,
}

/// The preset's invocation forms: macro, environment, and specials.
///
/// The set of forms is closed. New *callables* are registered freely, in a
/// [`Package`] on the scope stack, but every one of them is invoked in one of these
/// three ways.
///
/// This is the preset's [`Lang::CallableTypeId`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(SerializableValue, DeserializableValue)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallableType {
    /// A macro invocation (`\emph{…}`). Every command token resolves as a macro:
    /// `\begin` and `\end` are themselves ordinary macro entries of the
    /// [`builtin_package`] ([`BeginSpec`] and [`EndSpec`]), whose parsers then handle
    /// the environment shape.
    #[serial(name = "macro")]
    #[cfg_attr(feature = "serde", serde(rename = "macro"))]
    Macro,
    /// An environment (`\begin{itemize}…\end{itemize}`). [`BeginSpec`] reads the name
    /// in the `\begin` group, resolves the environment's own definition — normally an
    /// [`EnvironmentSpec`] — under this callable type, and records this type on the
    /// resulting node.
    #[serial(name = "environment")]
    #[cfg_attr(feature = "serde", serde(rename = "environment"))]
    Environment,
    /// A specials invocation: a trigger character sequence (`~`, `&`, `---`).
    #[serial(name = "specials")]
    #[cfg_attr(feature = "serde", serde(rename = "specials"))]
    Specials,
}

/// The preset's parsing modes: text and math.
///
/// The mode is part of the parsing state ([`ParsingState::mode`]) and is the one place
/// that answers "is this inside math". Entering a math group is what changes it (the
/// state the preset driver derives for a math interior,
/// [`math_group_interior_delta`]), and a package can restrict its definitions to
/// chosen modes ([`Package::set_visible_modes`]).
///
/// Inline and display math are not separate modes, and not separate group classes
/// either: they parse identically, and the distinction is the class payload
/// [`MathGroupForm`].
///
/// This is the preset's [`Lang::ModeId`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[derive(SerializableValue, DeserializableValue)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Mode {
    /// Ordinary text content — the seed state's mode.
    #[default]
    #[serial(name = "text")]
    #[cfg_attr(feature = "serde", serde(rename = "text"))]
    Text,
    /// Math content (inside `$…$`, `$$…$$`, `\(…\)`, `\[…\]`).
    #[serial(name = "math")]
    #[cfg_attr(feature = "serde", serde(rename = "math"))]
    Math,
}

/// The preset's state-transition events: one, "leave the math context".
///
/// An event asks for a state change that cannot be written out in advance. This one's
/// effect depends on the states enclosing the point of use, so it is resolved while a
/// parse derives a state — the driver turns it into a concrete change
/// ([`ParseDriver::resolve_state_event`](crate::core::ParseDriver::resolve_state_event),
/// called from
/// [`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state)).
/// Deriving a state outside a parse ([`ParsingState::derived`]) has no enclosing states
/// to consult and returns an error rather than guessing; events whose effect does not
/// depend on the surroundings are handled by [`Lang::finalize_transition`] instead.
///
/// The events of a state change are declared on
/// [`ParsingStateDelta::events`](crate::core::ParsingStateDelta::events).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(SerializableValue, DeserializableValue)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Event {
    /// Leave the math context: restore the innermost enclosing *non-math* context —
    /// its [`TokenRules`] and its mode — or, if every enclosing context is math, the
    /// outermost one. The rules are restored except for the in-flight structural
    /// expectations `expecting_group_close` and `temporary_group_rules`.
    ///
    /// This is how `\text{…}` is defined: an
    /// [`ArgumentSpec`](crate::core::specs::ArgumentSpec) whose state change carries
    /// this event parses its argument in the surrounding non-math context, however deep
    /// inside display math it sits, and whatever token rules an embedder had customized
    /// there. The target is the enclosing context as it actually is, so nothing here
    /// invents a "text mode" or resets the rules to a fixed set.
    ///
    /// [`exit_math_context_delta`] computes the resulting state change.
    #[serial(name = "exit-math-context")]
    #[cfg_attr(feature = "serde", serde(rename = "exit-math-context"))]
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
/// ([`BodySlotExt`]).
///
/// The preset's environment machinery mints the marker via
/// [`BodySlotExt::make_body`]; a non-body slot's value comes from
/// [`not_body`](BodyMarker::not_body).
///
/// There is deliberately no `Default` — an ext value is always minted by the party
/// with the knowledge, never invented by a default (population is initialization,
/// [`NodeExtTypes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BodyMarker {
    #[cfg_attr(feature = "serde", serde(rename = "body"))]
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

/// The preset's node-ext bundle ([`Lang::NodeExts`]).
///
/// There is no per-node and no per-argument data (`NodeExt`/`ArgumentExt` are `()`),
/// while **`SlotExt` is claimed** for the [`BodyMarker`].
///
/// The preset marks environment body slots through the generic [`BodySlotExt`]
/// mechanism, so [`NodeRef::body`](crate::core::node::NodeRef::body) selects the
/// marked slot rather than relying on slot positions.
#[derive(Debug, Clone, Copy)]
pub struct LatexlikeNodeExts;

impl NodeExtTypes for LatexlikeNodeExts {
    type NodeExt = ();
    type ArgumentExt = ();
    type SlotExt = BodyMarker;
}

/// The latexlike language bundle: a ZST implementing [`Lang`].
///
/// It provides the preset's vocabularies ([`GroupType`], [`CallableType`], [`Mode`]),
/// the canonical seed ([`default_token_rules`] + the [`builtin_package`] on the scope
/// stack), and the scope-stack specials scan.
///
/// Parse-time behavior is defined by [`LatexlikeDriver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Latexlike;

impl Lang for Latexlike {
    /// The preset uses every parsing feature ([`AllLangFeatures`]) — it does not
    /// participate in compile-time feature gating.
    type Features = AllLangFeatures;
    type GroupTypeId = GroupType;
    type CallableTypeId = CallableType;
    type ModeId = Mode;
    type StateExt = ();
    type Event = Event;
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type Tokenization = StdTokenization;
    type NodeExts = LatexlikeNodeExts;
    type InvocationSyntax = InvocationSyntaxData;
    type Driver = LatexlikeDriver;

    /// The canonical latexlike seed: [`default_token_rules`], a scope stack holding
    /// the [`builtin_package`] (`\begin`/`\end` dispatch), [`Mode::Text`]. Built
    /// from in-crate constants, so it always answers `Ok` (the hook's `Result`
    /// exists for seeds built from external data).
    ///
    /// Coherence contract: `finalize_transition` is not customized (nothing to
    /// normalize yet — math groups only set the mode), so the seed is trivially
    /// finalize-coherent; a test pins `lang_initial().derived(&empty) == lang_initial()`
    /// data-equivalence mechanically.
    fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
        let mut scopes = ScopeStack::new();
        scopes.push(builtin_package());
        Ok(StateData {
            rules: default_token_rules(),
            scopes,
            mode: Mode::Text,
            ext: (),
        })
    }

    /// The two-class event contract's loud arm ([`Event`]): the preset's
    /// [`ExitMathContext`](Event::ExitMathContext) is **context-dependent** — it
    /// is lowered by the driver
    /// ([`ParseDriver::resolve_state_event`](crate::core::ParseDriver::resolve_state_event)) (via
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
        // The loop body always returns on its first iteration today, because `Event`
        // has a single variant and that variant is refused. `Event` is
        // `#[non_exhaustive]`: a context-free variant added later is handled by
        // writing into `new` and moving on to the next event, so the loop over all
        // events is the intended shape, not an accident of the current variant list.
        #[allow(clippy::never_loop)]
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

    /// Scans the scope stack for a specials trigger at `pos`.
    ///
    /// Every provider on the stack is consulted innermost-first and the longest
    /// match wins ([`ScopeStack::scan_specials`]).
    fn scan_specials(
        state: &ParsingState<Self>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
        state.scopes().scan_specials(state, content, pos)
    }

    /// The trigger-character union over the state's providers
    /// ([`ScopeStack::specials_trigger_chars`]), cached per frozen state.
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        data.scopes.specials_trigger_chars()
    }

    /// The preset carries no per-node ext data (`NodeExt = ()`): the mint is the
    /// infallible `Ok(())` one-liner.
    fn make_node_ext(
        _kind: &crate::node::NodeKind<Self>,
        _span: &crate::source::SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), crate::node::NodeBuildError> {
        Ok(())
    }
}

/// [`Latexlike`] opts into its own language family: its vocabularies are the
/// preset enums (which implement the role traits), and every [`LatexlikeLang`]
/// behavior default is exactly the preset behavior — except the
/// parse-initialization checks, wired here unconditionally (the preset
/// vocabularies are enumerable, so the check function's
/// [`ClosedVocabulary`] bounds hold).
impl LatexlikeLang for Latexlike {
    fn check_parse_start(
        source: &Arc<crate::source::Source<Option<String>>>,
        seed: &Arc<ParsingState<Self>>,
        diagnostics: &mut crate::error::Diagnostics<Option<String>>,
    ) {
        crate::scopes::check_provider_commands_shadowed_by_escape(seed, source, diagnostics);
    }
}

/// The preset's canonical [`TokenRules`]: `\` + letters commands (single non-letter
/// characters form single-character commands like `\&` by the tokenizer's standard
/// rule), `{…}` content groups, the four math delimiter pairs (`$…$`, `$$…$$`,
/// `\(…\)`, `\[…\]` — all class [`GroupType::Math`]; `$` vs. `$$` at a close position
/// is disambiguated by the tokenizer's expected-close rule), `%` comments, standard
/// whitespace with multi-newline paragraph breaks, and specials enabled (recognition
/// itself comes from the scope stack's providers).
///
/// `[`/`]` are deliberately **not** group delimiters: in LaTeX they are plain
/// characters outside optional-argument positions (`a [b] c` is plain text), and the
/// optional-argument parser recognizes them through a temporary group rule
/// ([`GroupRules::temporary`]) exactly where an optional argument may sit.
///
/// Whitespace is the **ASCII** set (space, tab, `\n`, `\r`, vertical tab, form feed) —
/// deliberately *not* Unicode-aware (unlike pylatexenc's `str.isspace()`). A Unicode
/// space (NBSP U+00A0, U+2028, …) is ordinary content here, so e.g. an NBSP after
/// `\emph` becomes a content char rather than being consumed as post-macro space
/// (a deliberate choice, for determinism and a fixed char-set model).
///
/// Each math rule declares its [`MathGroupForm`] as class payload
/// ([`GroupType::Math`]): `$…$`/`\(…\)` are [`Inline`](MathGroupForm::Inline),
/// `$$…$$`/`\[…\]` are [`Display`](MathGroupForm::Display) — read back from parsed
/// nodes via [`NodeRef::math_form`](crate::core::node::NodeRef::math_form), with no
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
        whitespace: WhitespaceRules { enabled: true, chars: " \t\n\r\u{000B}\u{000C}".into() },
        paragraphs: ParagraphRules { enabled: true },
        groups: GroupRules {
            enabled: true,
            rules: groups,
            temporary: Vec::new(),
            expecting_close: None,
        },
        commands: CommandRules {
            enabled: true,
            rules: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            })],
        },
        comments: CommentRules {
            enabled: true,
            rules: vec![Arc::new(CommentRule { start: "%".into() })],
        },
        specials: SpecialsRules { enabled: true },
        forbidden_chars: ForbiddenCharsRules { chars: "".into() },
    }
}

/// The seed package `"_builtin"`: exactly what any latexlike parse must have
/// preloaded — the `\begin`/`\end` environment dispatch pair, nothing else.
///
/// The [`Macro`](CallableType::Macro) entries `begin` ([`BeginSpec`] — the
/// environment composition) and `end` ([`EndSpec`] — orphan-`\end` diagnostics) are
/// deliberately ordinary definitions — data in the scope stack,
/// not driver code, so they are shadowable and unloadable like anything else. The
/// internal-flavored name marks the package as parsing substrate rather than
/// definitions content: typography specials (`~`, the `--`/`---`/quote ligatures)
/// are *definitions* and belong to the opt-in [`minidefs`] package — a seed-only
/// parse emits those triggers as plain characters.
///
/// pylatexenc's default context also ships a `\n\n` paragraph-break special; the
/// preset deliberately omits it — a multi-newline break is a whitespace chars node
/// here (via the paragraphs gate, [`ParagraphRules::enabled`]),
/// not a specials node.
///
/// Seeded onto the stack by [`Latexlike::initial_state_data`]; drop it with an
/// [`Unload`](crate::core::specs::ScopeOp::Unload) op naming `"_builtin"` (which removes
/// `\begin`/`\end`), or shadow single entries by pushing a provider above it.
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]): a family member
/// seeds the same dispatch pair under its own vocabulary (the bound is
/// [`BeginSpec`]'s — the composition marks environment body slots through the
/// language's [`BodySlotExt`]).
///
/// The two names are this function's own choice, not the machinery's: the opening
/// command is named by its registration, the terminator by
/// [`BeginSpec::new`]'s argument, and a package spelling the pair differently is an
/// ordinary package ([`BeginSpec`]).
///
/// Built **shared** ([`Package::new_shared`]) with its `\begin` spec stamped, so a
/// parse's builtin definitions serialize by identity; every call builds a fresh
/// package (a fresh `Arc`) — a reading environment that should resolve a serialized
/// `_builtin` to the very package its own parses use holds that package's `Arc`
/// ([`KnownProviders::insert`](crate::serialize::KnownProviders::insert)), or lets
/// the preset's recipe build one
/// ([`serialize::register_package_recipes`]).
pub fn builtin_package<LLL: LatexlikeLang>() -> Arc<Package<LLL>>
where
    crate::node::SlotExt<LLL>: BodySlotExt,
{
    // One binding for the terminator's name: the body parsers stop on it (through
    // `BeginSpec`) and the orphan diagnoser is resolved under it (through the
    // registration) — two uses that must agree.
    let end_command_name = "end";
    Package::new_shared("_builtin", |package| {
        let macro_type = LLL::CallableTypeId::macro_callable();
        let mut begin = BeginSpec::<LLL>::new(end_command_name);
        if let Some(provenance) = package.provenance_for(macro_type, "begin") {
            begin = begin.with_provenance(provenance);
        }
        package.insert(macro_type, "begin", begin);
        // `EndSpec` is stateless: serialized in its self-contained form, no stamp.
        package.insert(macro_type, end_command_name, EndSpec::<LLL>::new());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{macro_package, parse_shapes, root_shapes, strict, tolerant};
    use crate::engine::Language;
    use crate::error::Severity;
    use super::check_latexlike_tree_invariants;
    use crate::scopes::ScopeOp;
    use crate::state::ParsingStateDelta;

    // --- seed & default rules ---------------------------------------------------------

    #[test]
    fn the_seed_state_has_the_canonical_defaults() {
        let language = strict();
        let seed = language.initial_state();
        assert_eq!(seed.mode(), Mode::Text);
        assert_eq!(seed.scopes().provider_names().collect::<Vec<_>>(), ["_builtin"]);
        assert_eq!(seed.rules(), &default_token_rules());
    }

    #[test]
    fn the_seed_is_finalize_coherent() {
        // The initial_state_data() contract: deriving with an empty delta must be
        // data-equivalent to the seed itself (pins the coherence obligation).
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state");
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
        // a Specials-formed callable named by the actual whitespace run
        // (name-as-written), stamped with the canonical ParagraphBreakSpec.
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict)
                .with_paragraph_break_style(ParagraphBreakStyle::Specials),
            ParsingState::lang_initial().expect("seed state"),
        );
        let result = language.parse("a\n\nb").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(root_shapes(&result), ["chars(a)", "Specials(\n\n)", "chars(b)"]);
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n\n"));
        assert_eq!(break_node.span().range(), 1..3);

        // Name-as-written: the actual run is the recorded name (canonical-"\n\n"
        // superseded); the span covers the same run. Identification is by spec
        // identity — the canonical ParagraphBreakSpec, recognized by downcast.
        let result = language.parse("a\n \t\nb").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n \t\n"));
        assert_eq!(break_node.span().content(), "\n \t\n");
        let spec = break_node.spec().expect("a callable node");
        assert!((&**spec as &dyn core::any::Any)
            .downcast_ref::<ParagraphBreakSpec>()
            .is_some());
    }

    // --- math modes -------------------------------------------------------------------

    #[test]
    fn math_group_interiors_parse_in_math_mode() {
        let result = strict().parse("a $x+y$ b").unwrap();
        check_latexlike_tree_invariants(&result.tree);
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
        check_latexlike_tree_invariants(&result.tree);
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
        check_latexlike_tree_invariants(&result.tree);
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
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(root_shapes(&result), ["Macro(alpha)", "chars(x)"]);
    }

    #[test]
    fn a_childless_macro_ends_past_its_own_post_space() {
        // `ParseContext::stage_invocation`'s childless rule: with no argument to end
        // at, the node ends where the reader stands, which is past the trigger's own
        // syntactic post-space. The two spaces after `\alpha` therefore belong to the
        // macro node, and the sibling chars node starts at the `x`.
        let language = with_alpha(crate::error::Recovery::Strict, false);
        let result = language.parse(r"\alpha  x").unwrap();
        check_latexlike_tree_invariants(&result.tree);

        let alpha = result.tree.root().child(0).unwrap();
        assert_eq!(alpha.macro_name(), Some("alpha"));
        assert_eq!(alpha.span().range(), 0..8);
        // The post-space is recorded on the node, which is why it is inside the span.
        assert_eq!(alpha.post_space(), Some("  "));

        let rest = result.tree.root().child(1).unwrap();
        assert_eq!(rest.chars(), Some("x"));
        assert_eq!(rest.span().range(), 8..9);
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
        assert!(message.contains("searched providers: alphapkg, _builtin"), "{message}");
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
            err.to_string().contains("searched providers: _builtin"),
            "strict error lost the searched-providers detail: {err}"
        );

        // The tolerant diagnostic is Error-severity and spans the unresolved command
        // token exactly (guards against a zero-width or post-space-shifted span).
        let result = tolerant().parse(r"a \foo b").unwrap();
        let diag = result.diagnostics.iter().next().unwrap();
        assert_eq!(diag.severity(), Severity::Error);
        assert_eq!(diag.span().content(), r"\foo ");
        assert!(
            diag.message().contains("searched providers: _builtin"),
            "{}",
            diag.message()
        );
    }

    #[test]
    fn did_you_mean_reports_escape_char_registrations() {
        // The A1 registration trap: a name registered WITH its escape character
        // (`insert(…, "\\greet", …)`) can never resolve — command tokens arrive
        // without it. The miss detail calls the shadowed definition out.
        let mut package = Package::new("mydefs");
        package.insert(
            CallableType::Macro,
            r"\greet",
            Arc::new(super::MacroSpec::default()),
        );
        let language = test_support::with_package(crate::error::Recovery::Tolerant, package);
        let result = language.parse(r"\greet x").unwrap();
        // Both defenses fire: the parse-init warning (every mydefs definition is
        // escape-shadowed) and the per-miss did-you-mean detail.
        assert_eq!(result.diagnostics.len(), 2);
        let message = result
            .diagnostics
            .with_identifier("core.specs.unresolvable-command")
            .next()
            .unwrap()
            .message();
        assert!(message.contains("searched providers: mydefs, _builtin"), "{message}");
        assert!(message.contains("provider ‘mydefs’ defines ‘\\greet’"), "{message}");
        assert!(message.contains("without the escape character"), "{message}");
    }

    #[test]
    fn parse_init_warns_when_all_of_a_providers_commands_are_escape_shadowed() {
        use crate::scopes::ProviderCommandsShadowedByEscape;

        // Every Macro definition of `mydefs` begins with the escape character: the
        // A1 registration trap in bulk. The warning fires at parse initialization —
        // on any input, even one that never invokes the definitions.
        let mut package = Package::new("mydefs");
        package.insert(CallableType::Macro, r"\greet", Arc::new(super::MacroSpec::default()));
        package.insert(CallableType::Macro, r"\bye", Arc::new(super::MacroSpec::default()));
        let language = test_support::with_package(crate::error::Recovery::Strict, package);
        let result = language.parse("plain text").unwrap();
        assert_eq!(result.diagnostics.len(), 1);
        let diag = result.diagnostics.iter().next().unwrap();
        assert_eq!(diag.severity(), Severity::Warning);
        assert_eq!(diag.identifier(), "core.specs.provider-commands-shadowed-by-escape");
        let condition =
            diag.data().downcast_ref::<ProviderCommandsShadowedByEscape>().unwrap();
        assert_eq!(condition.provider, "mydefs");
        assert_eq!(condition.callable_type, "Macro");
        assert_eq!(condition.count, 2);
        assert_eq!(condition.example, r"\bye"); // sorted: deterministic example
        assert_eq!(condition.escape_chars, "\\");
        assert!(diag.message().contains("without the escape character"), "{}", diag.message());

        // A mixed table means the author knows the convention (the lone
        // escape-prefixed entry may be intended): no warning.
        let mut package = Package::new("mydefs");
        package.insert(CallableType::Macro, r"\greet", Arc::new(super::MacroSpec::default()));
        package.insert(CallableType::Macro, "hello", Arc::new(super::MacroSpec::default()));
        let language = test_support::with_package(crate::error::Recovery::Strict, package);
        let result = language.parse("plain text").unwrap();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn parse_init_warning_fires_regardless_of_fallback_providers() {
        use crate::scopes::{FallbackProvider, ScopeOp};

        // A fallback provider makes every resolution SUCCEED, so the did-you-mean
        // miss detail never runs — the init-time check is the defense that still
        // fires (and the fallback itself, unable to enumerate, is skipped).
        let mut package = Package::new("mydefs");
        package.insert(CallableType::Macro, r"\greet", Arc::new(super::MacroSpec::default()));
        let mut fallback = FallbackProvider::new("anymacro");
        fallback.set(CallableType::Macro, Arc::new(super::MacroSpec::default()));
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(vec![
                Arc::new(fallback),
                builtin_package(),
                Arc::new(package),
            ])))
            .unwrap();
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            seed,
        );
        let result = language.parse(r"\greet x").unwrap(); // resolves via the fallback
        assert_eq!(result.diagnostics.len(), 1);
        let diag = result.diagnostics.iter().next().unwrap();
        assert_eq!(diag.identifier(), "core.specs.provider-commands-shadowed-by-escape");
        assert_eq!(diag.severity(), Severity::Warning);
    }

    #[test]
    fn did_you_mean_suggests_near_miss_names() {
        let mut package = Package::new("mydefs");
        package.insert(
            CallableType::Macro,
            "greet",
            Arc::new(super::MacroSpec::default()),
        );
        let language = test_support::with_package(crate::error::Recovery::Tolerant, package);
        let result = language.parse(r"\gret x").unwrap();
        assert_eq!(result.diagnostics.len(), 1);
        let message = result.diagnostics.iter().next().unwrap().message();
        assert!(
            message.contains("did you mean ‘greet’ (provider ‘mydefs’)?"),
            "{message}"
        );

        // A far-off name gets no suggestion — the searched-providers detail alone.
        let result = language.parse(r"\xyzzy x").unwrap();
        let message = result.diagnostics.iter().next().unwrap().message();
        assert!(message.contains("searched providers"), "{message}");
        assert!(!message.contains("did you mean"), "{message}");
    }

    #[test]
    fn the_seed_ships_no_specials_definitions() {
        // The typography specials are definitions content, not parsing substrate
        // ([§dd-dr:base-package] amendment): they live in minidefs' `"minilatex"`
        // package now, so a seed-only parse emits them as plain characters — and the
        // alignment `&` special is gone from the shipped definitions entirely (it is
        // not in minilatex either; see the minidefs tests for the moved coverage).
        assert_eq!(
            parse_shapes("a~b & c---d ``q''"),
            ["chars(a~b & c---d ``q'')"]
        );
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
        check_latexlike_tree_invariants(&result.tree);
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
        let mut wrap_groups = default_token_rules::<Latexlike>().groups.rules;
        wrap_groups.push(Arc::clone(&custom_group));
        let wrap_argument = Arc::new(
            ArgumentSpec::new_unnamed(GroupArgumentParser::new(GroupType::Content))
                .with_state_delta(ParsingStateDelta::new().rules(
                    crate::state::TokenRulesOverrides {
                        groups: crate::state::GroupOverrides {
                            rules: Some(wrap_groups),
                            ..crate::state::GroupOverrides::default()
                        },
                        forbidden_chars: crate::state::ForbiddenCharsOverrides {
                            chars: Some("!".into()),
                        },
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
        check_latexlike_tree_invariants(&result.tree);
        let wrap = result.tree.root().child(0).unwrap();
        let math = wrap.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert!(math.is_math_group());

        // Math entry merged the derived chars into the *embedder's* forbidden set.
        let in_math = math.child(0).unwrap();
        assert_eq!(in_math.chars(), Some("a"));
        let math_forbidden = in_math.parsing_state().rules().forbidden_chars();
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
        assert_eq!(restored_state.rules().forbidden_chars(), "!");

        // After the argument, the math context resumes untouched.
        let after = math.child(2).unwrap();
        assert_eq!(after.chars(), Some("b"));
        assert_eq!(after.parsing_state().mode(), Mode::Math);
    }

    #[test]
    fn a_bare_expression_invocation_chain_is_counted_by_the_descent_guard() {
        // Guard coverage for the expression-position dispatch (the F1 site): a
        // macro whose embellishment expression is another bare macro invocation —
        // single-token callables nested in argument position, no braces anywhere —
        // recurses through `dispatch_expression_invocation` alone. (A *mandatory*
        // argument cannot form this chain: a content-requiring callable is
        // refused as a bare expression — `ExpressionCallableRequiresContent`.)
        // Since the expression dispatch routes through `parse_construct`, a
        // configured depth limit counts the chain and refuses it instead of
        // letting it recurse unbounded.
        use crate::constructs::DescentLimitExceeded;
        use crate::engine::StdDescentGuardInit;
        use crate::error::DiagnosticInfo as _;

        let mut package: Package<Latexlike> = Package::new("chain");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(argument_specs(["e{^}"]).unwrap())),
        );
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        )
        .with_descent_guard_init(StdDescentGuardInit::depth_limit(20));

        // A short chain fits comfortably under the limit…
        assert!(language.parse(r"\m^\m^\m^x").is_ok());
        // …a 40-invocation chain crosses it and is refused at the limit.
        let mut chain = String::new();
        for _ in 0..40 {
            chain.push_str(r"\m^");
        }
        chain.push('x');
        let err = language.parse(chain).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
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

        let mut groups = default_token_rules::<Latexlike>().groups.rules;
        groups.push(Arc::clone(&custom_group));
        let seed = ParsingState::lang_initial_with_packages([package]).expect("seed state")
            .derived(&ParsingStateDelta::new().rules(crate::state::TokenRulesOverrides {
                groups: crate::state::GroupOverrides {
                    rules: Some(groups),
                    ..crate::state::GroupOverrides::default()
                },
                ..crate::state::TokenRulesOverrides::default()
            }))
            .unwrap();
        let language =
            Language::new(LatexlikeDriver::new(crate::error::Recovery::Strict), seed)
                // Explicit guard: the input nests enough construct levels to trip
                // the unconfigured default's half-budget warning in debug builds.
                .with_descent_guard_init(crate::engine::StdDescentGuardInit::depth_limit(64));

        let result = language.parse(r"\wrap«$$a\text{y}b$$»").unwrap();
        check_latexlike_tree_invariants(&result.tree);
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
        // The restored context is the `«»` interior — but its in-flight `»`
        // expectation is a transient gate, EXCLUDED from the restore (user
        // ruling amendment, 2026-08-04): the `\text` argument's state (recorded
        // on its group node) does NOT expect `»`.
        let argument_group = text.argument_nodes(0).unwrap().get(0).unwrap();
        let restored_state = argument_group.parsing_state();
        assert_eq!(restored_state.mode(), Mode::Text);
        assert_ne!(
            restored_state
                .rules()
                .expecting_group_close()
                .map(|rule| &*rule.close),
            Some("»")
        );
        // The argument group closes on its OWN `}` (the descent invariant
        // installs the entered rule's close), with the content in the restored
        // text context.
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
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state");
        let error =
            seed.derived(&ParsingStateDelta::new().event(Event::ExitMathContext)).unwrap_err();
        let finalize_error = error.finalize_error.as_ref().unwrap();
        assert!(finalize_error.message().contains("ExitMathContext"));
    }

    /// A foreign Lang adopting the preset vocabularies: the shared fixture of the
    /// family-membership tests below (zero role code — the `()` exts, the preset
    /// enums, `LatexlikeDriver<Flavored>`, and the family's payload enum over its
    /// own environment record).
    #[derive(Debug, Clone, Copy)]
    struct Flavored;
    impl Lang for Flavored {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = GroupType;
        type CallableTypeId = CallableType;
        type ModeId = Mode;
        type StateExt = ();
        type Event = Event;
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = InvocationSyntaxData<StdEnvironmentSyntax<Flavored>>;
        type Driver = LatexlikeDriver<Flavored>;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(StateData {
                rules: default_token_rules(),
                scopes: ScopeStack::new(),
                mode: Mode::Text,
                ext: (),
            })
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }
    impl super::LatexlikeLang for Flavored {}

    #[test]
    fn the_generic_driver_serves_a_foreign_family_member() {
        // A foreign Lang adopting the preset vocabularies joins the family with
        // zero role code and reuses LatexlikeDriver<LLL>, default_token_rules,
        // the pillars, and the NodeRef sugar (the generic StdCallableSpec carries
        // the \text recipe here).
        use crate::spec::StdCallableSpec;

        let text_spec: StdCallableSpec<Flavored> = StdCallableSpec::new([
            ArgumentSpec::new_unnamed(GroupArgumentParser::new(GroupType::Content))
                .with_state_delta(ParsingStateDelta::new().event(Event::ExitMathContext)),
        ]);
        let mut package: Package<Flavored> = Package::new("defs");
        package.insert(CallableType::Macro, "text", Arc::new(text_spec));

        let language: Language<Flavored> = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let result = language.parse(r"a $x\text{ b }y$ c").unwrap();
        check_latexlike_tree_invariants(&result.tree);

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
    fn a_foreign_family_member_parses_environments_and_verbatim() {
        // The environments machinery over `LLL`: `BeginSpec`/`EndSpec` registered
        // for the foreign member dispatch the composition; the payload records
        // begin/end facts in the member's own record type
        // (`StdEnvironmentSyntax<Flavored>`); a verbatim behavior reads the body
        // raw and still reports standard end facts for its terminator. The
        // body slot resolves through the `()` slot ext (`is_body()` is true —
        // slot 0 degenerates to the body).
        let mut package: Package<Flavored> = Package::new("defs");
        package.insert(
            CallableType::Macro,
            "begin",
            Arc::new(BeginSpec::<Flavored>::new("end")),
        );
        package.insert(
            CallableType::Macro,
            "end",
            Arc::new(EndSpec::<Flavored>::new()),
        );
        package.insert(
            CallableType::Environment,
            "itemize",
            EnvironmentSpec::<Flavored>::new(vec![]),
        );
        package.insert(
            CallableType::Environment,
            "verbatim",
            EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::<Flavored>::default())),
        );
        let language: Language<Flavored> = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );

        // The tokenized composition: begin/end facts per side, in the foreign
        // member's record.
        let content = "\\begin {itemize} a \\end{itemize}";
        let result = language.parse(content).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("itemize"));
        let body: Vec<_> = env.body().expect("the body slot").iter().collect();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].chars(), Some(" a "));
        let InvocationSyntaxData::Environment(syntax) = env.invocation_syntax().unwrap()
        else {
            panic!("expected the Environment arm");
        };
        let source = crate::source::Source::new(content);
        assert_eq!(syntax.begin.escape_char, '\\');
        assert_eq!(syntax.begin.post_space.resolve(&source), " ");
        assert_eq!(syntax.write_begin("itemize", &source), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", &source), "\\end{itemize}");

        // The verbatim takeover body: raw content (comment/escape chars inert),
        // standard end facts reported by the body parser for the terminator it was
        // given piecewise.
        let content = "\\begin{verbatim}\na % b \\x{\n\\end{verbatim}";
        let result = language.parse(content).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("verbatim"));
        let body: Vec<_> = env.body().expect("the body slot").iter().collect();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].chars(), Some("a % b \\x{\n"));
        let InvocationSyntaxData::Environment(syntax) = env.invocation_syntax().unwrap()
        else {
            panic!("expected the Environment arm");
        };
        let end = syntax.end.as_ref().expect("the terminator was consumed");
        assert!(!end.command_word.is_owned());
        assert_eq!(
            syntax.write_end("verbatim", &crate::source::Source::new(content)),
            "\\end{verbatim}"
        );
    }

    #[test]
    fn preset_declarative_specs_serve_a_foreign_family_member() {
        // MacroSpec<LLL> + the argument-code factory over LLL: the declarative
        // preset specs register under the foreign member directly (`LLL` inferred
        // from the package); the `v` code resolves its group class through the
        // family's verbatim_group() role constructor.
        let mut package: Package<Flavored> = Package::new("defs");
        package.insert(
            CallableType::Macro,
            "emph",
            Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
        );
        package.insert(
            CallableType::Macro,
            "verb",
            Arc::new(MacroSpec::new(argument_specs(["v"]).unwrap())),
        );
        let language: Language<Flavored> = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );

        let result = language.parse("\\emph{x} \\verb|a%\\y{|!").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        assert!(emph.arguments().unwrap().get(0).unwrap().is_provided());
        let verb = result.tree.root().child(2).unwrap();
        assert_eq!(verb.macro_name(), Some("verb"));
        // The verbatim argument read raw (comment/escape chars inert) into a
        // verbatim-class group of the family.
        let raw = verb.child(0).unwrap();
        assert_eq!(raw.group_type(), Some(GroupType::Verbatim));
        assert_eq!(raw.group_delimiters(), Some(("|", "|")));
    }

    // A foreign family member opts into serialization with the empty impl: the
    // preset's vocabulary and payload types carry the value conversions for every
    // language, and the preset's spec types serialize under `LLL`.
    impl crate::serialize::SerializableLang for Flavored {}

    #[test]
    fn a_foreign_family_member_serializes_with_the_presets_conversions() {
        use crate::serialize::tree_support::{ignore_annotations, round_trip_tree};
        use crate::serialize::{KnownProviders, SerdeSession};

        let defs = Package::<Flavored>::new_shared("defs", |package| {
            package.define_macro("emph", ["m"]).unwrap();
            package.define_environment("A", ["o"]).unwrap();
        });
        let language: Language<Flavored> = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([builtin_package(), Arc::clone(&defs)])
                .expect("seed state"),
        );
        let result = language.parse("\\emph{x} \\begin{A}[o] $m$ \\end{A}").unwrap();
        let providers = language.initial_state().scopes().providers().to_vec();
        let factory = move || {
            let mut session = SerdeSession::<Flavored>::new();
            let mut known = KnownProviders::<Flavored>::new();
            for provider in &providers {
                known.insert(Arc::clone(provider));
            }
            session.set_user_data(known);
            super::serialize::register(&mut session).unwrap();
            session
        };
        let back = round_trip_tree(factory, &result.tree, ignore_annotations);
        let emph = back.root().child(0).unwrap();
        assert!(Arc::ptr_eq(emph.spec().unwrap(), defs.get(CallableType::Macro, "emph").unwrap()));
        assert_eq!(back.root().child(2).unwrap().environment_name(), Some("A"));
    }

    #[test]
    fn the_builtin_package_is_unloadable_by_name() {
        let seed = ParsingState::<Latexlike>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::Unload {
                name: "_builtin".into(),
            }))
            .unwrap();
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            seed,
        );
        // With `\begin`/`\end` unloaded, a `\begin` command is unresolvable.
        let err = language.parse(r"\begin{itemize}").unwrap_err();
        assert!(err.to_string().contains("cannot resolve command ‘\\begin’"), "{err}");
    }
}
