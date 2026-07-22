//! The `latexlike` preset (S2): the familiar LaTeX behavior, assembled from the
//! generic core.
//!
//! The core has no privileged language concepts (no built-in math mode, `{`/`}`, `%`,
//! or `\`); this module is where the familiar vocabulary returns — as *preset data and
//! preset code* over the same extension surface any language uses:
//!
//! - [`Latexlike`] — the [`Lang`] ZST, with the preset's three closed vocabularies
//!   [`GroupType`], [`CallableType`], and [`Mode`];
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
//!   `\verb`-style delimited verbatim arguments (Phase 7.7);
//! - `NodeRef` accessor sugar for latexlike trees ([`MathStyle`],
//!   [`is_math_group`](crate::node::NodeRef::is_math_group), …) — inherent methods on
//!   `NodeRef<'_, Latexlike>`.
//!
//! ```
//! use techy::engine::Language;
//! use techy::latexlike::{Latexlike, Mode};
//!
//! let language: Language<Latexlike> = Language::default();
//! let result = language.parse("inline $x+y$ math").unwrap();
//! let math = result.tree.root().child(1).unwrap();
//! assert!(math.is_math_group());
//! assert_eq!(math.child(0).unwrap().parsing_state().mode(), Mode::Math);
//! ```
//!
//! **What the preset does not ship yet:** macro and environment *definitions*. The
//! standard spec database (pylatexenc's default-specs port) is a later phase; until
//! then embedders and tests register the specs they need via scope-stack deltas
//! ([`Language::with_seed_delta`](crate::engine::Language::with_seed_delta) +
//! [`ParsingStateDelta::push_provider`](crate::state::ParsingStateDelta::push_provider)),
//! as [`MacroSpec`]/[`EnvironmentSpec`]/[`SpecialsSpec`] entries (or any custom
//! [`CallableSpec`]) — `\verb` and `verbatim` included: the machinery ships here, the
//! definitions with the database.

mod arguments;
mod driver;
mod environments;
mod node_ref;
mod spec;
#[cfg(test)]
mod test_support;

pub use arguments::{argument_specs, argument_specs_from_str, ArgumentCodeError};
pub use driver::{LatexlikeDriver, ParagraphBreakStyle};
pub use environments::{
    BeginSpec, EndSpec, EnvironmentBehavior, EnvironmentInvocation, EnvironmentSpec,
    MalformedBegin, OrphanEnd, UnknownEnvironment, VerbatimBehavior,
};
pub use node_ref::MathStyle;
pub use spec::{MacroSpec, SpecialsSpec};

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::scopes::{Package, ScopeStack};
use crate::spec::CallableSpec;
use crate::state::{ClosedVocabulary, Lang, ParsingState, StateData};
use crate::token::{
    CommandRule, CommentRule, GroupRule, SpecialsMatch, TokenResult, TokenRules,
    TriggerChars, WhitespaceRules,
};

/// The preset's group classes ([`Lang::GroupTypeId`]).
///
/// Classes classify **parse behavior**, not delimiter spellings: several delimiter
/// pairs share one class, and the node's [`GroupData`](crate::node::GroupData) records
/// the delimiters as written. There is deliberately a *single* math class (decided at
/// the 7.5 checkpoint): inline and display math parse identically — same interior
/// [`Mode::Math`], same definition visibility — so inline/display is a delimiter fact,
/// exposed by [`NodeRef::math_style`](crate::node::NodeRef::math_style), not a class.
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
    /// plug).
    Math,
    /// A verbatim region's group (Phase 7.7): the `\verb|…|` shape staged by the `v`
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
/// mode (nor a group class): it changes nothing about how the interior parses — see
/// [`MathStyle`] for the presentation-side accessor.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    /// Ordinary text content — the seed state's mode.
    #[default]
    Text,
    /// Math content (inside `$…$`, `$$…$$`, `\(…\)`, `\[…\]`).
    Math,
}

// The preset's vocabularies are statically listable (Phase 7.8): generic tooling
// enumerates them (e.g. `ScopeStack::iter_symbols` once per `CallableType::ALL` entry).
// The enums are `#[non_exhaustive]`, so adding a variant means extending `ALL` in the
// same change (the `ClosedVocabulary` contract).

impl ClosedVocabulary for GroupType {
    const ALL: &'static [GroupType] =
        &[GroupType::Content, GroupType::Math, GroupType::Verbatim];
}

