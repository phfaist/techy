//! [`LatexlikeDriver`]: the preset's [`ParseDriver`] — recovery policy, scope-stack
//! command resolution, the math-mode group plug, and the paragraph-break emission
//! style.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{CommandResolution, ParseDriver};
use crate::error::Recovery;
use crate::node::{CallableData, NodeKind, ParsedArguments, ParsedSlots};
use crate::source::TextContent;
use crate::state::{ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::{GroupRule, Token};

use super::{CallableType, GroupType, Latexlike, Mode, SpecialsSpec};

/// How [`LatexlikeDriver`] emits the node for a paragraph-break token (a whitespace
/// run containing two or more newlines,
/// [`TokenRules::enable_multi_newline_paragraphs`](crate::token::TokenRules::enable_multi_newline_paragraphs)).
///
/// This is a **driver emission policy**, deliberately not scope-stack data (decided
/// with the 7.9 acceptance work): the tokenizer detects paragraph breaks within
/// leading whitespace, *before* the specials scan ever runs, so a package-registered
/// `"\n\n"` specials entry could never fire — correlating the node shape with package
/// contents would be dead configuration. The flag is driver-global; per-scope
/// suppression stays orthogonal (a state delta clearing the
/// `enable_multi_newline_paragraphs` gate, as verbatim's features-off state does).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParagraphBreakStyle {
    /// A whitespace-only `Chars` node over the break's extent — the core default
    /// shape (and pylatexenc-legacy's), friendly to text extraction
    /// ([`content_as_chars`](crate::node::extract::content_as_chars) folds it into
    /// text).
    #[default]
    Chars,
    /// A [`Specials`](CallableType::Specials)-formed `Callable` node —
    /// pylatexenc-modern's paragraph-break shape. The node's *name* is the canonical
    /// `"\n\n"` (the vocabulary key, like `"~"` or `"---"`), whatever the actual
    /// whitespace run looked like; the node's *span* covers the actual run. The
    /// token level is unchanged (still
    /// [`ParagraphBreak`](crate::token::TokenKind::ParagraphBreak)), and the spec
    /// stamped on the node is a fresh argument-less [`SpecialsSpec`] — it lives on
    /// no provider, so `"\n\n"` does **not** appear in
    /// [`iter_symbols`](crate::scopes::ScopeStack::iter_symbols) enumerations.
    /// Extraction helpers treat the node as the non-text material it now is
    /// (`content_as_chars` reports it instead of folding it into text).
    Specials,
}

/// The preset's parse-behavior object ([`Lang::Driver`](crate::state::Lang::Driver)):
/// carries the tolerant-parsing policy, resolves command tokens through the state's
/// scope stack (as [`Macro`](CallableType::Macro)s — `\begin`/`\end` resolve like any
/// other command to the [`base_package`](super::base_package)'s dispatch entries),
/// plugs [`Math`](GroupType::Math) group interiors into [`Mode::Math`] through
/// the descent-delta channel, and emits paragraph-break nodes per its
/// [`ParagraphBreakStyle`].
///
/// Construct-provision and the remaining hooks keep their trait defaults; preset
/// helper methods (e.g. package loading by name) arrive with the standard spec
/// database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatexlikeDriver {
    /// The tolerant-parsing policy to drive under (default: [`Recovery::Strict`]).
    pub recovery: Recovery,
    /// How paragraph-break tokens become nodes (default:
    /// [`ParagraphBreakStyle::Chars`]).
    pub paragraph_break_style: ParagraphBreakStyle,
}

impl LatexlikeDriver {
    /// A driver with the given recovery policy (and the default
    /// [`ParagraphBreakStyle::Chars`]).
    pub fn new(recovery: Recovery) -> LatexlikeDriver {
        LatexlikeDriver { recovery, paragraph_break_style: ParagraphBreakStyle::default() }
    }

    /// Emit paragraph-break nodes in the given style.
    pub fn with_paragraph_break_style(mut self, style: ParagraphBreakStyle) -> LatexlikeDriver {
        self.paragraph_break_style = style;
        self
    }
}

impl Default for LatexlikeDriver {
    fn default() -> Self {
        LatexlikeDriver::new(Recovery::Strict)
    }
}

impl ParseDriver<Latexlike> for LatexlikeDriver {
    fn recovery(&self) -> Recovery {
        self.recovery
    }

    /// Resolve a command token as a [`Macro`](CallableType::Macro) through the state's
    /// scope stack, via the shared [`CommandResolution::resolve_via_scopes`]: a hit
    /// dispatches; a clean miss reports the searched providers as the
    /// unresolvable-command detail; an operational provider failure is a distinct
    /// [`Failed`](CommandResolution::Failed) resolution.
    fn resolve_command(
        &self,
        state: &ParsingState<Latexlike>,
        token: &Token<'_, Latexlike>,
    ) -> CommandResolution<Latexlike> {
        CommandResolution::resolve_via_scopes(state, token, CallableType::Macro)
    }

