//! [`InputMacroSpec`] / [`input_macro_spec`]: the preset's opt-in `\input`-shaped
//! macro — resolve a referenced source and attach its parsed content to the
//! invocation.
//!
//! **Never preloaded**: the spec is not part of [`builtin_package`](super::builtin_package)
//! — an always-on `\input` under a resolver-less driver would just diagnose every
//! use. Embedders that want it insert it into their own package, under their own
//! macro callable type and any command name — choosing **consciously**, through the
//! two mandatory constructor parameters, whether included state changes persist
//! past the `\input` (`persist_state`) and what slot-ext value the attached slot
//! carries (for the preset, [`BodyMarker::not_body`](super::BodyMarker::not_body)
//! unless the framework wants the attached content findable as the node's *body*):
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::specs::Package;
//! use techy::error::Recovery;
//! use techy::latexlike::{
//!     input_macro_spec, BodyMarker, CallableType, Latexlike, LatexlikeDriver,
//! };
//! use techy::source::MapResolver;
//!
//! let mut resolver = MapResolver::new();
//! resolver.insert("chapter.tex", "included {content}");
//! let mut package: Package<Latexlike> = Package::new("mydefs");
//! package.insert(
//!     CallableType::Macro,
//!     "input",
//!     input_macro_spec(false, BodyMarker::not_body()),
//! );
//!
//! let language = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict).with_source_resolver(resolver),
//!     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
//! );
//! let result = language.parse(r"a \input{chapter.tex} b").unwrap();
//! let input = result.tree.root().child(1).unwrap();
//! // The invocation's own span lives in the includer; the attached content —
//! // parsed out of the resolved source — is retrieved by its slot name.
//! assert_eq!(input.span_content(), r"\input{chapter.tex}");
//! assert_eq!(
//!     input.slot_content_nodes_named("attached").unwrap().source_text().unwrap(),
//!     "included {content}",
//! );
//! ```

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    parse_declared_arguments, ChildStateSpec, ConstructParser, ConstructParserResult,
    GroupArgumentParser, InvalidReferenceReason, InvalidSourceReferenceArgument, Invocation,
    ParseContext, StopSpec,
};
use crate::node::{
    ArgumentExt, BuildId, ChildRegion, ContentNodes, NodeKind,
    ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots, SlotExt, SlotRole,
};
use crate::engine::ParseDriver;
use crate::scopes::SpecProvenance;
use crate::source::SourceSpan;
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::token::TokenEdge;
use crate::state::ParsingStateDelta;

use super::lang::LatexlikeGroupType;
use super::spec::frame_title;
use super::{Latexlike, LatexlikeLang};

/// The preset's opt-in `\input`-shaped macro spec: one mandatory `{…}` argument
/// naming an external source reference; the invocation resolves it through the
/// driver's [`SourceResolver`](crate::source::SourceResolver) and parses the
/// content **at the invocation point, into the same tree** — recorded as an
/// [`Attached`](SlotRole::Attached) slot (named `"attached"`) of the staged
/// callable. Constructed by [`input_macro_spec`], **never preloaded**: inclusion is
/// an explicit embedder choice — an always-on `\input` under a driver with no
/// source resolver would only diagnose every use.
///
/// The node's own span is its invocation in the *includer's* source (`\input{…}`);
/// only the attached slot's children live in the resolved source — a multi-source
/// tree is first-class, and recomposition per source emits the invocation text,
/// not the content.
///
/// # The attached slot's ext is the embedder's
///
/// The slot's [`SlotExt`] value is supplied at construction and cloned into every
/// invocation's slot record — the spec does not decide what the ext means. In
/// particular the preset's `\input` does **not** overload the environment-body
/// marker: the recipe passes [`BodyMarker::not_body`](super::BodyMarker::not_body),
/// so [`NodeRef::body`](crate::node::NodeRef::body) (ext-axis selection) does not
/// select the attached content — retrieval is by slot name,
/// [`slot_content_nodes_named("attached")`](crate::node::NodeRef::slot_content_nodes_named).
/// A framework that *wants* the attached content to be the node's body passes a
/// body-marked ext instead ([`BodySlotExt::make_body`](crate::node::BodySlotExt::make_body)),
/// and `body()` finds it — the ext axis is selected alone, with no hidden
/// role conjunction, precisely so that choice cannot become silently unfindable.
///
/// # The reference argument carries plain text
///
/// The argument's content must be plain characters. The reference is read off the
/// staged argument's own node data — the character payloads of its content nodes,
/// concatenated as read — and that text is what drives resolution. Content that is
/// anything else carries no such text: a protective group (`\input{{chap.tex}}`), a
/// callable, a comment inside the braces raise
/// [`InvalidSourceReferenceArgument`](crate::constructs::InvalidSourceReferenceArgument)
/// at the argument's span, and nothing is resolved or attached. Characters are taken as
/// written, with no trimming: whitespace inside the braces is part of the reference
/// (`\input{ chap.tex }` resolves `" chap.tex "`), so tolerating padding is the
/// resolver's choice.
///
/// What counts is the staged *nodes*, not the source text. An unresolvable command
/// inside the delimiters therefore does not raise the condition: the content run
/// recovers it as characters, and the recovered text becomes the reference the resolver
/// is asked for (`\input{\undefined.tex}` asks for `"\undefined.tex"`, after the
/// unresolvable-command condition). Conversely, a driver configured with
/// [`ParagraphBreakStyle::Specials`](super::ParagraphBreakStyle::Specials) stages a
/// paragraph break as a callable node, so a blank line inside the argument raises the
/// condition, where the default whitespace-characters shape would not.
///
/// Reading the reference from node data needs no assumption about where the tokens came
/// from, so the rule is the same under every language — including one declaring
/// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`, whose
/// argument may be read from several sources.
///
/// # Failure conditions
///
/// Resolution failures are diagnosed at the invocation span through the single
/// raising site ([`ParseContext::attach_source_reference`]):
/// [`NoSourceResolver`](crate::constructs::NoSourceResolver) when the driver has
/// no resolver, [`UnresolvableSourceReference`](crate::constructs::UnresolvableSourceReference)
/// when the resolver fails; the reference-argument condition above is diagnosed at the
/// argument instead, before any resolution is attempted. Tolerant parses record the
/// condition and stage the callable *without* an attached slot; strict parses abort.
///
/// # State handling — `persist_state` decides
///
/// The attached content always parses under the parsing state at the `\input`
/// point (definitions in force there apply inside the included content). What
/// happens to the included content's **own** after-effects — a
/// `\newcommand`-style definition made *inside* the included file — is the
/// mandatory `persist_state` constructor choice:
///
/// - **`persist_state: false` — transparent**: the included run's after-effects
///   end with the file; the rest of the including document is unaffected.
/// - **`persist_state: true` — persisting**: the included run's applied
///   after-effect deltas, merged into one record
///   ([`AttachedSourceOutcome::after_effects`](crate::constructs::AttachedSourceOutcome::after_effects)),
///   are returned as the `\input` invocation's own after-effect through the
///   ordinary sibling channel — the paradigm case is a preamble file whose
///   definitions must hold for the rest of the document. Nested inclusions
///   compose: an inner file's persisted effects join the outer file's record.
///
/// # Variants are custom-spec work
///
/// The form-specific parts stay in the spec: `\input[options]{file}` or
/// `\input*{f1,f2,f3}` variants parse their own argument shapes and reuse the
/// same two helpers (argument text →
/// [`attach_source_reference`](ParseContext::attach_source_reference) → an
/// `Attached` slot) — the brief form below is the template.
pub struct InputMacroSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure: one mandatory `{…}` argument named `"reference"`.
    arguments: Vec<Arc<ArgumentSpec<LLL>>>,
    /// Whether the included run's merged after-effects continue past the `\input`.
    persist_state: bool,
    /// The ext value cloned into every invocation's attached slot.
    attached_slot_ext: SlotExt<LLL>,
    /// Where the spec was defined, when known.
    provenance: Option<SpecProvenance<LLL>>,
}

