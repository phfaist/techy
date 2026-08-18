//! The preset's source re-emission: [`SourceRecomposer`] (constructor
//! [`source_recomposer`]) — the ONE recomposer implementation that
//! reconstructs a latexlike-family tree's source spelling from its recorded
//! facts. (Internal file note: this module is private; the public story
//! lives on the items' own docs.)

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

/// The preset's source reemitter: reconstructs a latexlike-family tree's
/// **source spelling from its recorded facts** —
/// `TreeRecomposer::new(&mut source_recomposer()).recompose(&tree, ())` reemits
/// the tree.
///
/// Everything it emits comes from node payload: the core-complete kinds
/// through [`core_source_instruction`] (chars content, comment parts, group
/// delimiters), and callables from the family's recorded invocation-syntax
/// payload — the macro escape character + name + post-space, the environment
/// sides through the record's own spelling writers
/// ([`write_begin`](EnvironmentSyntax::write_begin)/[`write_end`](EnvironmentSyntax::write_end)),
/// and the specials name-as-written. No span content is ever resolved (the
/// per-node reading contract, [`techy::recompose`](crate::recompose)); on a
/// [`materialize`](crate::core::node::NodeTree::materialize)d tree the
/// reconstruction touches no `Source` at all.
///
/// **Accuracy is what the parse records** (the preset's accuracy rule,
/// [`CallableData::invocation_syntax`](crate::node::CallableData::invocation_syntax)):
/// for trees the latexlike parse produces, reemission is byte-exact —
/// including tolerant-recovery shapes, which reemit exactly what was
/// recorded (an environment that never found its terminator has an empty end
/// side and reemits no terminator). The one recorded-less-than-consumed
/// recovery is the malformed environment terminator (its `\end` is consumed
/// alone, diagnosed, and recorded nowhere): that consumed command spelling
/// is not reproduced.
///
/// Byte-exactness is byte-equality with the parsed source, so it is stated for a
/// language that obeys span tiling
/// ([`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)) — the
/// preset's own [`Latexlike`] included. A family member declaring
/// `OBEYS_SPAN_TILING = false` — whose parsers assume nothing about where the
/// tokens come from — has its trees reemitted **as stored**: the owned text the
/// parser recorded, in tree order. No byte-equality with any one source is claimed
/// for such a tree.
///
/// A [`Recomposer`] with `State = ()`, `Piece = String`, instruction-only —
/// it never descends explicitly, so it composes correctly under a wrapping
/// recomposer (the wrapping contract), and it needs no scope call at all:
/// the default `Concat` scope already skips `Attached` and `Hidden` slot
/// children (an `\input`'s attached content reemits as the `\input{…}`
/// invocation it came from). Works for any language family member (`LLL:`
/// [`LatexlikeLang`]) over trees with any annotation type.
///
/// Its only failure is [`SourceRecomposeError`]'s payload-coherence check,
/// which **no parse output can trigger**
/// ([`IncoherentInvocationSyntax`](SourceRecomposeError::IncoherentInvocationSyntax)
/// reports a hand-built or incoherently restaged tree) — over trees a parse
/// produced, the recomposition never errs.
pub struct SourceRecomposer<LLL: LatexlikeLang = Latexlike> {
    _lang: PhantomData<LLL>,
}

/// The [`SourceRecomposer`] constructor:
/// `TreeRecomposer::new(&mut source_recomposer()).recompose(&tree, ())` reemits
/// `tree`'s source spelling.
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

/// Error of a [`SourceRecomposer`] run.
///
/// Variant/`callable_type` coherence is deliberately **unenforced at parse
/// time** (an accepted cost of the Lang-owned payload channel) — the source
/// recomposer is where an incoherent combination surfaces, as
/// [`IncoherentInvocationSyntax`](SourceRecomposeError::IncoherentInvocationSyntax).
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
