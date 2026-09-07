//! The preset's source re-emission: [`SourceRecomposer`], built by
//! [`source_recomposer`] — the recomposer that reconstructs the source text of a
//! latexlike-family tree out of the facts the parse recorded on its nodes.

// This module is private; the public story lives on the items' own docs.

use core::fmt;
use core::marker::PhantomData;

use alloc::string::String;

use crate::node::{NodeId, NodeRef};
use crate::recompose::{
    core_source_instruction, ComposePiece, ConcatPieces, Recompose, RecomposeContext,
    Recomposer,
};

use super::invocation_syntax::EnvironmentSyntax;
use super::lang::{LatexlikeCallableType, LatexlikeInvocationSyntax, LatexlikeLang};
use super::Latexlike;

/// Reconstructs the source text of a parsed latexlike-family tree.
///
/// This is the ready-made [`Recomposer`] for writing a tree back out as markup.
/// Build one with [`source_recomposer`] (or [`Default`]) and run it with a
/// [`TreeRecomposer`](crate::recompose::TreeRecomposer):
///
/// ```
/// use techy::core::specs::Package;
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::{source_recomposer, Latexlike, LatexlikeDriver};
/// use techy::recompose::TreeRecomposer;
///
/// let mut package: Package<Latexlike> = Package::new("mydefs");
/// package.define_macro("emph", ["m"]).unwrap();
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
/// );
///
/// let input = "a \\emph{b} % note\n  c";
/// let result = language.parse(input).unwrap();
/// let text: String = TreeRecomposer::new(&mut source_recomposer())
///     .recompose(&result.tree, ())
///     .unwrap();
/// assert_eq!(text, input);
/// ```
///
/// It serves any member of the language family (`LLL:` [`LatexlikeLang`]) and trees
/// carrying any annotation type.
///
/// # What it emits
///
/// Everything comes from what the parse recorded on the nodes themselves; the source
/// text under a node's own span is never read. Chars content, comment parts, and
/// group delimiters come through [`core_source_instruction`]; a callable is written
/// from the family's recorded invocation syntax — the escape character, the name as
/// written, and the post-space for a macro, the two sides' own spelling writers
/// ([`write_begin`](EnvironmentSyntax::write_begin) and
/// [`write_end`](EnvironmentSyntax::write_end)) for an environment, and the name as
/// written for a specials. On a
/// [`materialize`](crate::core::node::NodeTree::materialize)d tree the reconstruction
/// therefore reads no [`Source`](crate::source::Source) at all.
///
/// # What the output equals
///
/// For a tree the latexlike parse produced, the output equals the parsed source byte
/// for byte. Only what the parse recorded can be re-emitted (see
/// [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax)),
/// and a tolerant parse records the shapes it recovered as they were written: an
/// environment that never found its terminator has an empty end side and re-emits no
/// terminator, which is exactly what the input had. The one recovery that consumes
/// more than it records is the malformed environment terminator — its `\end` is
/// consumed on its own, diagnosed, and recorded nowhere — so that command spelling is
/// not reproduced.
///
/// Byte-equality with one source presupposes that the tree's nodes tile that source,
/// so it is stated for a language declaring
/// [`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING) `= true`, the
/// preset's own [`Latexlike`] included. A family member declaring it `false` has
/// parsers that assume nothing about where their tokens came from; its trees are
/// re-emitted **as stored** — the text the parser recorded, in tree order — and no
/// byte-equality with any one source is claimed for them.
///
/// # How it composes
///
/// `State = ()` and `Piece = `[`String`]. Every node is answered with an instruction
/// alone, and the recomposer never descends by itself, so it also works as the inner
/// recomposer of one that wraps it. It needs no scope call: the default `Concat` scope
/// already skips `Attached` and `Hidden` slot children, so an `\input`'s attached
/// content is re-emitted as the `\input{…}` invocation it came from rather than as the
/// included file's text.
///
/// # Errors
///
/// [`SourceRecomposeError::IncoherentInvocationSyntax`], for a callable whose recorded
/// invocation syntax does not match the role its callable type plays. No parse output
/// can hold such a node, so over a tree a parse produced the recomposition never
/// fails.
///
/// # Panics
///
/// Panics if a span-backed payload field of a node names a range that is not within
/// the content of that node's own source, or does not fall on character boundaries —
/// the panic condition of
/// [`TextContent::resolve`](crate::source::TextContent::resolve). That means a tree
/// invariant is broken, which no parsed input can cause; only a tree assembled by hand
/// through [`NodeTreeBuilder`](crate::core::node::NodeTreeBuilder) can reach it, and
/// [`validate_tree`](crate::core::node::validate_tree) is the check for it.
pub struct SourceRecomposer<LLL: LatexlikeLang = Latexlike> {
    _lang: PhantomData<LLL>,
}

