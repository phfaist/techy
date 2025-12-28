//! Parsing state and context management.

use crate::spec::LatexContextDb;

/// The parsing state tracks context information during parsing.
#[derive(Clone)]
pub struct ParsingState<'ctx> {
    /// Are we currently in math mode?
    pub in_math_mode: bool,

    /// The LaTeX context (known macros/environments).
    pub latex_context: &'ctx LatexContextDb,
}

impl<'ctx> ParsingState<'ctx> {
    /// Create a new parsing state with the given context.
    pub fn new(latex_context: &'ctx LatexContextDb) -> Self {
        Self {
            in_math_mode: false,
            latex_context,
        }
    }

    /// Create a sub-state (copy of current state).
    pub fn sub_state(&self) -> Self {
        Self {
            in_math_mode: self.in_math_mode,
            latex_context: self.latex_context,
        }
    }

    /// Apply a state delta to create a new state.
    pub fn apply_delta(&self, delta: &StateDelta) -> Self {
        let mut new_state = self.sub_state();

        match delta {
            StateDelta::EnterMathMode => {
                new_state.in_math_mode = true;
            }
            StateDelta::ExitMathMode => {
                new_state.in_math_mode = false;
            }
            StateDelta::SetMathMode(value) => {
                new_state.in_math_mode = *value;
            }
        }

        new_state
    }

    /// Enter math mode.
    pub fn with_math_mode(mut self, in_math_mode: bool) -> Self {
        self.in_math_mode = in_math_mode;
        self
    }
}

/// Represents a change to the parsing state.
///
/// State deltas are returned by parsers to indicate how the parsing
/// state should change after parsing a construct.
#[derive(Debug, Clone, PartialEq)]
pub enum StateDelta {
    /// Enter math mode.
    EnterMathMode,

    /// Exit math mode.
    ExitMathMode,

    /// Set math mode to a specific value.
    SetMathMode(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::LatexContextDb;

    #[test]
    fn test_parsing_state_creation() {
        let ctx = LatexContextDb::new();
        let state = ParsingState::new(&ctx);
        assert!(!state.in_math_mode);
    }

    #[test]
    fn test_state_delta_application() {
        let ctx = LatexContextDb::new();
        let state = ParsingState::new(&ctx);

        let new_state = state.apply_delta(&StateDelta::EnterMathMode);
        assert!(new_state.in_math_mode);

        let state2 = new_state.apply_delta(&StateDelta::ExitMathMode);
        assert!(!state2.in_math_mode);
    }

    #[test]
    fn test_with_math_mode() {
        let ctx = LatexContextDb::new();
        let state = ParsingState::new(&ctx).with_math_mode(true);
        assert!(state.in_math_mode);
    }
}
