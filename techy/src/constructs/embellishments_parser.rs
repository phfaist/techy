//! [`EmbellishmentsArgumentParser`]: the embellishment-arguments parser
//! (pylatexenc's `LatexOptionalEmbellishmentArgsParser`, xparse's `e{<chars>}`
//! argument type, the preset's `e{…}` code).
//!
//! One argument position absorbing a run of *marker + expression* pairs: whenever one
//! of the configured markers (`^`, `_`, `'`, …) comes next, it must be followed —
//! at most plain whitespace apart — by one expression (a delimited group, a full
//! invocation, or a single char — the [`ExpressionParser`](super::ExpressionParser)
//! core), and the pair repeats until no marker matches. Each marker matches **at most
//! once** per invocation (xparse semantics; pylatexenc removes matched chars the same
//! way), in any source order — `\op^{a}_{b}` and `\op_{b}^{a}` both provide both
//! fields.
//!
//! # Record shape
//!
//! The whole run is **one argument** — per-marker `ParsedArgument` entries cannot
//! express free source order through sequential per-spec parsing — and each matched
//! pair stages one **classless wrapper `Group`** ([`GroupData::untyped`]): `open` =
//! the marker as written (span-backed), `close` empty, children = the expression's
//! node(s), preceded by a whitespace `Chars` node when whitespace separated marker
//! and expression (pylatexenc's `delimiters=(marker, '')` shape). The argument's content
//! designation covers the run of wrapper groups from the first one on — noise between
//! embellishments included, leading noise excluded. By-marker access is the read-side
//! helper [`split_embellishments`](crate::extract::split_embellishments).
//!
//! # Matching rules
//!
//! - **Noise before a marker; only whitespace before an expression**: whitespace and
//!   comments are scanned ahead of each marker (region noise, like every argument),
//!   and between a marker and its expression **plain whitespace** is tolerated
//!   (pylatexenc's
//!   `allow_pre_space` parity; TeX's `x^ 2`), staged *inside* the wrapper group as
//!   leading noise. Anything else — a comment, a paragraph break, end of input, a
//!   group close… — is **not a match**: the marker is rewound whole and the run
//!   ends, silently. Marker + expression stay atomic; a lone `^` is ordinary content
//!   of the enclosing level.
//! - **Longest match** among the still-available markers when several share a prefix
//!   (`'` vs. `''`) — a deliberate divergence from pylatexenc, whose accumulate-until-
//!   equal scan makes the shortest alternative win unconditionally. Multi-character
//!   markers must be contiguous (no intervening whitespace; the
//!   [`MarkerArgumentParser`](super::MarkerArgumentParser) precedent — no pylatexenc
//!   space-normalized accumulation).
//! - **Markers are matched against the raw characters**, under the state
//!   [`marker_scan_state_delta`] derives: a character the argument's state would read
//!   as a specials trigger, a group delimiter, a comment start or a command escape
//!   still matches a marker spelled with it — the markers are the spec's own syntax,
//!   like a verbatim delimiter, and a language's definitions do not switch them off.
//!   A `'` marker thus matches ahead of the latexlike `''` typography ligature:
//!   `\op''` is the marker `'` followed by the expression `'`. Only the marker is read
//!   raw; its expression parses under the argument's state, so a ligature or specials
//!   *inside* the expression (`^{a~b}`) is recognized as usual.
//!
//! Absent — no marker matches at the position — is a silent, valid outcome
//! (`can_match_empty` = `true`), consuming nothing.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::node::{ArgumentExt, ContentNodes, GroupData, NodeKind};
use crate::source::{SourceSpan, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::{Lang, ParsingState};
use crate::token::{Token, TokenEdge, TokenKind};

use super::argument_parsers::{
    marker_scan_state_delta, parse_expression_node, scan_argument_noise, stage_pre_space,
};
use super::{node_text_content, ConstructParserResult, FromInvocation, ParseContext};

/// An argument position that absorbs a run of *marker + expression* pairs, such as
/// the sub- and superscripts of `\op^{a}_{b}` (xparse's `e{<chars>}` argument type).
///
/// The engine reaches for it when a callable's spec declares an argument with this
/// parser. Whenever one of the configured markers (`^`, `_`, `'`, …) comes next, it
/// must be followed — at most plain whitespace apart — by one expression: a
/// delimited group, a full invocation, or a single character. The pair repeats until
/// no still-available marker matches, in any source order, and each marker matches
/// at most once per invocation. Among available markers sharing a prefix the longest
/// one wins. A marker not followed by an expression is rewound whole and ends the run
/// silently, staying ordinary content of the enclosing level.
///
/// Markers are matched against the raw characters of the source, under the state
/// [`marker_scan_state_delta`] derives: a character the argument's state would read as
/// a specials trigger or a group delimiter still matches a marker spelled with it. The
/// expression that follows a marker parses under the argument's own state.
///
/// The whole run is recorded as **one argument**. Each matched pair stages one
/// classless wrapper `Group`
/// ([`GroupData::untyped`](crate::core::node::GroupData::untyped)) whose opening delimiter
/// is the marker as written and whose children are the expression's nodes; read the
/// pairs back by marker with
/// [`split_embellishments`](crate::extract::split_embellishments).
pub struct EmbellishmentsArgumentParser {
    markers: Vec<Box<str>>,
}

impl EmbellishmentsArgumentParser {
    /// An embellishments argument over the given markers (e.g. `["^", "_"]`; xparse's
    /// `e{^_}`). Markers must be non-empty; the list must be non-empty (violations are
    /// reported as implementation errors when the parser runs). Multi-character
    /// markers are allowed and win over shorter alternatives at the same position
    /// (longest match).
    pub fn new(
        markers: impl IntoIterator<Item = impl Into<Box<str>>>,
    ) -> EmbellishmentsArgumentParser {
        EmbellishmentsArgumentParser {
            markers: markers.into_iter().map(Into::into).collect(),
        }
    }
}

impl<L: Lang> ArgumentParser<L> for EmbellishmentsArgumentParser
where
    ArgumentExt<L>: Default,
    L::InvocationSyntax: FromInvocation<L>,
{
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        // Spec-author contract, validated where the parser runs
        // ([§dd-dr:panic-policy]: an outer-layer contract violation is an `Err`).
        if self.markers.is_empty() {
            return Err(cx.implementation_error(
                "EmbellishmentsArgumentParser::new needs at least one marker",
                cx.here(),
            ));
        }
        if self.markers.iter().any(|marker| marker.is_empty()) {
            return Err(cx.implementation_error(
                "EmbellishmentsArgumentParser::new needs non-empty markers",
                cx.here(),
            ));
        }
        // Markers are matched against the raw characters (module docs): one scan state
        // per argument parse — a rules-only derivation the session memoizes.
        let scan_state = cx.derive_state(&marker_scan_state_delta())?;
        let mut nodes = Vec::new();
        let mut used = alloc::vec![false; self.markers.len()];
        let mut content_start: Option<u32> = None;

        loop {
            // Noise is allowed ahead of each marker; a failed probe rewinds exactly
            // this scan, keeping previously committed pairs. The scan's stop token is
            // not used: the marker scan re-reads the position under the scan state.
            let noise = scan_argument_noise(cx)?;

            let Some(marker) = self.scan_marker(cx, &scan_state, &used)? else {
                noise.rewind(cx);
                break;
            };
            let MarkerMatch { index: marker_index, first, span: marker_span } = marker;

            // The expression follows the marker at most plain whitespace apart: the
            // probed token may carry pre-space, which the expression core stages
            // *inside* the wrapper as leading noise on commit. Anything else — a
            // comment, a paragraph break, end of input, any token that cannot begin
            // an expression — unmatches the marker whole (`parse_expression_node`
            // answers `None` for all of these, staging and consuming nothing).
            let mut wrapper_children = Vec::new();
            let state = Arc::clone(&cx.state);
            let expression = match cx.probe_token(&state)? {
                Some(token) => parse_expression_node(cx, &token, &mut wrapper_children)?,
                None => None,
            };
            if expression.is_none() {
                // Marker + expression are atomic: rewind the marker (and this
                // iteration's noise) and end the run, silently.
                cx.tokens.move_to_position(&noise.start);
                break;
            }

            // Committed: the wrapper group over the expression's nodes, the marker as
            // its open delimiter (pylatexenc's `(marker, '')` shape). The wrapper runs
            // from the marker to the end of the expression's last staged node, when
            // that node lies in the marker's own source; to the marker's end
            // otherwise.
            let span = match L::OBEYS_SPAN_TILING {
                true => {
                    let child_span = wrapper_children
                        .last()
                        .and_then(|last| cx.staged_nodes().get(*last))
                        .map(|child| child.span().clone())
                        .filter(|child| {
                            child.same_source(&marker_span)
                                && child.end() >= marker_span.end()
                        });
                    match child_span {
                        Some(child) => SourceSpan::new(
                            marker_span.source(),
                            marker_span.start()..child.end(),
                        ),
                        None => marker_span.clone(),
                    }
                }
                // No assumption about the staged child: a language that does not obey
                // span tiling gives no relation between its span and the marker's, and
                // the child's end is not a stream position to measure the wrapper
                // with. The span is what the reader describes for the stretch from the
                // marker's start to where the stream now stands — just past the
                // expression the wrapper holds.
                false => cx.source_span_within(
                    &cx.tokens.position_at(&first, TokenEdge::Start),
                    &cx.tokens.position_here(),
                )?,
            };
            let data = GroupData::untyped(
                node_text_content(&marker_span, &span),
                TextContent::empty(),
            );
            let wrapper = cx
                .stage_node(
                    NodeKind::group(data),
                    span.clone(),
                    Arc::clone(&cx.state),
                    wrapper_children,
                )
                .map_err(|error| cx.staging_error(error, span))?;

            used[marker_index] = true;
            nodes.extend(noise.nodes);
            stage_pre_space(cx, &mut nodes, &first)?;
            if content_start.is_none() {
                content_start = Some(nodes.len() as u32);
            }
            nodes.push(wrapper);

            if used.iter().all(|used| *used) {
                break; // every marker matched once — nothing left to probe for
            }
        }

        match content_start {
            // The content: the full run from the first wrapper group on (intermediate
            // noise included, leading noise excluded — module docs).
            Some(start) => {
                let end = nodes.len() as u32;
                Ok(Some(ParsedArgumentNodes::new(
                    nodes,
                    ContentNodes::InRegion(start..end),
                    Default::default(),
                )))
            }
            // No pair matched; the reader is back at the entry position (the first
            // iteration's rewind).
            None => Ok(None),
        }
    }

    /// Optional: absent is a valid, silent outcome (the trait default, stated
    /// explicitly — the expression-position guard leans on this answer).
    fn can_match_empty(&self) -> bool {
        true
    }
}