/// Creates a [`SourceRecomposer`], which re-emits a parsed latexlike-family tree as
/// source text.
///
/// Run it with a [`TreeRecomposer`](crate::recompose::TreeRecomposer):
/// `TreeRecomposer::new(&mut source_recomposer()).recompose(&tree, ())`. The type's
/// documentation states what the output is guaranteed to equal, and the one condition
/// under which a run panics.
pub fn source_recomposer<LLL: LatexlikeLang>() -> SourceRecomposer<LLL> {
    SourceRecomposer { _lang: PhantomData }
}

// Manual impls: derives would demand `LLL:` bounds although only PhantomData
// is stored.

impl<LLL: LatexlikeLang> Clone for SourceRecomposer<LLL> {
    fn clone(&self) -> Self {
        SourceRecomposer { _lang: PhantomData }
    }
}

impl<LLL: LatexlikeLang> Default for SourceRecomposer<LLL> {
    fn default() -> Self {
        source_recomposer()
    }
}

impl<LLL: LatexlikeLang> fmt::Debug for SourceRecomposer<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceRecomposer").finish()
    }
}

/// The failure of a [`SourceRecomposer`] run.
///
/// A parse never produces a tree that causes one. The single variant reports a
/// callable whose recorded invocation syntax does not match the role its callable type
/// plays, which only a tree built by hand or restaged incoherently can hold: the parse
/// deliberately does not check the pairing when it records it, and the source
/// recomposer is where a mismatch surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceRecomposeError {
    /// The node's invocation-syntax payload records a different invocation
    /// form than the role its `callable_type` plays — the recorded spelling
    /// facts cannot be trusted to reemit this node (a hand-built or
    /// incoherently restaged tree; parses never produce this).
    IncoherentInvocationSyntax {
        /// The offending callable.
        node: NodeId,
        /// The role the node's `callable_type` plays (`"macro"`,
        /// `"environment"`, `"specials"` — or `"unknown"` for a form outside
        /// the family's three roles).
        callable_form: &'static str,
        /// The arm the payload records (same labels).
        payload_arm: &'static str,
    },
}

impl fmt::Display for SourceRecomposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceRecomposeError::IncoherentInvocationSyntax {
                node,
                callable_form,
                payload_arm,
            } => write!(
                f,
                "callable {:?}: the invocation-syntax payload records the {} arm but \
                 the node's callable type plays the {} role — the recorded spelling \
                 facts cannot reemit this node",
                node, payload_arm, callable_form
            ),
        }
    }
}

impl core::error::Error for SourceRecomposeError {}

impl<LLL: LatexlikeLang, A> Recomposer<LLL, A> for SourceRecomposer<LLL> {
    type State = ();
    type Piece = String;
    type Error = SourceRecomposeError;

