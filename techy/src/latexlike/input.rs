//! [`InputMacroSpec`] / [`input_macro_spec`]: the preset's opt-in `\input`-shaped
//! macro — resolve a referenced source and attach its parsed content to the
//! invocation.
//!
//! **Never preloaded**: the spec is not part of [`base_package`](super::base_package)
//! — an always-on `\input` under a resolver-less driver would just diagnose every
//! use. Embedders that want it insert it into their own package, under their own
//! macro callable type and any command name:
//!
//! ```
//! use techy::core::{Language, ParsingState};
//! use techy::core::specs::Package;
//! use techy::error::Recovery;
//! use techy::latexlike::{input_macro_spec, CallableType, Latexlike, LatexlikeDriver};
//! use techy::source::MapResolver;
//!
//! let mut resolver = MapResolver::new();
//! resolver.insert("chapter.tex", "included {content}");
//! let mut package: Package<Latexlike> = Package::new("mydefs");
//! package.insert(CallableType::Macro, "input", input_macro_spec());
//!
//! let language = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict).with_source_resolver(resolver),
//!     ParsingState::lang_initial_with_packages([package]),
//! );
//! let result = language.parse(r"a \input{chapter.tex} b").unwrap();
//! let input = result.tree.root().child(1).unwrap();
//! // The invocation's own span lives in the includer; the attached content is
//! // the node's body, parsed out of the resolved source.
//! assert_eq!(input.span_content(), r"\input{chapter.tex}");
//! assert_eq!(input.body().unwrap().source_text().unwrap(), "included {content}");
//! ```

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    parse_declared_arguments, ChildStateSpec, ConstructParser, ConstructParserResult,
    GroupArgumentParser, Invocation, ParseContext, StopSpec,
};
use crate::node::{
    ArgumentExt, BodySlotExt, BuildId, ChildRegion, ContentNodes, NodeKind,
    ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots, SlotExt, SlotRole,
};
use crate::engine::ParseDriver;
use crate::source::{SourceSpan, Span};
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::state::ParsingStateDelta;

use super::lang::LatexlikeGroupType;
use super::spec::frame_title;
use super::{Latexlike, LatexlikeLang};

/// The preset's opt-in `\input`-shaped macro spec: one mandatory `{…}` argument
/// naming an external source reference; the invocation resolves it through the
/// driver's [`SourceResolver`](crate::source::SourceResolver) and parses the
/// content **at the invocation point, into the same tree** — recorded as an
/// [`Attached`](SlotRole::Attached) slot (named `"attached"`) of the staged
/// callable, which is also the node's body slot
/// ([`NodeRef::body`](crate::node::NodeRef::body); body-ness is the ext axis,
/// recorded independently of the role). Constructed by [`input_macro_spec`],
/// **never preloaded** (see the module docs).
///
/// The node's own span is its invocation in the *includer's* source (`\input{…}`);
/// only the attached slot's children live in the resolved source — a multi-source
/// tree is first-class, and recomposition per source emits the invocation text,
/// not the content.
///
/// # Failure conditions
///
/// Resolution failures are diagnosed at the invocation span through the single
/// raising site ([`ParseContext::attach_source_reference`]):
/// [`NoSourceResolver`](crate::constructs::NoSourceResolver) when the driver has
/// no resolver, [`UnresolvableSourceReference`](crate::constructs::UnresolvableSourceReference)
/// when the resolver fails. Tolerant parses record the condition and stage the
/// callable *without* an attached slot; strict parses abort.
///
/// # State handling — deliberately transparent
///
/// The attached content parses under the parsing state at the `\input` point
/// (definitions in force there apply inside the included content), but its own
/// after-effects — a `\newcommand`-style definition made *inside* the included
/// file — do **not** continue into the rest of the including document: this spec
/// returns no after-effect delta. A framework whose `\input` must propagate
/// state (the preamble-defines-macros case) writes its own composition around
/// [`ParseContext::parse_attached_source`], where it controls both the sub-parse
/// and what it hands back to the caller.
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
}

/// Create the preset's opt-in `\input` spec ([`InputMacroSpec`] — never
/// preloaded; see the type and module docs).
///
/// # No input caching
///
/// The included file is read **on the spot, at parse time** — deliberately: the
/// parsing state at the `\input` point governs how the content tokenizes, and in
/// general an `\input`-style construct may even feed state back into the
/// including document (not this spec — see the type docs — but the general
/// shape), so a parse-without-attachment cache is not generally sound. techy
/// therefore neither implements nor recommends input caching; resolvers may
/// freely cache *content* (the [`SourceResolver`](crate::source::SourceResolver)
/// contract), which is the part that costs I/O. The guide's include chapter
/// discusses the trade-offs, including the separate-parse-then-splice recipe
/// that applies only when `\input` is known state-transparent.
pub fn input_macro_spec<LLL>() -> InputMacroSpec<LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
{
    InputMacroSpec {
        arguments: vec![Arc::new(ArgumentSpec::new(
            Arc::new(GroupArgumentParser::new(LLL::GroupTypeId::content_group())),
            "reference",
        ))],
    }
}