impl<LLL: LatexlikeLang> InputMacroSpec<LLL> {
    /// Whether the included run's merged after-effects continue past the `\input`
    /// (the `persist_state` choice of [`input_macro_spec`]).
    pub fn persist_state(&self) -> bool {
        self.persist_state
    }

    /// The ext value recorded on every invocation's attached slot (the
    /// `attached_slot_ext` choice of [`input_macro_spec`]).
    pub fn attached_slot_ext(&self) -> &SlotExt<LLL> {
        &self.attached_slot_ext
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out — so that the spec is serialized by identity rather than in
    /// its self-contained form (`persist_state` plus the slot ext). Replaces a
    /// previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> InputMacroSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Where this spec is defined, if it was stamped.
    pub fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }
}

/// Create the preset's opt-in `\input` spec ([`InputMacroSpec`] — never preloaded:
/// inclusion is an explicit embedder choice; the type's documentation carries the
/// full contract).
///
/// # The two mandatory choices
///
/// Both parameters are deliberate embedder decisions with **no defaults**:
///
/// - `persist_state` — whether state changes made inside the included file
///   (after-effect deltas of its constructs) continue past the `\input` into the
///   rest of the including document. See the type's
///   [state-handling section](InputMacroSpec#state-handling--persist_state-decides).
/// - `attached_slot_ext` — the [`SlotExt`] value recorded on the `"attached"`
///   slot (cloned per invocation). The preset recipe passes
///   [`BodyMarker::not_body`](super::BodyMarker::not_body); a body-marked value
///   makes the attached content the node's
///   [`body()`](crate::node::NodeRef::body) — the framework's choice, never the
///   shipped default. See the type's
///   [ext section](InputMacroSpec#the-attached-slots-ext-is-the-embedders).
///
/// # No input caching
///
/// The included file is read **on the spot, at parse time** — deliberately: the
/// parsing state at the `\input` point governs how the content tokenizes, and an
/// `\input`-style construct may feed state back into the including document —
/// with `persist_state: true` this very spec does, which makes the rationale
/// stronger still: a parse-without-attachment cache is unsound for any document
/// whose included files carry definitions. techy therefore neither implements
/// nor recommends input caching; resolvers may freely cache *content* (the
/// [`SourceResolver`](crate::source::SourceResolver) contract), which is the
/// part that costs input/output. A separate-parse-then-splice arrangement (caching
/// parsed trees of included files) is sound only when the inclusion is known
/// state-transparent — `persist_state: false` **and** no out-of-band state
/// coupling — and is an embedder-level optimization, not something techy
/// provides.
pub fn input_macro_spec<LLL>(
    persist_state: bool,
    attached_slot_ext: SlotExt<LLL>,
) -> InputMacroSpec<LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
{
    InputMacroSpec {
        arguments: vec![Arc::new(ArgumentSpec::new(
            Arc::new(GroupArgumentParser::new(LLL::GroupTypeId::content_group())),
            "reference",
        ))],
        persist_state,
        attached_slot_ext,
        provenance: None,
    }
}

// The `SerializableObject`/`DeserializableObject` impls (identity when stamped, the
// two constructor choices otherwise) live in `super::serialize`.

impl<LLL> CallableSpec<LLL> for InputMacroSpec<LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
{
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }

    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, LLL>,
    ) -> Result<
        alloc::boxed::Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>,
        crate::error::ParseError<LLL::SourceOrigin>,
    >
    {
        Ok(alloc::boxed::Box::new(InputInvocationParser {
            invocation,
            persist_state: self.persist_state,
            attached_slot_ext: &self.attached_slot_ext,
        }))
    }
}

// Manual impls: derives would demand `LLL: Debug`/`Clone` although only `Arc`s and
// the `NodeExtTypes`-bounded ext are stored (the MacroSpec/SpecialsSpec pattern;
// `SlotExt` is `Clone + Debug` by the `NodeExtTypes` bounds).

impl<LLL: LatexlikeLang> fmt::Debug for InputMacroSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputMacroSpec")
            .field("arguments", &self.arguments)
            .field("persist_state", &self.persist_state)
            .field("attached_slot_ext", &self.attached_slot_ext)
            .field("provenance", &self.provenance)
            .finish()
    }
}