impl ClosedVocabulary for CallableType {
    const ALL: &'static [CallableType] =
        &[CallableType::Macro, CallableType::Environment, CallableType::Specials];
}

impl ClosedVocabulary for Mode {
    const ALL: &'static [Mode] = &[Mode::Text, Mode::Math];
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
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type Driver = LatexlikeDriver;

    /// The canonical latexlike seed: [`default_token_rules`], a scope stack holding
    /// the [`base_package`] (standard specials), [`Mode::Text`].
    ///
    /// Coherence contract: `finalize_transition` is not customized (nothing to
    /// normalize yet — math groups only set the mode), so the seed is trivially
    /// finalize-coherent; a test pins `initial().derived(&empty) == initial()`
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
}

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
/// (decided for determinism and a fixed char-set model; DESIGN_RATIONALE.md [§dd-dr:latexlike]).
///
/// The four math delimiter pairs come from the shared `MATH_DELIMITERS` table (the
/// single source of truth also read by [`NodeRef::math_style`](crate::node::NodeRef::math_style)).
pub fn default_token_rules() -> TokenRules<Latexlike> {
    fn group(group_type: GroupType, open: &str, close: &str) -> Arc<GroupRule<Latexlike>> {
        Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
    }

    let mut groups = vec![group(GroupType::Content, "{", "}")];
    groups.extend(
        node_ref::MATH_DELIMITERS
            .iter()
            .map(|&(open, close, _style)| group(GroupType::Math, open, close)),
    );

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
/// not a specials node (DESIGN_RATIONALE.md [§dd-dr:latexlike]).
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
        package.insert_specials(trigger, CallableType::Specials, Arc::clone(&spec));
    }
    // Typography ligatures are text-mode only (no math meaning): inside `$…$` they stay
    // plain chars (7.5 review — per-entry mode visibility).
    for trigger in ["``", "''", "--", "---"] {
        package.insert_specials_in_modes(
            trigger,
            CallableType::Specials,
            Arc::clone(&spec),
            Some(vec![Mode::Text]),
        );
    }
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
        let seed = ParsingState::<Latexlike>::initial();
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
            LatexlikeDriver::default().with_paragraph_break_style(ParagraphBreakStyle::Specials),
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
            ["chars(a )", "group(Math $ $)", "chars( b)"]
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
            ["group(Math $ $)", "group(Math $ $)"]
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
        assert_eq!(root_shapes(&result), ["group(Math $$ $$)"]);
        let display = result.tree.root().child(0).unwrap();
        let interior: String = display.children().iter().filter_map(|child| child.chars()).collect();
        assert_eq!(interior, "a$b");
        // Exactly one diagnostic: the forbidden `$`.
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn display_math_delimiters() {
        assert_eq!(parse_shapes("$$ab$$"), ["group(Math $$ $$)"]);
        assert_eq!(parse_shapes(r"\[x\]"), [r"group(Math \[ \])"]);
        assert_eq!(parse_shapes(r"\(x\)"), [r"group(Math \( \))"]);
    }

    // --- scope stack: commands, visibility, specials ----------------------------------

    /// `language` seeded with the zero-argument macro `\alpha` in a package
    /// `"alphapkg"`, optionally math-only (package-level visibility).
    fn with_alpha(language: Language<Latexlike>, math_only: bool) -> Language<Latexlike> {
        let modes = math_only.then(|| vec![Mode::Math]);
        language.with_provider(Arc::new(macro_package("alphapkg", "alpha", modes))).unwrap()
    }

    #[test]
    fn commands_resolve_through_the_scope_stack() {
        let language = with_alpha(strict(), false);
        let result = language.parse(r"\alpha x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(root_shapes(&result), ["Macro(alpha)", "chars(x)"]);
    }

    #[test]
    fn mode_visibility_gates_package_definitions() {
        let language = with_alpha(tolerant(), true);

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
        assert_eq!(math.group_type(), Some(GroupType::Math));
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

    #[test]
    fn the_base_package_is_unloadable_by_name() {
        let language = strict()
            .with_seed_delta(
                ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "base".into() }),
            )
            .unwrap();
        let result = language.parse("a~b --- c").unwrap();
        assert_eq!(root_shapes(&result), ["chars(a~b --- c)"]);
    }
}