    /// Emit the paragraph-break node per the driver's
    /// [`paragraph_break_style`](LatexlikeDriver::paragraph_break_style): the core
    /// default whitespace `Chars` shape, or a `Specials`-formed `Callable` named
    /// `"\n\n"` (see [`ParagraphBreakStyle::Specials`] for the exact contract).
    fn make_paragraph_break_node(
        &self,
        _state: &ParsingState<Latexlike>,
        token: &Token<'_, Latexlike>,
    ) -> NodeKind<Latexlike> {
        match self.paragraph_break_style {
            ParagraphBreakStyle::Chars => NodeKind::chars(token.span),
            // The spec is minted per break rather than cached on the driver: caching
            // an `Arc` would cost the driver its `Copy`/`Eq` config-value nature for
            // a negligible allocation (specs are behavior, never compared).
            ParagraphBreakStyle::Specials => NodeKind::callable(CallableData {
                callable_type: CallableType::Specials,
                name: "\n\n".into(),
                spec: Arc::new(SpecialsSpec::default()),
                arguments: ParsedArguments::empty(),
                slots: ParsedSlots::empty(),
                post_space: TextContent::empty(),
                ext: Default::default(),
            }),
        }
    }

    /// The math plug (DESIGN_RATIONALE.md [§dd-dr:parsing-state]/[§dd-dr:parsers-engine]): a math group's interior parses in
    /// [`Mode::Math`], and — since LaTeX forbids nested math — the math delimiters stop
    /// being *openers* inside it. Derived from the **outer** `base` state (not the seed):
    /// the interior's group rules are `base`'s minus the [`Math`](GroupType::Math)
    /// openers (the descent invariant still installs `expecting_group_close` for the
    /// current group's close), and the bare `$` trigger is merged into `base`'s existing
    /// [`forbidden_chars`](crate::token::TokenRules::forbidden_chars) so a stray `$` is a
    /// diagnostic rather than the opener of an unclosed nested group. Pure in
    /// `(base, rule)` per the memoization contract.
    fn group_interior_delta(
        &self,
        base: &ParsingState<Latexlike>,
        rule: &Arc<GroupRule<Latexlike>>,
    ) -> Option<ParsingStateDelta<Latexlike>> {
        match rule.group_type {
            GroupType::Math => {
                let rules = base.rules();
                // Drop the math openers; keep everything else (content groups, temporary
                // groups, commands, …) exactly as the outer state has it.
                let groups: Vec<Arc<GroupRule<Latexlike>>> = rules
                    .groups
                    .iter()
                    .filter(|group_rule| group_rule.group_type != GroupType::Math)
                    .cloned()
                    .collect();
                // Merge `$` into the *current* forbidden chars (at the transition), not a
                // fresh set — an embedder's forbidden chars must survive into math.
                let mut forbidden = String::from(&*rules.forbidden_chars);
                if !forbidden.contains('$') {
                    forbidden.push('$');
                }
                Some(
                    ParsingStateDelta::new().mode(Mode::Math).rules(TokenRulesOverrides {
                        groups: Some(groups),
                        forbidden_chars: Some(forbidden.into()),
                        ..TokenRulesOverrides::default()
                    }),
                )
            }
            // Verbatim rules never reach a tokenizer descent (the class marks raw
            // regions and minted terminator rules, `GroupType::Verbatim` docs) — the
            // arm exists for match exhaustiveness only.
            GroupType::Content | GroupType::Verbatim => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_driver_is_strict() {
        assert_eq!(LatexlikeDriver::default().recovery, Recovery::Strict);
        assert_eq!(LatexlikeDriver::new(Recovery::Tolerant).recovery, Recovery::Tolerant);
    }

    #[test]
    fn the_default_paragraph_break_style_is_chars() {
        assert_eq!(
            LatexlikeDriver::default().paragraph_break_style,
            ParagraphBreakStyle::Chars
        );
        assert_eq!(
            LatexlikeDriver::default()
                .with_paragraph_break_style(ParagraphBreakStyle::Specials)
                .paragraph_break_style,
            ParagraphBreakStyle::Specials
        );
    }

    #[test]
    fn math_rules_enter_math_mode_content_rules_do_not() {
        let driver = LatexlikeDriver::default();
        let state = ParsingState::<Latexlike>::initial();

        let math = Arc::new(GroupRule {
            group_type: GroupType::Math,
            open: "$".into(),
            close: "$".into(),
        });
        let delta = driver.group_interior_delta(&state, &math).unwrap();
        let derived = state.derived(&delta).unwrap();
        assert_eq!(derived.mode(), Mode::Math);

        let content = Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "{".into(),
            close: "}".into(),
        });
        assert!(driver.group_interior_delta(&state, &content).is_none());
    }
}