impl<LLL: LatexlikeLang> Clone for InputMacroSpec<LLL> {
    fn clone(&self) -> Self {
        InputMacroSpec {
            arguments: self.arguments.clone(),
            persist_state: self.persist_state,
            attached_slot_ext: self.attached_slot_ext.clone(),
            provenance: self.provenance.clone(),
        }
    }
}

/// The `\input` invocation parser — the **brief form** the attachment helpers
/// exist for: declared arguments → argument text → attach → `Attached` slot.
struct InputInvocationParser<'a, LLL: LatexlikeLang> {
    invocation: Invocation<'a, LLL>,
    persist_state: bool,
    attached_slot_ext: &'a SlotExt<LLL>,
}

impl<LLL> ConstructParser<LLL> for InputInvocationParser<'_, LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<Box<ParsingStateDelta<LLL>>>)> {
        let token = self.invocation.token;
        let name = cx.tokens.source_span_between(token, TokenEdge::Start, TokenEdge::End);

        // 1. The declared arguments (the shared core loop).
        let (mut children, arguments) =
            parse_declared_arguments(cx, self.invocation.spec, &name)?;

        // The invocation's extent in the *includer's* source: trigger through the
        // last argument node — where the reader now stands (the trigger's own end
        // when the argument is absent).
        let end = cx.tokens.position_here();
        let at = cx.source_span_within(
            &cx.tokens.position_at(token, TokenEdge::Start),
            &end,
        )?;

        // 2. The reference, exactly as written. It drives source resolution, so it is
        //    read where it can be read exactly: off the staged argument's own node data
        //    — the character payloads of its content nodes — under every language,
        //    whether or not it obeys span tiling. (A span covering several tokens is
        //    only what the reader described for them; for a language that obeys span
        //    tiling it describes them exactly, and then it says nothing the node data
        //    does not.) The argument's content must be plain characters: anything else
        //    is diagnosed here and no reference is read.
        let reference: Option<String> = match arguments
            .first()
            .filter(|argument| argument.is_provided())
        {
            Some(argument) => match argument_text(cx, argument, &children) {
                Ok(text) => Some(text),
                // The document's mistake: recovered, nothing resolved.
                Err(ArgumentTextError::NotPlainCharacters) => {
                    let anchor =
                        argument_span(cx, argument, &children).unwrap_or_else(|| at.clone());
                    cx.recover(
                        InvalidSourceReferenceArgument::new(
                            InvalidReferenceReason::NotPlainCharacters,
                        ),
                        anchor,
                    )?;
                    None
                }
                // A staged record that does not resolve is a bug in the machinery, never
                // the document's doing — it aborts under any recovery policy.
                Err(ArgumentTextError::Malformed(detail)) => {
                    return Err(cx.implementation_error(detail, at.clone()))
                }
            },
            // An absent argument: the argument parser already diagnosed the missing
            // mandatory argument, and there is nothing here to add to it.
            None => None,
        };

        // 3. Resolve + attach through the single raising site, driving the root
        //    nodes-parse shape under the state at the `\input` point.
        let attached = match &reference {
            Some(reference) => {
                // A factory Err aborts under any policy ("could not build the
                // parser"), with the live traceback attached here.
                let driver = cx.driver;
                let mut parser = driver
                    .make_nodes_parser(StopSpec::none(), ChildStateSpec::inherit())
                    .map_err(|error| cx.attach_hook_frames(error))?;
                cx.attach_source_reference(
                    reference,
                    &at,
                    Arc::clone(&cx.state),
                    &mut *parser,
                )?
            }
            // No reference to resolve, and the failure is already diagnosed: an
            // absent argument by the argument parser, a provided argument whose
            // content is not plain characters by step 2's
            // `InvalidSourceReferenceArgument`. Nothing is resolved and nothing is
            // attached in either case.
            None => None,
        };

        // 4. The attached content becomes the `Attached` slot — present exactly
        //    when a source was attached (an empty file attaches an empty slot;
        //    a diagnosed resolution failure attaches none). The slot's ext is the
        //    embedder-supplied value, cloned per invocation — this spec never
        //    decides body-ness itself.
        let (slots, after_effects) = match attached {
            Some(outcome) => {
                let offset = children.len() as u32;
                let count = outcome.nodes.len() as u32;
                children.extend(outcome.nodes);
                let slots = ParsedSlots::new(vec![ParsedSlot::new(
                    ChildRegion::new(offset..offset + count, ContentNodes::InRegion(0..count)),
                    "attached",
                    SlotRole::Attached,
                    self.attached_slot_ext.clone(),
                )]);
                // The persist_state choice: forward the included run's merged
                // after-effect record — already boxed, moved as-is — as this
                // invocation's own after-effect (the existing sibling channel),
                // or stay transparent.
                let after_effects = if self.persist_state {
                    outcome.after_effects
                } else {
                    None
                };
                (slots, after_effects)
            }
            None => (ParsedSlots::empty(), None),
        };

        // 5. Stage. `Some(end)` pins the node's span to its invocation in the
        //    includer's source — the std last-child rule would reach into the
        //    attached source.
        let id = cx.stage_invocation(
            &self.invocation,
            ParsedArguments::from(arguments),
            slots,
            children,
            Some(&end),
        )?;
        Ok((id, after_effects))
    }
}

/// The span of a staged argument itself, for a diagnostic to point at: a delimited
/// argument's own node (delimiters included), or the extent of the nodes a bare
/// argument staged. `None` when the argument has no single span to point at — it was
/// not provided, it staged no node, or its nodes lie in more than one source (which a
/// language with [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`
/// allows); the caller then points at the invocation instead.
fn argument_span<LLL: LatexlikeLang>(
    cx: &ParseContext<'_, '_, LLL>,
    argument: &ParsedArgument<LLL>,
    children: &[BuildId],
) -> Option<SourceSpan<LLL::SourceOrigin>> {
    let region = argument.region.as_ref()?;
    // At parse time the region is staged by construction (`finish` has not run).
    let (offsets, content) = region.staged()?;
    let staged = cx.staged_nodes();
    if let ContentNodes::InChildrenOf(id, _) = content {
        return Some(staged.get(*id)?.span().clone());
    }
    let region_nodes = children.get(offsets.start as usize..offsets.end as usize)?;
    let first = staged.get(*region_nodes.first()?)?.span().clone();
    let last = staged.get(*region_nodes.last()?)?.span();
    if !first.same_source(last) || last.end() < first.start() {
        return None;
    }
    Some(SourceSpan::new(first.source(), first.start()..last.end()))
}

