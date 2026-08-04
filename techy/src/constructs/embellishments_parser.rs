//! [`EmbellishmentsArgumentParser`]: the embellishment-arguments parser
//! (ParserLibraryParity.md N3; pylatexenc's `LatexOptionalEmbellishmentArgsParser`,
//! xparse's `e{<chars>}` argument type, the preset's `e{…}` code).
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
//! - **Noise before a marker; only whitespace before an argument** (decided July
//!   2026, whitespace allowance revised the same month): whitespace and comments are
//!   scanned ahead of each marker (region noise, like every argument), and between a
//!   marker and its expression **plain whitespace** is tolerated (pylatexenc's
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
//! - Markers are sequences of [`Char`](TokenKind::Char) tokens: a character claimed by
//!   another recognizer in the argument's state (a specials trigger, a group open)
//!   does not match. State-dependent tokenization is the law of the land — e.g. the
//!   latexlike `''` typography ligature outranks a `'` marker in text mode but not in
//!   math mode, where the ligature is not visible.
//!
//! Absent — no marker matches at the position — is a silent, valid outcome
//! (`can_match_empty` = `true`), consuming nothing.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::node::{ArgumentExt, ContentNodes, GroupData, NodeKind};
use crate::source::{SourceSpan, Span, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::Lang;
use crate::token::TokenKind;

use super::argument_parsers::{
    parse_expression_node, scan_argument_noise, stage_pre_space,
};
use super::{ConstructParserResult, FromInvocation, ParseContext};

/// The embellishment-arguments parser (see the module docs): marker alternatives, each
/// followed by one expression (at most plain whitespace apart), repeating in any order
/// until no available marker matches; each marker at most once.
pub struct EmbellishmentsArgumentParser {
    markers: Vec<Box<str>>,
}

impl EmbellishmentsArgumentParser {
    /// An embellishments argument over the given markers (e.g. `["^", "_"]`; xparse's
    /// `e{^_}`). Markers must be non-empty; the list must be non-empty. Multi-character
    /// markers are allowed and win over shorter alternatives at the same position
    /// (longest match).
    pub fn new(
        markers: impl IntoIterator<Item = impl Into<Box<str>>>,
    ) -> EmbellishmentsArgumentParser {
        let markers: Vec<Box<str>> = markers.into_iter().map(Into::into).collect();
        debug_assert!(!markers.is_empty(), "an embellishments argument needs markers");
        debug_assert!(
            markers.iter().all(|marker| !marker.is_empty()),
            "embellishment markers need at least one character"
        );
        EmbellishmentsArgumentParser { markers }
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
        let mut nodes = Vec::new();
        let mut used = alloc::vec![false; self.markers.len()];
        let mut content_start: Option<u32> = None;

        loop {
            // Noise is allowed ahead of each marker; a failed probe rewinds exactly
            // this scan, keeping previously committed pairs.
            let noise = scan_argument_noise(cx)?;
            let Some(first) = noise.next.clone() else {
                noise.rewind(cx);
                break;
            };

            let Some((marker_index, marker_span)) =
                self.scan_marker(cx, &first, &used)?
            else {
                noise.rewind(cx);
                break;
            };

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
                cx.tokens.move_to_pos(noise.start);
                break;
            }

            // Committed: the wrapper group over the expression's nodes, the marker as
            // its open delimiter (pylatexenc's `(marker, '')` shape).
            let end = wrapper_children
                .last()
                .and_then(|last| cx.staged_nodes().get(*last))
                .map(|child| child.span().end())
                .unwrap_or(marker_span.end());
            let span = Span::new(marker_span.start(), end);
            let data = GroupData::untyped(
                TextContent::Spanned(marker_span),
                TextContent::empty(),
            );
            let wrapper = cx.stage_node(
                    NodeKind::group(data),
                    SourceSpan::new(&cx.source, span),
                    Arc::clone(&cx.state),
                    wrapper_children,
                )
                .map_err(|error| cx.implementation_error(error, span))?;

            used[marker_index] = true;
            nodes.extend(noise.nodes);
            stage_pre_space(cx, &mut nodes, first.pre_space)?;
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

impl EmbellishmentsArgumentParser {
    /// Scan one marker starting at `first` (the noise scan's unconsumed stop token):
    /// consume contiguous `Char` tokens while they prefix a still-available marker,
    /// and settle on the **longest** fully matched one. On a match, the reader stands
    /// at the marker's end and `Some((marker index, marker span))` is returned; on a
    /// miss the reader is *not* rewound (the caller owns the rewind target).
    fn scan_marker<'s, L: Lang>(
        &self,
        cx: &mut ParseContext<'_, 's, L>,
        first: &crate::token::Token<'s, L>,
        used: &[bool],
    ) -> ConstructParserResult<L, Option<(usize, Span)>> {
        let TokenKind::Char(first_char) = &first.kind else { return Ok(None) };

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
        run.push(*first_char);
        if !prefixes_any(&run) {
            return Ok(None);
        }
        cx.tokens.move_past(first, true);
        let mut end = first.span.end();
        let mut best: Option<(usize, usize)> = available(&run).map(|index| (index, end));

        let state = Arc::clone(&cx.state);
        loop {
            // Accumulate while the run still prefixes an available marker; each
            // continuation char must be contiguous (no pre-space).
            let Some(token) = cx.probe_token(&state)? else { break };
            let TokenKind::Char(c) = &token.kind else { break };
            if !token.pre_space.is_empty() {
                break;
            }
            run.push(*c);
            if !prefixes_any(&run) {
                break;
            }
            cx.tokens.move_past(&token, true);
            end = token.span.end();
            if let Some(index) = available(&run) {
                best = Some((index, end));
            }
        }

        let Some((index, match_end)) = best else { return Ok(None) };
        // The scan may have read past the settled match (a longer alternative that
        // never completed): stand the reader at the marker's end.
        cx.tokens.move_to_pos(match_end);
        Ok(Some((index, Span::new(first.span.start(), match_end))))
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
    use crate::latexlike::{CallableType, Latexlike, MacroSpec};
    use crate::extract::{content_as_chars, split_embellishments_drop_annotations};
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use crate::error::Recovery;
    use crate::latexlike::LatexlikeDriver;
    use alloc::string::ToString;
    use alloc::vec;

    /// A language defining `\m` with one embellishments argument over `markers`, and
    /// a zero-argument `\alpha`.
    fn language(markers: &[&str]) -> Language<Latexlike> {
        let parser = EmbellishmentsArgumentParser::new(markers.iter().copied());
        let mut package = Package::new("embellishment-tests");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(vec![Arc::new(ArgumentSpec::new_unnamed(Arc::new(parser)))])),
        );
        package.insert(CallableType::Macro, "alpha", Arc::new(MacroSpec::new(vec![])));
        Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]),
        )
    }

    fn parse(markers: &[&str], input: &str) -> crate::engine::ParseResult<Latexlike> {
        let result = language(markers).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
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
}