impl<LLL> CallableSpec<LLL> for InputMacroSpec<LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
    SlotExt<LLL>: BodySlotExt,
{
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }

    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, LLL>,
    ) -> alloc::boxed::Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        alloc::boxed::Box::new(InputInvocationParser { invocation })
    }
}

// Manual impls: derives would demand `LLL: Debug`/`Clone` although only `Arc`s are
// stored (the MacroSpec/SpecialsSpec pattern).

impl<LLL: LatexlikeLang> fmt::Debug for InputMacroSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputMacroSpec").field("arguments", &self.arguments).finish()
    }
}

impl<LLL: LatexlikeLang> Clone for InputMacroSpec<LLL> {
    fn clone(&self) -> Self {
        InputMacroSpec { arguments: self.arguments.clone() }
    }
}

/// The `\input` invocation parser — the **brief form** the attachment helpers
/// exist for: declared arguments → argument text → attach → `Attached` slot.
struct InputInvocationParser<'a, 's, LLL: LatexlikeLang> {
    invocation: Invocation<'a, 's, LLL>,
}

impl<LLL> ConstructParser<LLL> for InputInvocationParser<'_, '_, LLL>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
    SlotExt<LLL>: BodySlotExt,
{
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<ParsingStateDelta<LLL>>)> {
        let token = self.invocation.token;
        let name_span = Span::new(token.span.start(), token.post_space().start());

        // 1. The declared arguments (the shared core loop).
        let (mut children, arguments) =
            parse_declared_arguments(cx, self.invocation.spec, name_span)?;

        // The invocation's extent in the *includer's* source: trigger through the
        // last argument node (the trigger's own end when the argument is absent).
        let end = children
            .last()
            .and_then(|last| cx.staged_nodes().get(*last))
            .map(|view| view.span().end())
            .unwrap_or(token.span.end());
        let at = SourceSpan::new(&cx.source, token.span.start()..end);

        // 2. The argument text — the reference, exactly as written.
        let reference: Option<String> = arguments
            .first()
            .and_then(|argument| argument_text_span(cx, argument, &children))
            .map(|span| cx.source.content()[span.range()].to_string());

        // 3. Resolve + attach through the single raising site, driving the root
        //    nodes-parse shape under the state at the `\input` point.
        let attached = match &reference {
            Some(reference) => {
                let driver = cx.driver;
                let mut parser = driver
                    .make_nodes_parser(StopSpec::none(), ChildStateSpec::inherit());
                cx.attach_source_reference(
                    reference,
                    &at,
                    Arc::clone(&cx.state),
                    &mut *parser,
                )?
            }
            // Absent argument: the argument parser already diagnosed it.
            None => None,
        };

        // 4. The attached content becomes the `Attached` slot — present exactly
        //    when a source was attached (an empty file attaches an empty slot;
        //    a diagnosed resolution failure attaches none), and marked as the
        //    node's body on the ext axis.
        let slots = match attached {
            Some(nodes) => {
                let offset = children.len() as u32;
                let count = nodes.len() as u32;
                children.extend(nodes);
                ParsedSlots::new(vec![ParsedSlot::new(
                    ChildRegion::new(offset..offset + count, ContentNodes::InRegion(0..count)),
                    "attached",
                    SlotRole::Attached,
                    BodySlotExt::make_body(),
                )])
            }
            None => ParsedSlots::empty(),
        };

        // 5. Stage. `Some(end)` pins the node's span to its invocation in the
        //    includer's source — the std last-child rule would reach into the
        //    attached source.
        let id = cx.stage_invocation(
            &self.invocation,
            ParsedArguments::from(arguments),
            slots,
            children,
            Some(end),
        )?;
        Ok((id, None))
    }
}

