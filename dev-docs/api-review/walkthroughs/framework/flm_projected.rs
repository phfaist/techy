//! T5 P3-acceptance sketch: the FLM probe **as it would look under the ruled
//! generalization** (P3 + T3 D/E + P4 + T1/T2 E4 + T4 — none applied to code yet, so
//! this file does NOT compile today; the binding compile check happens at Phase 3
//! acceptance, per PLAN).
//!
//! Projection sources, per construct:
//!   [G]  = [§dd-dr:latexlike-generalization] (role traits, LatexlikeLang, pillars)
//!   [MF] = [§dd-dr:math-group-form]
//!   [DP] = [§dd-dr:preset-driver-pillars] + T3_RULINGS D (LatexlikeDriver<LLL>)
//!   [EM] = [§dd-dr:ext-minting] (make_node_ext required; tier-2 removed)
//!   [SR] = [§dd-dr:slot-roles] (BodySlotExt; preset claims SlotExt)
//!   [AN] = [§dd-dr:node-annotations] (NodeTree<L, A>)
//!   [ES] = [§dd-dr:enclosing-state-stack] + T3 D amendment (exit_math_context_delta)
//!   [LI] = [§dd-dr:language-init] + T4 collapse
//!   [IW] = [§dd-dr:input-wiring] (driver resolver field)
//!   [T5?] = NOT ruled — proposed in T5_BRIEF.md (points cited inline)
//!
//! Old probe for contrast: walkthroughs/framework probes flm_lang.rs +
//! flm_reuse.rs.negative-probe (re-verified failing at 4c324c7).

use std::sync::Arc;

// Post-P1 topology paths (illustrative):
use techy::core::{Lang, LatexlikeLangBoundsEtc /* placeholder */, Language, ParsingState};
use techy::latexlike::{
    self, builtin_package, minidefs, CallableType, LatexlikeDriver, MacroSpec, Mode,
    MathGroupForm,
};

// ---------------------------------------------------------------------------------
// 1. Vocabularies: extend ONE (GroupType), adopt the others verbatim.
// ---------------------------------------------------------------------------------

/// FLM extends the group taxonomy with a fenced-block class; implementing the role
/// trait guarantees the preset-required classes exist while the enum stays FLM's own.
/// [G] role traits are method-based, implemented by the vocabulary type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FlmGroupType {
    Content,
    Math(MathGroupForm), // [MF] the payload travels through the role trait
    Verbatim,
    FencedBlock, // FLM's own — invisible to the preset
}

impl latexlike::LatexlikeGroupType for FlmGroupType {
    fn content_group() -> Self { FlmGroupType::Content }
    fn math_group(form: MathGroupForm) -> Self { FlmGroupType::Math(form) }
    fn verbatim_group() -> Self { FlmGroupType::Verbatim }
    fn math_form(self) -> Option<MathGroupForm> {
        match self { FlmGroupType::Math(f) => Some(f), _ => None }
    }
    // is_math() keeps its default (math_form().is_some()).
}

// CallableType and Mode are adopted verbatim: techy implements the role traits for
// its own enums, so these two cost zero code. [G]
// (Mode role trait post-T3 E: only math_mode() + is_math() — no text constructor.)

/// [T5?] — THE GAP (brief point C1): the ruled role-trait set has NO Event member,
/// but the LLL-generic `\text`-style factory ([§dd-dr:argument-factory-additions])
/// must mint the exit-math event *in FLM's own Event vocabulary*, and
/// `LatexlikeDriver<Flm>::resolve_state_event` must recognize it. Proposed:
///
///   trait LatexlikeEvent: Clone + ... {
///       fn exit_math_context() -> Self;          // constructor role
///       fn is_exit_math_context(&self) -> bool;  // recognizer role
///   }
///
/// with the usual coherence contract. Without it, an FLM using its own Event type
/// loses the preset text-restore machinery (a fresh cliff of exactly the P3 kind).
#[derive(Debug, Clone, PartialEq)]
enum FlmEvent {
    ExitMathContext,
    BeginTheorem, // FLM's own events coexist
}

// impl latexlike::LatexlikeEvent for FlmEvent { ... }   // [T5?]