/// A matched marker: which of the configured markers, the raw token its first
/// character was read as (its pre-space is the marker's pre-space), and the marker's
/// span.
struct MarkerMatch<L: Lang> {
    index: usize,
    first: Token<L>,
    span: SourceSpan<L::SourceOrigin>,
}

impl EmbellishmentsArgumentParser {
    /// Scan one marker at the reader's position, reading under `scan_state` (the
    /// marker-scan state, where every marker character is a plain `Char`): consume
    /// contiguous `Char` tokens while they prefix a still-available marker, and settle
    /// on the **longest** fully matched one. On a match, the reader stands at the
    /// marker's end; on a miss the reader is *not* rewound (the caller owns the rewind
    /// target).
    fn scan_marker<'s, L: Lang>(
        &self,
        cx: &mut ParseContext<'_, 's, L>,
        scan_state: &Arc<ParsingState<L>>,
        used: &[bool],
    ) -> ConstructParserResult<L, Option<MarkerMatch<L>>> {
        let Some(first) = cx.probe_token(scan_state)? else {
            return Ok(None);
        };
        let TokenKind::Char(first_char) = cx.tokens.token_kind(&first) else {
            return Ok(None);
        };

        let available = |candidate: &str| {
            self.markers
                .iter()
                .enumerate()
                .filter(|(index, _)| !used[*index])
                .find(|(_, marker)| &***marker == candidate)
                .map(|(index, _)| index)
        };
        let prefixes_any = |candidate: &str| {
            self.markers
                .iter()
                .enumerate()
                .any(|(index, marker)| !used[index] && marker.starts_with(candidate))
        };

        let mut run = String::new();
        run.push(first_char);
        if !prefixes_any(&run) {
            return Ok(None);
        }
        cx.tokens.move_to(&first, TokenEdge::EndPastPostSpace);
        // The best-so-far match is kept as the *token* it ends on: only the reader can
        // say where that is.
        let mut best: Option<(usize, Token<L>)> =
            available(&run).map(|index| (index, first.clone()));

        loop {
            // Accumulate while the run still prefixes an available marker; each
            // continuation char must be contiguous (no pre-space).
            let Some(token) = cx.probe_token(scan_state)? else { break };
            let TokenKind::Char(c) = cx.tokens.token_kind(&token) else { break };
            if cx.tokens.position_at(&token, TokenEdge::StartBeforePreSpace)
                != cx.tokens.position_at(&token, TokenEdge::Start)
            {
                break;
            }
            run.push(c);
            if !prefixes_any(&run) {
                break;
            }
            cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
            if let Some(index) = available(&run) {
                best = Some((index, token.clone()));
            }
        }

        let Some((index, last)) = best else { return Ok(None) };
        // The scan may have read past the settled match (a longer alternative that
        // never completed): stand the reader at the marker's end.
        cx.tokens.move_to(&last, TokenEdge::EndPastPostSpace);
        let span = cx.source_span_within(
            &cx.tokens.position_at(&first, TokenEdge::Start),
            &cx.tokens.position_at(&last, TokenEdge::EndPastPostSpace),
        )?;
        Ok(Some(MarkerMatch { index, first, span }))
    }
}