/// Why [`argument_text`] could not answer.
enum ArgumentTextError {
    /// The argument's content is not plain characters — the document's mistake, which
    /// the caller diagnoses as
    /// [`InvalidSourceReferenceArgument`](crate::constructs::InvalidSourceReferenceArgument).
    NotPlainCharacters,
    /// A staged record did not resolve: the region, its offsets, or a node it names.
    /// None of this depends on the document — the caller reports it as an
    /// implementation error, aborting under any recovery policy.
    Malformed(&'static str),
}

/// The text of a **provided** argument's content: the character payloads of its content
/// nodes, concatenated in order and read off the nodes' own data rather than off their
/// coordinates. Empty content (`\input{}`) reads as the empty text.
///
/// Node data is what a reference may be read from under every language. A chars node
/// carries the text the reader answered for it; a span covering several nodes is only a
/// description of the stretch they were read from — exact for a language that obeys
/// span tiling ([`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)), and no
/// more than a description for one that does not.
///
/// [`NotPlainCharacters`](ArgumentTextError::NotPlainCharacters) for content holding a
/// group, a callable or a comment node: such node data carries no single text.
/// [`Malformed`](ArgumentTextError::Malformed) for a staged record that does not
/// resolve — the caller must have checked
/// [`is_provided`](crate::node::ParsedArgument::is_provided) first.
fn argument_text<LLL: LatexlikeLang>(
    cx: &ParseContext<'_, '_, LLL>,
    argument: &ParsedArgument<LLL>,
    children: &[BuildId],
) -> Result<String, ArgumentTextError> {
    let malformed = ArgumentTextError::Malformed;
    let region = argument
        .region
        .as_ref()
        .ok_or(malformed("a provided reference argument records no child region"))?;
    // At parse time the region is staged by construction (`finish` has not run).
    let (offsets, content) = region
        .staged()
        .ok_or(malformed("the reference argument's child region is already resolved"))?;
    let region_nodes = children
        .get(offsets.start as usize..offsets.end as usize)
        .ok_or(malformed("the reference argument's region lies outside the staged children"))?;
    let staged = cx.staged_nodes();
    let text_of = |ids: &[BuildId]| -> Result<String, ArgumentTextError> {
        let mut text = String::new();
        for id in ids {
            let view = staged
                .get(*id)
                .ok_or(malformed("the reference argument names a node that is not staged"))?;
            match view.kind() {
                NodeKind::Chars { content, .. } => {
                    text.push_str(content.resolve(view.span().source()))
                }
                _ => return Err(ArgumentTextError::NotPlainCharacters),
            }
        }
        Ok(text)
    };
    match content {
        ContentNodes::InRegion(range) => text_of(
            region_nodes
                .get(range.start as usize..range.end as usize)
                .ok_or(malformed("the reference argument's content lies outside its region"))?,
        ),
        ContentNodes::InChildrenOf(id, range) => {
            let view = staged.get(*id).ok_or(malformed(
                "the reference argument's content parent is not staged",
            ))?;
            text_of(view.children().get(range.start as usize..range.end as usize).ok_or(
                malformed("the reference argument's content lies outside its content parent"),
            )?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::root_shapes;
    use super::super::{
        check_latexlike_tree_invariants, BodyMarker, CallableType, GroupType, Latexlike,
        LatexlikeDriver, MacroSpec,
    };
    use super::*;
    use crate::constructs::{
        InvalidReferenceReason, InvalidSourceReferenceArgument, NoSourceResolver, StrayGroupClose,
        UnresolvableCommand, UnresolvableSourceReference,
    };
    use crate::engine::Language;
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::node::BodySlotExt;
    use crate::scopes::Package;
    use crate::source::{
        check_include_chain, MapResolver, ResolveError, ResolvedContent,
        SourceProvenance, SourceResolver, SourceSpan,
    };
    use crate::state::{CommentOverrides, ParsingState, TokenRulesOverrides};

    /// The **shipped registration recipe**: `\input` state-transparent, the attached
    /// slot carrying the preset's not-body marker (Ruling A: the preset never
    /// overloads the environment-body marker).
    fn input_package() -> Package<Latexlike> {
        input_package_with(false, BodyMarker::not_body())
    }

    fn input_package_with(persist_state: bool, ext: BodyMarker) -> Package<Latexlike> {
        let mut package = Package::new("inputs");
        package.insert(CallableType::Macro, "input", input_macro_spec(persist_state, ext));
        package
    }

    /// A language whose driver resolves the given references (origins labeled with
    /// the reference — the canonical-origin invariant) and defines `\input` under
    /// the shipped registration recipe.
    fn language(recovery: Recovery, entries: &[(&str, &str)]) -> Language<Latexlike> {
        language_with_packages(recovery, entries, [input_package()])
    }

    fn language_with_packages(
        recovery: Recovery,
        entries: &[(&str, &str)],
        packages: impl IntoIterator<Item = Package<Latexlike>>,
    ) -> Language<Latexlike> {
        let mut resolver = MapResolver::new();
        for (reference, content) in entries {
            resolver.insert(*reference, *content);
        }
        Language::new(
            LatexlikeDriver::new(recovery)
                .with_source_resolver(resolver.with_reference_as_origin()),
            ParsingState::lang_initial_with_packages(packages).expect("seed state"),
        )
        // Explicit guard: nested inclusions stack enough construct levels to trip
        // the unconfigured default's half-budget warning in debug builds.
        .with_descent_guard_init(crate::engine::StdDescentGuardInit::depth_limit(64))
    }

    /// `\input` defined but no resolver configured.
    fn language_without_resolver(recovery: Recovery) -> Language<Latexlike> {
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([input_package()]).expect("seed state"),
        )
    }

    #[test]
    fn a_self_including_source_is_refused_by_the_shared_session_guard() {
        // The attached-source sub-parse runs on the SAME session — and therefore
        // under the same descent guard — as the includer: an inclusion cycle (a
        // source `\input`-ing itself through a resolver with no cycle check of
        // its own) is cut off at the configured depth limit as an ordinary
        // error, aborting under the tolerant policy too — never unbounded
        // recursion. (`language()` configures `depth_limit(64)`.)
        use crate::constructs::DescentLimitExceeded;

        let language =
            language(Recovery::Tolerant, &[("self.tex", r"\input{self.tex}")]);
        let err = language.parse(r"\input{self.tex}").unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
    }

    #[test]
    fn input_attaches_the_resolved_content_as_the_attached_slot() {
        let language =
            language(Recovery::Strict, &[("chapter.tex", "hello {world}")]);
        let result = language.parse(r"A\input{chapter.tex}B").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        assert_eq!(root_shapes(&result), ["chars(A)", "Macro(input)", "chars(B)"]);

        let input = result.tree.root().child(1).unwrap();
        // The node's span is its invocation in the *includer's* source.
        assert_eq!(input.span().range(), 1..20);
        assert_eq!(input.span_content(), r"\input{chapter.tex}");
        assert!(Arc::ptr_eq(input.span().source(), result.tree.root().span().source()));

        // The argument records the reference.
        let arguments = input.arguments().unwrap();
        assert_eq!(arguments.len(), 1);
        assert!(arguments.get(0).unwrap().is_provided());
        assert_eq!(
            input.argument_content_nodes_named("reference").unwrap().unwrap().source_text(),
            Some("chapter.tex")
        );

        // The attached slot: role `Attached`, named "attached", carrying the
        // embedder-supplied ext — the shipped recipe's not-body marker (Ruling A:
        // the preset does not overload the environment-body marker), so `body()`
        // does NOT select it and retrieval is by slot name.
        let slots = input.slots().unwrap();
        assert_eq!(slots.len(), 1);
        let slot = slots.get(0).unwrap();
        assert_eq!(slot.name(), Some("attached"));
        assert_eq!(slot.role, SlotRole::Attached);
        assert!(!slot.ext.is_body());
        assert!(input.body().is_none());

        // Per-source facts: the attached children live in the resolved source and
        // tile it in full — the named slot reads them back as one single-source
        // slice.
        let attached = input.slot_content_nodes_named("attached").unwrap();
        assert_eq!(attached.source_text(), Some("hello {world}"));
        let attached_span = attached.span().unwrap();
        assert!(!Arc::ptr_eq(attached_span.source(), input.span().source()));
        assert_eq!(attached_span.range(), 0..13);
        match attached_span.source().provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at.range(), 1..20);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }
        // The resolver labeled the origin with the reference.
        assert_eq!(attached_span.source().origin().as_deref(), Some("chapter.tex"));
    }

    #[test]
    fn nested_inputs_attach_recursively_with_chained_provenance() {
        let language = language(
            Recovery::Strict,
            &[("outer.tex", r"x\input{inner.tex}y"), ("inner.tex", "deep")],
        );
        let result = language.parse(r"\input{outer.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());

        let outer = result.tree.root().child(0).unwrap();
        let outer_attached = outer.slot_content_nodes_named("attached").unwrap();
        assert_eq!(outer_attached.len(), 3); // "x", \input{inner.tex}, "y"
        let inner = outer_attached.get(1).unwrap();
        assert_eq!(inner.name(), Some("input"));
        let inner_attached = inner.slot_content_nodes_named("attached").unwrap();
        assert_eq!(inner_attached.source_text(), Some("deep"));

        // The include chain is walkable from the innermost source: inner →
        // outer → primary.
        assert_eq!(
            inner_attached.span().unwrap().source().including_sources().count(),
            3
        );
    }

    #[test]
    fn input_without_a_resolver_diagnoses_and_stages_no_attached_slot() {
        let language = language_without_resolver(Recovery::Tolerant);
        let result = language.parse(r"A\input{chapter.tex}B").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), NoSourceResolver::IDENTIFIER);
        // Raised at the invocation span.
        assert_eq!(diagnostic.span().range(), 1..20);

        // The callable is staged with its argument but no attached slot; the
        // rest of the document still parses.
        assert_eq!(root_shapes(&result), ["chars(A)", "Macro(input)", "chars(B)"]);
        let input = result.tree.root().child(1).unwrap();
        assert!(input.slots().unwrap().is_empty());
        assert!(input.body().is_none());

        // Strict parses abort with the same condition.
        let strict = language_without_resolver(Recovery::Strict);
        let err = strict.parse(r"A\input{chapter.tex}B").unwrap_err();
        assert_eq!(err.identifier(), NoSourceResolver::IDENTIFIER);
    }