// ---------------------------------------------------------------------------------
// 2. Ext bundle: post-P4 shape — three members, no tier-2, no blanket Default. [EM]
// ---------------------------------------------------------------------------------

/// Tier-1 ext minted once at staging by `make_node_ext` (no `Default` bound).
#[derive(Clone, Debug)]
struct FlmNodeExt {
    char_count: u32, // the old probe's finalize_node payload, now minted-not-populated
}

/// The preset claims the SlotExt member for body marking [SR]: FLM's slot ext must
/// implement `BodySlotExt` or the preset environment machinery loses `body()`.
#[derive(Clone, Debug)]
struct FlmSlotExt {
    body: bool,
}

impl techy::core::node::BodySlotExt for FlmSlotExt {
    fn is_body(&self) -> bool { self.body }
    fn make_body() -> Self { FlmSlotExt { body: true } }
}

struct FlmExts;
impl techy::core::NodeExtTypes for FlmExts {
    type NodeExt = FlmNodeExt;
    type ArgumentExt = (); // () keeps the std argument parsers usable (Default bound-where-used)
    type SlotExt = FlmSlotExt;
    // Tier-2 members are GONE. [EM]
}

// ---------------------------------------------------------------------------------
// 3. The Lang impl: the irreducible ~30-line residue, all pillar delegations. [G]
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Flm;

impl Lang for Flm {
    type GroupTypeId = FlmGroupType;
    type CallableTypeId = CallableType; // adopted
    type ModeId = Mode;                 // adopted
    type StateExt = ();
    type Event = FlmEvent;
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = FlmExts;
    type Driver = FlmDriver; // custom (one extra hook) — LatexlikeDriver<Flm> works for wholesale

    /// One line instead of the old probe's 13-field TokenRules literal:
    /// pillar `latexlike::initial_state_data::<Flm>()` (canonical rules + builtin
    /// package via the role traits; math delimiters via the LatexlikeLang data
    /// methods). [G] — was the flm_reuse negative probe's first two compile errors.
    fn initial_state_data() -> techy::core::StateData<Flm> {
        latexlike::initial_state_data::<Flm>()
    }

    /// T1/T2 E4: fallible now; FLM has no context-free events → default Ok. [ES]
    // fn finalize_transition(...) -> Result<(), DeriveError<Flm>> { Ok(()) }

    /// The standard two-line scope-fold delegation (T3 B3 recipe — defaults stay
    /// recognize-nothing).
    fn scan_specials<'s>(
        state: &ParsingState<Flm>, content: &'s str, pos: usize,
    ) -> techy::core::TokenResult<'s, Flm, Option<techy::core::SpecialsMatch<'s, Flm>>> {
        state.scopes().scan_specials(state, content, pos)
    }
    fn specials_trigger_chars(data: &techy::core::StateData<Flm>) -> techy::core::TriggerChars {
        data.scopes.specials_trigger_chars()
    }

    /// REQUIRED post-P4 [EM] — the old probe's finalize_node body becomes a
    /// value-returning mint (StagedChildren gives subtree-deep read access).
    fn make_node_ext(
        kind: &techy::core::node::NodeKind<Flm>,
        _span: &techy::source::SourceSpan<Option<String>>,
        _state: &Arc<ParsingState<Flm>>,
        _children: techy::core::node::StagedChildren<'_, Flm>,
    ) -> FlmNodeExt {
        let char_count = match kind {
            techy::core::node::NodeKind::Chars { content } => match content {
                techy::source::TextContent::Owned(s) => s.chars().count() as u32,
                techy::source::TextContent::Spanned(s) => s.len() as u32,
            },
            _ => 0,
        };
        FlmNodeExt { char_count }
    }
}

/// Opting into the preset family: no blanket impl, defaults overridable. [G]
impl latexlike::LatexlikeLang for Flm {
    // math-delimiter data + math-interior adjustment keep their defaults.
}

// ---------------------------------------------------------------------------------
// 4. The driver: FLM's documented posture = preset behavior + one custom hook →
//    write-your-own over pillars ([DP]; the canned LatexlikeDriver<Flm> would serve
//    the wholesale case in one line). Clone + Debug only (Copy/Eq dropped, T4).
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FlmDriver {
    recovery: techy::error::Recovery,
    resolver: Option<Arc<dyn techy::source::SourceResolver<Option<String>>>>, // [IW]
}

