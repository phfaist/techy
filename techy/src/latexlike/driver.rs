//! [`LatexlikeDriver`]: the preset's [`ParseDriver`] — recovery policy, scope-stack
//! command resolution, and the math-mode group plug.

use alloc::string::ToString;
use alloc::sync::Arc;

use crate::engine::{CommandResolution, ParseDriver, ResolvedCallable};
use crate::error::Recovery;
use crate::scopes::{CallableQuery, CallableSyntax};
use crate::state::{ParsingState, ParsingStateDelta};
use crate::token::{GroupRule, Token, TokenKind};

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

    /// Resolve a command token as a [`Macro`](CallableType::Macro) through the
    /// state's scope stack. A miss reports the searched providers as the
    /// unresolvable-command detail; a provider failure reports the provider's error.
    fn resolve_command(
        &self,
        state: &ParsingState<Latexlike>,
        token: &Token<'_, Latexlike>,
    ) -> CommandResolution<Latexlike> {
        let TokenKind::Command { name, escape_char, .. } = &token.kind else {
            return CommandResolution::Unresolved { detail: None };
        };
        let query = CallableQuery::new(
            CallableType::Macro,
            name,
            CallableSyntax::Command { escape_char: *escape_char },
        )
        .with_token(token);
        match state.scopes().retrieve_spec(&query, state) {
            Ok(Some(spec)) => CommandResolution::Resolved(ResolvedCallable {
                callable_type: CallableType::Macro,
                spec,
            }),
            Ok(None) => CommandResolution::Unresolved {
                detail: Some(state.scopes().searched_providers().to_string()),
            },
            Err(error) => CommandResolution::Unresolved { detail: Some(error.to_string()) },
        }
    }

    /// The math plug (DESIGN_RATIONALE.md §3.3/§3.6): a math group's interior parses
    /// in [`Mode::Math`]. Pure in `(base, rule)` per the memoization contract;
    /// unconditional on the base mode — re-entering math inside math is a no-op
    /// override.
    fn group_interior_delta(
        &self,
        base: &ParsingState<Latexlike>,
        rule: &Arc<GroupRule<Latexlike>>,
    ) -> Option<ParsingStateDelta<Latexlike>> {
        let _ = base;
        match rule.group_type {
            GroupType::Math => Some(ParsingStateDelta::new().mode(Mode::Math)),
            GroupType::Content => None,
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