    #[test]
    fn input_with_an_unknown_reference_diagnoses_unresolvable() {
        let language = language(Recovery::Tolerant, &[]);
        let result = language.parse(r"\input{missing.tex} tail").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), UnresolvableSourceReference::IDENTIFIER);
        let condition = diagnostic
            .data()
            .downcast_ref::<UnresolvableSourceReference>()
            .unwrap();
        assert_eq!(condition.reference, "missing.tex");
        let input = result.tree.root().child(0).unwrap();
        assert!(input.slots().unwrap().is_empty());
    }

    #[test]
    fn the_reference_is_the_arguments_characters_whitespace_included() {
        // The reference is the argument's content characters as read, unnormalized:
        // whitespace inside the braces belongs to the chars node and therefore to the
        // reference. The resolver keyed on the padded name is what resolves.
        let padded = language(Recovery::Tolerant, &[(" chap.tex", "included")]);
        let result = padded.parse(r"\input{ chap.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let input = result.tree.root().child(0).unwrap();
        assert_eq!(
            input.slot_content_nodes_named("attached").unwrap().source_text(),
            Some("included")
        );
        // Trailing whitespace belongs to the chars node too, and so to the reference.
        let plain = language(Recovery::Tolerant, &[("chap.tex ", "included")]);
        let result = plain.parse(r"\input{chap.tex }").unwrap();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let input = result.tree.root().child(0).unwrap();
        assert_eq!(
            input.slot_content_nodes_named("attached").unwrap().source_text(),
            Some("included")
        );
    }

    #[test]
    fn a_reference_argument_that_is_not_plain_characters_is_diagnosed() {
        // The reference argument carries plain text — that is the rule. Here the
        // content is a protective group (`\input{{chap.tex}}`), which carries no text
        // to resolve: the condition is raised at the argument's span, nothing is
        // attached, and the rest of the document parses.
        let language = language(Recovery::Tolerant, &[("chap.tex", "included")]);
        let result = language.parse(r"\input{{chap.tex}} tail").unwrap();
        check_latexlike_tree_invariants(&result.tree);

        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), InvalidSourceReferenceArgument::IDENTIFIER);
        let condition = diagnostic
            .data()
            .downcast_ref::<InvalidSourceReferenceArgument>()
            .unwrap();
        assert_eq!(condition.reason, InvalidReferenceReason::NotPlainCharacters);
        // The rendered wording names the reason.
        assert_eq!(
            diagnostic.message(),
            "invalid source reference argument: its content is not plain characters"
        );
        // Anchored at the argument, delimiters included — `{{chap.tex}}`.
        assert_eq!(diagnostic.span().range(), 6..18);
        assert_eq!(diagnostic.span().content(), "{{chap.tex}}");

        // Nothing was resolved: no attached slot, and the resolver — which *does* know
        // `chap.tex` — was never asked for the braces-included literal.
        let input = result.tree.root().child(0).unwrap();
        assert!(input.slots().unwrap().is_empty());
        assert_eq!(root_shapes(&result), ["Macro(input)", "chars( tail)"]);
    }

    #[test]
    fn a_comment_or_a_callable_in_the_reference_argument_is_diagnosed_too() {
        // The same condition for the other two ways an argument's content stops being
        // plain characters: a comment node, and a callable node.
        let language = language(Recovery::Tolerant, &[("chap.tex", "included")]);
        let result = language.parse("\\input{chap%c\n.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(
            result.diagnostics.iter().next().unwrap().identifier(),
            InvalidSourceReferenceArgument::IDENTIFIER
        );

        let mut macros = Package::new("macros");
        macros.insert(CallableType::Macro, "x", MacroSpec::new(vec![]));
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("chap.tex", "included")],
            [input_package(), macros],
        );
        let result = language.parse(r"\input{\x}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(
            result.diagnostics.iter().next().unwrap().identifier(),
            InvalidSourceReferenceArgument::IDENTIFIER
        );
        assert!(result.tree.root().child(0).unwrap().slots().unwrap().is_empty());
    }

    #[test]
    fn an_unresolvable_command_in_the_argument_is_recovered_as_reference_characters() {
        // The condition fires on callable *nodes*, and an unresolvable command never
        // stages one: the content run recovers it as characters, so the recovered text
        // is the reference the resolver is asked for.
        let language = language(Recovery::Tolerant, &[("chap.tex", "included")]);
        let result = language.parse(r"\input{\undefined.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);

        let identifiers: Vec<_> =
            result.diagnostics.iter().map(|d| d.identifier()).collect();
        assert_eq!(
            identifiers,
            [UnresolvableCommand::IDENTIFIER, UnresolvableSourceReference::IDENTIFIER],
            "{:?}",
            result.diagnostics
        );
        let condition = result
            .diagnostics
            .iter()
            .nth(1)
            .unwrap()
            .data()
            .downcast_ref::<UnresolvableSourceReference>()
            .unwrap();
        assert_eq!(condition.reference, r"\undefined.tex");
    }

    #[test]
    fn a_strict_parse_aborts_on_a_reference_argument_that_is_not_plain_characters() {
        let strict = language(Recovery::Strict, &[("chap.tex", "included")]);
        let err = strict.parse(r"\input{{chap.tex}}").unwrap_err();
        assert_eq!(err.identifier(), InvalidSourceReferenceArgument::IDENTIFIER);
    }

    #[test]
    fn an_empty_included_file_attaches_an_empty_slot() {
        let language = language(Recovery::Strict, &[("empty.tex", "")]);
        let result = language.parse(r"\input{empty.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let input = result.tree.root().child(0).unwrap();
        // The slot exists (the file *was* attached), with zero children.
        let slot = input.slots().unwrap().get(0).unwrap();
        assert_eq!(slot.role, SlotRole::Attached);
        assert_eq!(input.slot_content_nodes_named("attached").unwrap().len(), 0);
    }

    #[test]
    fn a_stray_close_in_the_included_file_never_unwinds_the_includer() {
        // The `\input` sits *inside a group*; the included file has a stray `}`.
        // Local recovery: the includer's group still closes at its own `}` —
        // the included close is diagnosed and staged as chars in the attached
        // source.
        let language = language(Recovery::Tolerant, &[("frag.tex", "a}b")]);
        let result = language.parse(r"{\input{frag.tex}}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        // The diagnostic points into the *attached* source, inside the door's frame.
        assert_eq!(diagnostic.span().range(), 1..2);
        assert!(diagnostic.frames().iter().any(|f| f.title() == "attached source"));

        // The outer group is intact and spans the whole input.
        let group = result.tree.root().child(0).unwrap();
        assert!(group.is_group());
        assert_eq!(group.span().range(), 0..18);
        let input = group.child(0).unwrap();
        assert_eq!(input.name(), Some("input"));
        // The attached content carries all three pieces, `}` included.
        assert_eq!(
            input.slot_content_nodes_named("attached").unwrap().source_text(),
            Some("a}b")
        );
    }

    #[test]
    fn input_is_never_preloaded() {
        // Plain Latexlike (the canonical seed): `\input` is not defined — the
        // spec is strictly opt-in.
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::<Latexlike>::lang_initial().expect("seed state"),
        );
        let result = language.parse(r"\input{chapter.tex}").unwrap();
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics.iter().next().unwrap().identifier(),
            UnresolvableCommand::IDENTIFIER
        );
    }

    /// The embedder-policy recipe: a resolver enforcing cycle/depth policy with
    /// [`check_include_chain`] before delegating (origins = canonical names).
    #[derive(Debug)]
    struct PolicyResolver {
        map: MapResolver,
    }

    impl SourceResolver for PolicyResolver {
        fn resolve(
            &self,
            reference: &str,
            triggered_at: &SourceSpan,
        ) -> Result<ResolvedContent, ResolveError> {
            check_include_chain(
                &String::from(reference),
                triggered_at,
                |origin: &Option<String>| origin.clone(),
                Some(8),
            )?;
            SourceResolver::<Option<String>>::resolve(&self.map, reference, triggered_at)
        }
    }

    #[test]
    fn a_policy_resolver_turns_self_inclusion_into_a_diagnosed_cycle() {
        // `a.tex` includes itself. The *core* never blocks this (`.dtx`-style
        // self-inclusion is legal); the embedder's resolver enforces its own
        // policy via check_include_chain, and the violation surfaces as an
        // ordinary unresolvable-reference diagnostic at the inner `\input`.
        let mut map = MapResolver::new();
        map.insert("a.tex", r"x\input{a.tex}y");
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant)
                .with_source_resolver(PolicyResolver { map: map.with_reference_as_origin() }),
            ParsingState::lang_initial_with_packages([input_package()]).expect("seed state"),
        );

        let result = language.parse(r"\input{a.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), UnresolvableSourceReference::IDENTIFIER);
        let condition = diagnostic
            .data()
            .downcast_ref::<UnresolvableSourceReference>()
            .unwrap();
        assert!(condition.error.message().contains("include cycle"));

        // The outer inclusion succeeded — one attached level, the inner `\input`
        // staged without a slot.
        let outer = result.tree.root().child(0).unwrap();
        let outer_attached = outer.slot_content_nodes_named("attached").unwrap();
        let inner = outer_attached.get(1).unwrap();
        assert_eq!(inner.name(), Some("input"));
        // The failed inner `\input` recorded no slot at all: the by-name access is
        // the unknown-name category error, not a silent miss.
        assert!(matches!(
            inner.slot_content_nodes_named("attached"),
            Err(crate::node::NamedAccessError::UnknownSlotName { .. })
        ));
    }

    #[test]
    fn the_reference_argument_accepts_the_expression_fallback() {
        // `\input a` — the `{` argument code's single-expression fallback: the
        // one-char reference "a".
        let language = language(Recovery::Strict, &[("a", "ok")]);
        let result = language.parse(r"\input a").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let input = result.tree.root().child(0).unwrap();
        assert_eq!(
            input.slot_content_nodes_named("attached").unwrap().source_text(),
            Some("ok")
        );
    }

    #[test]
    fn parse_result_recomposes_per_source_facts() {
        // The I-18 reconstruction shape: every recomposition-relevant fact of the
        // multi-source tree is recorded — the includer's bytes reproduce from the
        // root's non-attached spans, the attached source's bytes from the body.
        let language = language(Recovery::Strict, &[("part.tex", "in{ner}")]);
        let source_text = r"pre\input{part.tex}post";
        let result = language.parse(source_text).unwrap();
        check_latexlike_tree_invariants(&result.tree);

        // Root children tile the includer's source exactly (span tiling over
        // the primary source ignores the attached region).
        let root = result.tree.root();
        let mut rebuilt = String::new();
        for child in root.children().iter() {
            assert!(Arc::ptr_eq(child.span().source(), root.span().source()));
            rebuilt.push_str(child.span_content());
        }
        assert_eq!(rebuilt, source_text);

        // And the attached source rebuilds from its own slot's children.
        let input = root.child(1).unwrap();
        let slot_nodes = input.slot_content_nodes_named("attached").unwrap();
        let mut attached = String::new();
        for child in slot_nodes.iter() {
            attached.push_str(child.span_content());
        }
        assert_eq!(attached, "in{ner}");
    }

    #[test]
    fn a_body_marked_ext_makes_the_attached_slot_findable_as_the_body() {
        // The framework-choice path: an embedder passing a body-marked ext gets
        // an `Attached` slot that `NodeRef::body()` finds — the T5 findability
        // clause ([§dd-dr:slot-roles]: `body()` selects on the ext axis alone,
        // no hidden role conjunction, so an Attached-body choice is never
        // silently unlocatable). The pairing is a framework option, never the
        // shipped default.
        let language = language_with_packages(
            Recovery::Strict,
            &[("chapter.tex", "content")],
            [input_package_with(false, BodyMarker::make_body())],
        );
        let result = language.parse(r"\input{chapter.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let input = result.tree.root().child(0).unwrap();
        let slot = input.slots().unwrap().get(0).unwrap();
        assert_eq!(slot.role, SlotRole::Attached);
        assert!(slot.ext.is_body());
        assert_eq!(input.body().unwrap().source_text(), Some("content"));
    }

    // --- persist_state (Ruling B) ----------------------------------------------------

    /// A package defining `\{name}` as a `\def`-style macro whose after-effect is
    /// `delta` — the public path ([`MacroSpec::with_after_effect`]).
    fn defining_package(name: &str, delta: ParsingStateDelta<Latexlike>) -> Package<Latexlike> {
        let mut package = Package::new(name);
        package.insert(CallableType::Macro, name, MacroSpec::new(vec![]).with_after_effect(delta));
        package
    }

    /// An after-effect delta pushing a provider that defines the zero-argument
    /// macro `\{defined}`.
    fn definition_delta(defined: &str, package_name: &str) -> ParsingStateDelta<Latexlike> {
        let mut lib: Package<Latexlike> = Package::new(package_name);
        lib.insert(CallableType::Macro, defined, MacroSpec::new(vec![]));
        ParsingStateDelta::new().push_provider(Arc::new(lib))
    }

    #[test]
    fn persist_state_true_carries_included_definitions_past_the_input() {
        // Persist test (a), the paradigm case: the included file's `\def`
        // registers `\x` via an after-effect delta; under `persist_state: true`
        // the definition is USED after the `\input` in the includer.
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("defs.tex", r"\def\x")],
            [
                input_package_with(true, BodyMarker::not_body()),
                defining_package("def", definition_delta("x", "xdefs")),
            ],
        );
        let result = language.parse(r"\input{defs.tex}\x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
        // The included run resolved its own `\x` (state evolves within the run
        // regardless of persist_state) AND the includer's `\x` after the
        // `\input` resolved through the persisted definition.
        assert_eq!(root_shapes(&result), ["Macro(input)", "Macro(x)"]);
        let input = result.tree.root().child(0).unwrap();
        let attached = input.slot_content_nodes_named("attached").unwrap();
        assert_eq!(attached.len(), 2); // \def, \x — both resolved inside too
    }

    #[test]
    fn persist_state_false_leaves_the_includers_state_untouched() {
        // Persist test (b): the SAME input under `persist_state: false` — the
        // included run still resolves its own `\x` (transparent means the
        // after-effects end with the file, not that they never applied), but
        // the includer's `\x` after the `\input` is unresolvable.
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("defs.tex", r"\def\x")],
            [
                input_package_with(false, BodyMarker::not_body()),
                defining_package("def", definition_delta("x", "xdefs")),
            ],
        );
        let result = language.parse(r"\input{defs.tex}\x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        // Exactly one diagnostic: the includer-side `\x`.
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), UnresolvableCommand::IDENTIFIER);
        assert_eq!(diagnostic.span().range(), 16..18);
        assert!(Arc::ptr_eq(
            diagnostic.span().source(),
            result.tree.root().span().source()
        ));
        // Inside the included file the definition applied as usual.
        let input = result.tree.root().child(0).unwrap();
        let attached = input.slot_content_nodes_named("attached").unwrap();
        assert_eq!(attached.get(1).unwrap().name(), Some("x"));
    }

    #[test]
    fn nested_inclusion_composes_persisted_effects_to_the_primary() {
        // Persist test (c): inner.tex defines `\x`; outer.tex uses it after its
        // own `\input` (inner persisted effects visible to the outer file's
        // remainder) and the primary uses it after the outer `\input` (the
        // inner-origin delta rides the outer run's merged record outward).
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("outer.tex", r"\input{inner.tex}\x"), ("inner.tex", r"\def")],
            [
                input_package_with(true, BodyMarker::not_body()),
                defining_package("def", definition_delta("x", "xdefs")),
            ],
        );
        let result = language.parse(r"\input{outer.tex}\x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
        assert_eq!(root_shapes(&result), ["Macro(input)", "Macro(x)"]);
        // Outer file's remainder saw the inner definition.
        let outer = result.tree.root().child(0).unwrap();
        let outer_attached = outer.slot_content_nodes_named("attached").unwrap();
        assert_eq!(outer_attached.get(1).unwrap().name(), Some("x"));
    }

    #[test]
    fn merged_after_effects_apply_in_order() {
        // Persist test (d): two delta-producing constructs in the included file.
        //
        // (d1) Field override, last-writer-wins: `\con` re-enables comments,
        // `\coff` disables them — the later override governs the includer after
        // the `\input`, so `%x` stages as plain chars (an empty or first-wins
        // record would leave comments enabled and stage a comment node).
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("toggles.tex", r"\con\coff")],
            [
                input_package_with(true, BodyMarker::not_body()),
                defining_package(
                    "con",
                    ParsingStateDelta::new().rules(TokenRulesOverrides {
                        comments: CommentOverrides {
                            enabled: Some(true),
                            ..CommentOverrides::default()
                        },
                        ..TokenRulesOverrides::default()
                    }),
                ),
                defining_package(
                    "coff",
                    ParsingStateDelta::new().rules(TokenRulesOverrides {
                        comments: CommentOverrides::disable(),
                        ..TokenRulesOverrides::default()
                    }),
                ),
            ],
        );
        let result = language.parse("\\input{toggles.tex}%x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
        assert_eq!(root_shapes(&result), ["Macro(input)", "chars(%x)"]);

        // (d2) Scope pushes concatenate in application order: both constructs
        // define `\x`, the later push is innermost and wins resolution — its
        // zero-argument shape leaves `{q}` a sibling group (the earlier,
        // one-argument shape would consume it).
        let takes_arg = {
            let mut lib: Package<Latexlike> = Package::new("xa");
            lib.insert(
                CallableType::Macro,
                "x",
                MacroSpec::new(vec![Arc::new(ArgumentSpec::new(
                    Arc::new(GroupArgumentParser::new(GroupType::Content)),
                    "arg",
                ))]),
            );
            Arc::new(lib)
        };
        let no_arg = {
            let mut lib: Package<Latexlike> = Package::new("xb");
            lib.insert(CallableType::Macro, "x", MacroSpec::new(vec![]));
            Arc::new(lib)
        };
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("defs.tex", r"\defa\defb")],
            [
                input_package_with(true, BodyMarker::not_body()),
                defining_package("defa", ParsingStateDelta::new().push_provider(takes_arg)),
                defining_package("defb", ParsingStateDelta::new().push_provider(no_arg)),
            ],
        );
        let result = language.parse(r"\input{defs.tex}\x{q}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
        let shapes = root_shapes(&result);
        assert_eq!(shapes[1], "Macro(x)");
        assert!(shapes[2].starts_with("group("), "expected a sibling group, got {:?}", shapes);
        let x = result.tree.root().child(1).unwrap();
        assert!(x.arguments().unwrap().is_empty());
    }

    #[test]
    fn cross_segment_after_effects_merge_across_a_stray_close_resume() {
        // Review should-fix: a stray `}` in the included file splits its run into
        // two segments (local recovery + resume). After-effect deltas from BOTH
        // segments must ride the door's merged record under `persist_state: true`
        // — a merge that only kept the first segment's record would leave `\b`
        // unresolvable in the includer.
        let language = language_with_packages(
            Recovery::Tolerant,
            &[("defs.tex", r"\defa}\defb")],
            [
                input_package_with(true, BodyMarker::not_body()),
                defining_package("defa", definition_delta("a", "adefs")),
                defining_package("defb", definition_delta("b", "bdefs")),
            ],
        );
        let result = language.parse(r"\input{defs.tex}\a\b").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        // Exactly the stray close's diagnostic, in the attached source.
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        // Both definitions — one from each segment — govern the includer.
        assert_eq!(root_shapes(&result), ["Macro(input)", "Macro(a)", "Macro(b)"]);
    }
}