    /// Returns the instruction that re-emits `node`'s recorded source spelling.
    ///
    /// # Errors
    ///
    /// [`SourceRecomposeError::IncoherentInvocationSyntax`] if `node` is a callable
    /// whose recorded invocation syntax does not match the role its callable type
    /// plays.
    ///
    /// # Panics
    ///
    /// Panics if a span-backed payload field of `node` names a range that is not
    /// within the content of the node's own source, or does not fall on character
    /// boundaries — see [`SourceRecomposer`].
    fn recompose_node(
        &mut self,
        node: NodeRef<'_, LLL, A>,
        _state: &(),
        _cx: &mut RecomposeContext<'_, LLL, A>,
    ) -> Result<Recompose<String, ()>, SourceRecomposeError> {
        // The core-complete kinds: chars, comments, groups, lists.
        if let Some(instruction) = core_source_instruction(node) {
            return Ok(instruction);
        }
        // `core_source_instruction` declines exactly the callables.
        let data = node
            .callable()
            .expect("core_source_instruction answers every non-callable kind");
        let payload = &data.invocation_syntax;
        let payload_arm = payload_arm::<LLL>(payload);
        let source = node.span().source();
        let name: &str = &data.name;

        // Dispatch on the role the callable type plays; the payload must
        // record the matching arm (coherence is unenforced at parse time —
        // this is where a mismatch surfaces).
        let incoherent = |callable_form: &'static str| {
            SourceRecomposeError::IncoherentInvocationSyntax {
                node: node.id(),
                callable_form,
                payload_arm,
            }
        };
        if data.callable_type.is_macro() {
            let Some((escape_char, post_space)) = payload.macro_syntax() else {
                return Err(incoherent("macro"));
            };
            // The trigger spelling as recorded: escape char + name as written
            // + the trigger token's own syntactic post-space; the argument
            // regions follow as children.
            let mut head = String::new();
            head.push(escape_char);
            head.push_str(name);
            head.push_str(post_space.resolve(source));
            Ok(Recompose::Concat(ConcatPieces::children().wrap(head, String::empty())))
        } else if data.callable_type.is_environment() {
            let Some(env) = payload.environment_syntax() else {
                return Err(incoherent("environment"));
            };
            // The record's own spelling writers, one per side (an empty end
            // side writes "" — recovered shapes reemit what was recorded).
            Ok(Recompose::Concat(ConcatPieces::children().wrap(
                env.write_begin(name, source),
                env.write_end(name, source),
            )))
        } else if data.callable_type.is_specials() {
            if !payload.is_specials() {
                return Err(incoherent("specials"));
            }
            // Name-as-written IS the invocation spelling (paragraph-break
            // Specials nodes record the whole whitespace run as name);
            // arguments, when a specials spec declares any, follow as
            // children.
            Ok(Recompose::Concat(
                ConcatPieces::children().wrap(String::from(name), String::empty()),
            ))
        } else {
            // A callable form outside the family's three roles: this
            // recomposer has no spelling rule for it.
            Err(incoherent("unknown"))
        }
    }
}