/// The byte extent, in the context's source, of a staged argument's **content**
/// (its designated content nodes): a delimited group argument's interior, a bare
/// expression's own span; an empty group interior anchors after the open
/// delimiter. `None` for an absent argument (or content in no staged node).
fn argument_text_span<LLL: LatexlikeLang>(
    cx: &ParseContext<'_, '_, LLL>,
    argument: &ParsedArgument<LLL>,
    children: &[BuildId],
) -> Option<Span> {
    let region = argument.region.as_ref()?;
    // At parse time the region is staged by construction (`finish` has not run).
    let (offsets, content) = region.staged()?;
    let region_nodes = children.get(offsets.start as usize..offsets.end as usize)?;
    let staged = cx.staged_nodes();
    let span_of = |ids: &[BuildId]| -> Option<Span> {
        let first = staged.get(*ids.first()?)?.span().start();
        let last = staged.get(*ids.last()?)?.span().end();
        Some(Span::new(first, last))
    };
    match content {
        ContentNodes::InRegion(range) => {
            span_of(region_nodes.get(range.start as usize..range.end as usize)?)
        }
        ContentNodes::InChildrenOf(id, range) => {
            let view = staged.get(*id)?;
            let content_children =
                view.children().get(range.start as usize..range.end as usize)?;
            if let Some(span) = span_of(content_children) {
                return Some(span);
            }
            // Empty content (`\input{}`): anchor after the open delimiter.
            if let NodeKind::Group(group) = view.kind() {
                let open_len = group.open.resolve(view.span().source()).len();
                return Some(Span::empty(view.span().start() + open_len));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::root_shapes;
    use super::super::{
        check_latexlike_tree_invariants, CallableType, Latexlike, LatexlikeDriver,
    };
    use super::*;
    use crate::constructs::{NoSourceResolver, UnresolvableCommand, UnresolvableSourceReference, StrayGroupClose};
    use crate::engine::Language;
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::scopes::Package;
    use crate::source::{
        check_include_chain, MapResolver, ResolveError, ResolvedContent,
        SourceProvenance, SourceResolver,
    };
    use crate::state::ParsingState;

    fn input_package() -> Package<Latexlike> {
        let mut package = Package::new("inputs");
        package.insert(CallableType::Macro, "input", input_macro_spec());
        package
    }

    /// A language whose driver resolves the given references (origins labeled with
    /// the reference — the canonical-origin invariant) and defines `\input`.
    fn language(recovery: Recovery, entries: &[(&str, &str)]) -> Language<Latexlike> {
        let mut resolver = MapResolver::new();
        for (reference, content) in entries {
            resolver.insert(*reference, *content);
        }
        Language::new(
            LatexlikeDriver::new(recovery)
                .with_source_resolver(resolver.with_reference_as_origin()),
            ParsingState::lang_initial_with_packages([input_package()]),
        )
    }

    /// `\input` defined but no resolver configured.
    fn language_without_resolver(recovery: Recovery) -> Language<Latexlike> {
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([input_package()]),
        )
    }

    #[test]
    fn input_attaches_the_resolved_content_as_the_attached_body_slot() {
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
            input.argument_content_nodes_named("reference").unwrap().source_text(),
            Some("chapter.tex")
        );

        // The attached slot: role `Attached`, named "attached", the body slot.
        let slots = input.slots().unwrap();
        assert_eq!(slots.len(), 1);
        let slot = slots.get(0).unwrap();
        assert_eq!(slot.name(), Some("attached"));
        assert_eq!(slot.role, SlotRole::Attached);
        assert!(slot.ext.is_body());

        // Per-source facts: the attached children live in the resolved source and
        // tile it in full — body() reads them back as one single-source slice.
        let body = input.body().unwrap();
        assert_eq!(body.source_text(), Some("hello {world}"));
        let body_span = body.span().unwrap();
        assert!(!Arc::ptr_eq(body_span.source(), input.span().source()));
        assert_eq!(body_span.range(), 0..13);
        match body_span.source().provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at.range(), 1..20);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }
        // The resolver labeled the origin with the reference.
        assert_eq!(body_span.source().origin().as_deref(), Some("chapter.tex"));
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
        let outer_body = outer.body().unwrap();
        assert_eq!(outer_body.len(), 3); // "x", \input{inner.tex}, "y"
        let inner = outer_body.get(1).unwrap();
        assert_eq!(inner.name(), Some("input"));
        let inner_body = inner.body().unwrap();
        assert_eq!(inner_body.source_text(), Some("deep"));

        // The include chain is walkable from the innermost source: inner →
        // outer → primary.
        assert_eq!(
            inner_body.span().unwrap().source().including_sources().count(),
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
    fn an_empty_included_file_attaches_an_empty_slot() {
        let language = language(Recovery::Strict, &[("empty.tex", "")]);
        let result = language.parse(r"\input{empty.tex}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let input = result.tree.root().child(0).unwrap();
        // The slot exists (the file *was* attached), with zero children.
        let slot = input.slots().unwrap().get(0).unwrap();
        assert_eq!(slot.role, SlotRole::Attached);
        assert_eq!(input.body().unwrap().len(), 0);
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
        assert_eq!(input.body().unwrap().source_text(), Some("a}b"));
    }

    #[test]
    fn input_is_never_preloaded() {
        // Plain Latexlike (the canonical seed): `\input` is not defined — the
        // spec is strictly opt-in.
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::<Latexlike>::lang_initial(),
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
            ParsingState::lang_initial_with_packages([input_package()]),
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
        let outer_body = outer.body().unwrap();
        let inner = outer_body.get(1).unwrap();
        assert_eq!(inner.name(), Some("input"));
        assert!(inner.body().is_none());
    }

    #[test]
    fn the_reference_argument_accepts_the_expression_fallback(){
        // `\input a` — the `{` argument code's single-expression fallback: the
        // one-char reference "a".
        let language = language(Recovery::Strict, &[("a", "ok")]);
        let result = language.parse(r"\input a").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let input = result.tree.root().child(0).unwrap();
        assert_eq!(input.body().unwrap().source_text(), Some("ok"));
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

        // Root children tile the includer's source exactly (the parse law over
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
        let body = input.body().unwrap();
        let mut attached = String::new();
        for child in body.iter() {
            attached.push_str(child.span_content());
        }
        assert_eq!(attached, "in{ner}");
    }
}