impl techy::core::ParseDriver<Flm> for FlmDriver {
    fn recovery(&self) -> techy::error::Recovery { self.recovery }

    fn source_resolver(&self) -> Option<&dyn techy::source::SourceResolver<Option<String>>> {
        self.resolver.as_deref() // [IW] T4 B1
    }

    /// Pillar delegation: H's extracted resolver + the macro role. [DP]
    fn resolve_command(
        &self, state: &ParsingState<Flm>, token: &techy::core::Token<'_, Flm>,
    ) -> techy::core::specs::CommandResolution<Flm> {
        techy::core::specs::resolve_command_in_scopes(state, token, CallableType::Macro)
    }

    /// Pillar delegation: the math plug (forbidden set derived from the removed
    /// math-class rules — works unchanged for FlmGroupType via is_math()). [DP][MF]
    fn group_interior_delta(
        &self, base: &ParsingState<Flm>, rule: &Arc<techy::core::GroupRule<Flm>>,
    ) -> Option<techy::core::ParsingStateDelta<Flm>> {
        latexlike::math_group_interior_delta::<Flm>(base, rule)
    }

    /// E4 hook: preset pillar for the exit-math event, FLM's own events beside it.
    /// [ES] — signature sufficiency for post-parse synthesis is brief point E.
    fn resolve_state_event(
        &self, event: &FlmEvent, stack: &techy::core::StateStackView<'_, Flm>,
    ) -> Option<techy::core::ParsingStateDelta<Flm>> {
        match event {
            FlmEvent::ExitMathContext =>
                Some(latexlike::exit_math_context_delta::<Flm>(stack.states() /* [T5?] point E */)),
            FlmEvent::BeginTheorem => None, // handled in finalize_transition? FLM's business
        }
    }

    /// The one hook FLM actually customizes — the reason it is not LatexlikeDriver<Flm>.
    fn refine_diagnostic(&self /* ... */) { /* FLM wording */ }
}

// ---------------------------------------------------------------------------------
// 5. Setup + annotated pipeline: the framework shape the annotations were built for.
// ---------------------------------------------------------------------------------

fn main() {
    // Registration: LLL-generic specs + one-liners (T1/T2 E1; wish-8 named-first).
    let mut package = techy::core::specs::Package::<Flm>::new("flm");
    package.define_macro("ref", "m").unwrap(); // MacroSpec<Flm> under the hood [G]

    // Language init: P2/T1-T2/T3/T4 collapsed surface. [LI]
    let language = Language::new(
        FlmDriver { recovery: techy::error::Recovery::Tolerant, resolver: None },
        ParsingState::lang_initial_with_packages([
            minidefs::minilatex_package::<Flm>(), // generic rider, T1/T2 B
            package,
        ]),
    );

    let result = language.parse(r"See \ref{fig:one} and $x+y$.").unwrap();

    // The old probe's side tables become typed annotation stages. [AN]
    #[derive(Clone, Debug)]
    struct Sem { origin: techy::core::node::NodeId, ref_key: Option<String> }

    let sem: techy::core::node::NodeTree<Flm, Sem> = result.tree.annotate(|node| Sem {
        origin: node.id(), // the origin-by-convention one-liner (P4 point 5)
        ref_key: (node.name() == Some("ref"))
            .then(|| node.argument_content_nodes(0)?.source_text().map(String::from))
            .flatten(),
    });

    // Restage pass over the annotated tree — signatures are T5_BRIEF point A
    // proposals, NOT ruled:
    let _out: techy::core::node::NodeTree<Flm, ()> =
        techy::transform::restage(&sem, &mut |node, cx| {
            if node.annotation().ref_key.as_deref() == Some("fig:one") {
                // targeted rewrite: swap the argument content, keep the wrapper
                let chars = cx.builder().add(/* new Chars node ... */)?;
                let arg = cx.restage_argument_with_content(node, 0, vec![chars])?; // [T5? A4]
                let id = cx.restage_invocation(node, vec![arg], vec![], ())?;      // [T5? A2]
                return Ok(techy::transform::Restage::Emit(vec![id]));
            }
            Ok(techy::transform::Restage::Descend(()))                              // [T5? A7]
        })
        .unwrap();
}
