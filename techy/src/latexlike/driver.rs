//! [`LatexlikeDriver`]: the preset's [`ParseDriver`] — recovery policy, scope-stack
//! command resolution, and the math-mode group plug.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{CommandResolution, ParseDriver};
use crate::error::Recovery;
use crate::state::{ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::{GroupRule, Token};

use super::{CallableType, GroupType, Latexlike, Mode};

/// The preset's parse-behavior object ([`Lang::Driver`](crate::state::Lang::Driver)):
/// carries the tolerant-parsing policy, resolves command tokens through the state's
/// scope stack (as [`Macro`](CallableType::Macro)s — `\begin`/`\end` resolve like any
/// other command to the [`base_package`](super::base_package)'s dispatch entries),
/// and plugs [`Math`](GroupType::Math) group interiors into [`Mode::Math`] through
/// the descent-delta channel.
///
/// Construct-provision and the remaining hooks keep their trait defaults; preset
/// helper methods (e.g. package loading by name) arrive with the standard spec
/// database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatexlikeDriver {
    /// The tolerant-parsing policy to drive under (default: [`Recovery::Strict`]).
    pub recovery: Recovery,
}

impl LatexlikeDriver {
    /// A driver with the given recovery policy.
    pub fn new(recovery: Recovery) -> LatexlikeDriver {
        LatexlikeDriver { recovery }
    }
}

impl Default for LatexlikeDriver {
    fn default() -> Self {
        LatexlikeDriver { recovery: Recovery::Strict }
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

    /// The math plug (DESIGN_RATIONALE.md §3.3/§3.6): a math group's interior parses in
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