/// The label of the arm `payload` records (for the coherence diagnosis).
fn payload_arm<LLL: LatexlikeLang>(payload: &LLL::InvocationSyntax) -> &'static str {
    if payload.macro_syntax().is_some() {
        "macro"
    } else if payload.environment_syntax().is_some() {
        "environment"
    } else if payload.is_specials() {
        "specials"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::super::test_support::{macro_package, strict, with_package};
    use super::super::{
        argument_specs, CallableType, EnvironmentSpec, InvocationSyntaxData, Latexlike,
        LatexlikeDriver, MacroSpec, ParagraphBreakStyle, SpecialsSpec, VerbatimBehavior,
    };
    use super::*;
    use crate::engine::Language;
    use crate::error::Recovery;
    use crate::latexlike::check_latexlike_tree_invariants;
    use crate::node::{
        BuildId, CallableData, NodeKind, NodeTree, NodeTreeBuilder, ParsedArguments,
        ParsedSlots,
    };
    use crate::recompose::{RecomposeError, TreeRecomposer};
    use crate::scopes::Package;
    use crate::source::{Source, SourceSpan};
    use crate::state::ParsingState;

    /// The suite's shorthand: a default-configured [`TreeRecomposer`] run.
    fn recompose<A, R>(
        tree: &NodeTree<Latexlike, A>,
        state: R::State,
        recomposer: &mut R,
    ) -> Result<R::Piece, RecomposeError<R::Error>>
    where
        R: Recomposer<Latexlike, A>,
    {
        TreeRecomposer::new(recomposer).recompose(tree, state)
    }

    /// Parse strictly with `language`, assert a clean parse, and assert the
    /// reemission equals the input byte-for-byte.
    fn assert_reemit(language: &Language<Latexlike>, input: &str) {
        let result = language.parse(input).expect("test inputs parse");
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "unexpected: {:?}", result.diagnostics);
        let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn macros_reemit_their_recorded_spelling() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        assert_reemit(&language, "\\emph  x");
        assert_reemit(&language, "\\emph{x}");
        assert_reemit(&language, "pre \\emph post");
    }

    #[test]
    fn arguments_reemit_incl_optional_and_single_token_shapes() {
        let mut package = Package::new("t");
        package.insert(
            CallableType::Macro,
            "frac",
            MacroSpec::new(argument_specs(&["m", "m"]).unwrap()),
        );
        package.insert(
            CallableType::Macro,
            "o",
            MacroSpec::new(argument_specs(&["[", "{"]).unwrap()),
        );
        let language = with_package(Recovery::Strict, package);
        assert_reemit(&language, "\\frac 1 2");
        assert_reemit(&language, "\\frac{a}{b}");
        assert_reemit(&language, "\\o[x]{y}");
        assert_reemit(&language, "\\o{y} after"); // absent optional
        assert_reemit(&language, "\\o [x] {y}"); // inter-argument noise
    }

    fn env_language(recovery: Recovery) -> Language<Latexlike> {
        let mut package = Package::new("t");
        package.insert(CallableType::Environment, "itemize", EnvironmentSpec::new(vec![]));
        package.insert(
            CallableType::Environment,
            "verbatim",
            EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
        );
        with_package(recovery, package)
    }

    #[test]
    fn environments_reemit_both_sides_from_the_record() {
        let language = env_language(Recovery::Strict);
        assert_reemit(&language, "\\begin{itemize}x\\end{itemize}");
        // The tolerated `\begin {itemize}` spacing is recorded, not
        // normalized — reemission reproduces it.
        assert_reemit(&language, "\\begin {itemize}x y\\end {itemize}");
        assert_reemit(
            &language,
            "\\begin{itemize}a\\begin{itemize}b\\end{itemize}c\\end{itemize}",
        );
    }

    #[test]
    fn verbatim_reemits_every_byte() {
        let mut package = Package::new("t");
        package.insert(
            CallableType::Environment,
            "verbatim",
            EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
        );
        package.insert(
            CallableType::Macro,
            "verb",
            MacroSpec::new(argument_specs(&["v"]).unwrap()),
        );
        let language = with_package(Recovery::Strict, package);
        assert_reemit(&language, "\\begin{verbatim}\na % b {\n\\end{verbatim}");
        assert_reemit(&language, "\\verb|a b|");
    }

    #[test]
    fn specials_and_paragraph_breaks_reemit_name_as_written() {
        // The seed's base specials (ligatures, ~).
        assert_reemit(&strict(), "a---b ~ c");

        // Paragraph breaks, both driver styles: the Chars shape is a plain
        // whitespace node; the Specials shape records the actual run as name.
        assert_reemit(&strict(), "a\n\n  b");
        let language: Language<Latexlike> = Language::new(
            LatexlikeDriver::new(Recovery::Strict)
                .with_paragraph_break_style(ParagraphBreakStyle::Specials),
            ParsingState::lang_initial().expect("seed state"),
        );
        assert_reemit(&language, "a\n \t\nb");
    }

    #[test]
    fn math_groups_and_comments_reemit() {
        assert_reemit(&strict(), "inline $x+y$ math");
        assert_reemit(&strict(), "\\[d^2\\] and $$s$$ and \\(i\\)");
        assert_reemit(&strict(), "a% note\n  b");
        assert_reemit(&strict(), "{nested {groups} here}");
    }

    #[test]
    fn tolerant_recovery_shapes_reemit_what_was_recorded() {
        // Unterminated environment: the end side is empty, write_end emits
        // nothing — and the input had no terminator: reemit == input.
        let language = env_language(Recovery::Tolerant);
        let result = language.parse("\\begin{itemize}x").unwrap();
        assert!(!result.diagnostics.is_empty());
        let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
        assert_eq!(out, "\\begin{itemize}x");
    }

    #[test]
    fn a_materialized_tree_reemits_source_free() {
        let language = env_language(Recovery::Strict);
        let input = "\\begin {itemize}x\\end{itemize} $m$ \\begin{verbatim}r\\end{verbatim}";
        let result = language.parse(input).unwrap();
        let owned = result.tree.materialize();
        // Every payload is owned now; the writers resolve with no source
        // content at all (source-independent byte-faithful reconstruction).
        let out = recompose(&owned, (), &mut source_recomposer()).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn an_incoherent_payload_is_a_diagnosed_error() {
        // Hand-build a macro-typed callable carrying the Specials arm (a
        // parse never produces this).
        let source: Arc<Source> = Arc::new(Source::new("\\emph x"));
        let state = Arc::new(ParsingState::<Latexlike>::lang_initial().expect("seed state"));
        let mut builder: NodeTreeBuilder<Latexlike> = NodeTreeBuilder::new();
        let node = builder
            .add(
                NodeKind::callable(CallableData {
                    callable_type: CallableType::Macro,
                    name: "emph".into(),
                    spec: Arc::new(MacroSpec::default()),
                    arguments: ParsedArguments::empty(),
                    slots: ParsedSlots::empty(),
                    invocation_syntax: InvocationSyntaxData::Specials,
                }),
                SourceSpan::new(&source, 0..5),
                Arc::clone(&state),
                Vec::<BuildId>::new(),
                (),
                (),
            )
            .unwrap();
        let root = builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&source, 0..5),
                Arc::clone(&state),
                vec![node],
                (),
                (),
            )
            .unwrap();
        let tree: NodeTree<Latexlike> = builder.finish(root).unwrap();

        let error = recompose(&tree, (), &mut source_recomposer()).unwrap_err();
        match error {
            RecomposeError::Recomposer(SourceRecomposeError::IncoherentInvocationSyntax {
                callable_form,
                payload_arm,
                ..
            }) => {
                assert_eq!((callable_form, payload_arm), ("macro", "specials"));
            }
            other => panic!("expected the coherence error, got {other:?}"),
        }
    }

    #[test]
    fn specials_with_arguments_reemit_the_argument_regions() {
        // A specials spec with a mandatory brace argument: the name is the
        // head, the argument region follows as children.
        let mut package = Package::new("t");
        let mut spec = SpecialsSpec::<Latexlike>::default();
        spec.arguments = argument_specs(&["{"]).unwrap();
        package.insert(CallableType::Specials, "!!", spec);
        let language = with_package(Recovery::Strict, package);
        assert_reemit(&language, "a!!{b}c");
    }

    #[test]
    fn the_attached_slot_is_skipped_by_default_and_widens_on_opt_in() {
        use crate::latexlike::{input_macro_spec, BodyMarker};
        use crate::source::MapResolver;

        let mut package = Package::new("inputs");
        package.insert(
            CallableType::Macro,
            "input",
            input_macro_spec(false, BodyMarker::not_body()),
        );
        let mut resolver = MapResolver::new();
        resolver.insert("sub.tex", "SUB");
        let language: Language<Latexlike> = Language::new(
            LatexlikeDriver::new(Recovery::Strict)
                .with_source_resolver(resolver.with_reference_as_origin()),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let input = "a\\input{sub.tex}b";
        let result = language.parse(input).unwrap();
        assert!(result.diagnostics.is_empty(), "unexpected: {:?}", result.diagnostics);

        // Default scope: the attached content does not reemit — the
        // invocation text is its recomposition; reemit == the includer.
        let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
        assert_eq!(out, input);

        // Widening (a wrapper overriding the \input node): the attached
        // region participates — the included bytes appear in place of
        // nothing having been emitted for them before.
        struct Widened {
            inner: SourceRecomposer<Latexlike>,
        }
        impl<A> Recomposer<Latexlike, A> for Widened {
            type State = ();
            type Piece = String;
            type Error = SourceRecomposeError;

            fn recompose_node(
                &mut self,
                node: NodeRef<'_, Latexlike, A>,
                state: &(),
                cx: &mut RecomposeContext<'_, Latexlike, A>,
            ) -> Result<Recompose<String, ()>, SourceRecomposeError> {
                match self.inner.recompose_node(node, state, cx)? {
                    Recompose::Concat(pieces) if node.name() == Some("input") => {
                        Ok(Recompose::Concat(pieces.include_attached()))
                    }
                    other => Ok(other),
                }
            }
        }
        let out = recompose(
            &result.tree,
            (),
            &mut Widened { inner: source_recomposer() },
        )
        .unwrap();
        assert_eq!(out, "a\\input{sub.tex}SUBb");
    }
}