impl fmt::Debug for EmbellishmentsArgumentParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbellishmentsArgumentParser")
            .field("markers", &self.markers)
            .finish()
    }
}

// The `e{…}`-code slice of the behavior (single-char markers, pylatexenc test ports)
// is covered in `latexlike::arguments`; here the marker-generality axes the code
// cannot spell — multi-character markers, longest match, expression forms.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Language;
    use crate::latexlike::{CallableType, Latexlike, MacroSpec, SpecialsSpec};
    use crate::extract::{content_as_chars, split_embellishments_drop_annotations};
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::latexlike::LatexlikeDriver;
    use alloc::string::ToString;
    use alloc::vec;

    /// A language defining `\m` with one embellishments argument over `markers`, a
    /// zero-argument `\alpha`, and a zero-argument specials for each of `specials`.
    fn language_with_specials(markers: &[&str], specials: &[&str]) -> Language<Latexlike> {
        let parser = EmbellishmentsArgumentParser::new(markers.iter().copied());
        let mut package = Package::new("embellishment-tests");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(vec![Arc::new(ArgumentSpec::new_unnamed(Arc::new(parser)))])),
        );
        package.insert(CallableType::Macro, "alpha", Arc::new(MacroSpec::new(vec![])));
        for trigger in specials {
            package.insert_specials(
                CallableType::Specials,
                *trigger,
                Arc::new(SpecialsSpec::new(vec![])),
            );
        }
        Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        )
    }

    /// [`language_with_specials`] with no specials.
    fn language(markers: &[&str]) -> Language<Latexlike> {
        language_with_specials(markers, &[])
    }

    fn parse_with_specials(
        markers: &[&str],
        specials: &[&str],
        input: &str,
    ) -> crate::engine::ParseResult<Latexlike> {
        let result = language_with_specials(markers, specials).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn parse(markers: &[&str], input: &str) -> crate::engine::ParseResult<Latexlike> {
        parse_with_specials(markers, &[], input)
    }

    fn keys(m: NodeRef<'_, Latexlike>) -> Vec<alloc::string::String> {
        split_embellishments_drop_annotations(m.argument_content_nodes(0).expect("provided"))
            .unwrap()
            .iter()
            .map(|entry| entry.key().to_string())
            .collect()
    }

    #[test]
    fn multi_char_markers_take_the_longest_available_match() {
        let result = parse(&["a", "ab"], r"\m ab{x}a{y}!");
        let m = result.tree.root().child(0).unwrap();
        assert_eq!(keys(m), ["ab", "a"]);
        let fields = split_embellishments_drop_annotations(m.argument_content_nodes(0).unwrap()).unwrap();
        assert_eq!(
            content_as_chars(fields.get("ab").unwrap().value_content().unwrap()).unwrap(),
            "x"
        );
        assert_eq!(
            content_as_chars(fields.get("a").unwrap().value_content().unwrap()).unwrap(),
            "y"
        );
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("!"));
    }

    #[test]
    fn a_longer_candidate_that_never_completes_falls_back_to_the_shorter_match() {
        // Markers `a` and `abc` on `ab{x}`: the scan reads `ab` chasing `abc`, settles
        // on the full match `a`, and the expression is then the immediately following
        // single char `b` — `{x}` stays enclosing content.
        let result = parse(&["a", "abc"], r"\m ab{x}");
        let m = result.tree.root().child(0).unwrap();
        assert_eq!(keys(m), ["a"]);
        let fields = split_embellishments_drop_annotations(m.argument_content_nodes(0).unwrap()).unwrap();
        assert_eq!(
            content_as_chars(fields.get("a").unwrap().value().unwrap()).unwrap(),
            "b"
        );
        assert!(result.tree.root().child(1).unwrap().is_group());
    }

    #[test]
    fn multi_char_markers_must_be_contiguous() {
        // `a b` is not the marker `ab`: nothing matches, the argument is silently
        // absent, and everything stays enclosing content.
        let result = parse(&["ab"], r"\m a b{x}");
        let m = result.tree.root().child(0).unwrap();
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("a b"));
    }

    #[test]
    fn the_expression_takes_every_expression_form() {
        // A full invocation…
        let result = parse(&["^"], r"\m^\alpha rest");
        let m = result.tree.root().child(0).unwrap();
        let wrapper = m.argument_content_nodes(0).unwrap().first().unwrap();
        assert_eq!(wrapper.group_delimiters(), Some(("^", "")));
        assert!(wrapper.child(0).unwrap().is_callable());

        // …and a single character.
        let result = parse(&["^"], r"\m^2 rest");
        let m = result.tree.root().child(0).unwrap();
        let wrapper = m.argument_content_nodes(0).unwrap().first().unwrap();
        assert_eq!(wrapper.child(0).unwrap().chars(), Some("2"));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" rest"));
    }

    #[test]
    fn markers_are_matched_raw_ahead_of_specials_and_the_expression_is_not() {
        // `` ` `` is a specials trigger in the argument's state and still matches the
        // `` ` `` marker (the marker-scan state reads it raw); inside the `^`
        // expression, which parses under the argument's state, the same character is
        // the specials invocation.
        let result = parse_with_specials(&["`", "^"], &["`"], "\\m`{x}^{a`b} `!");
        let m = result.tree.root().child(0).unwrap();
        assert_eq!(keys(m), ["`", "^"]);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content[0].group_delimiters(), Some(("`", "")));
        let sup_value = content[1].child(0).unwrap();
        assert_eq!(sup_value.child(0).unwrap().chars(), Some("a"));
        assert_eq!(sup_value.child(1).unwrap().specials_name(), Some("`"));
        assert_eq!(sup_value.child(2).unwrap().chars(), Some("b"));
        // After the run, the enclosing content reads `` ` `` as the specials again.
        let root = result.tree.root();
        assert!((0..root.child_count())
            .any(|i| root.child(i).unwrap().specials_name() == Some("`")));
    }

    // --- spec-author contract violations are implementation errors ([§dd-dr:panic-policy]) --

    #[test]
    fn an_empty_marker_list_is_an_implementation_error() {
        let err = language(&[]).parse(r"\m x").unwrap_err();
        assert_eq!(err.identifier(), super::super::ImplementationError::IDENTIFIER);
    }

    #[test]
    fn an_empty_marker_is_an_implementation_error() {
        let err = language(&["^", ""]).parse(r"\m^2").unwrap_err();
        assert_eq!(err.identifier(), super::super::ImplementationError::IDENTIFIER);
    }
}
