//! Parsing state and context management.






use crate::spec::ContextDb;

/// The parsing state tracks context information during parsing.
#[derive(Clone)]
pub struct ParsingState<'ctx> {
    /// Are we currently in math mode?
    pub in_math_mode: bool,

    /// The context database (known macros/environments).
    pub context: &'ctx ContextDb,
}

impl<'ctx> ParsingState<'ctx> {
    /// Create a new parsing state with the given context.
    pub fn new(context: &'ctx ContextDb) -> Self {
        Self {
            in_math_mode: false,
            context,
        }
    }

    /// Create a sub-state (copy of current state).
    pub fn sub_state(&self) -> Self {
        Self {
            in_math_mode: self.in_math_mode,
            context: self.context,
        }
    }

    /// Apply a state delta to create a new state.
    pub fn apply_delta(&self, delta: &ParsingStateDelta) -> Self {
        let mut new_state = self.sub_state();

        match delta {
            ParsingStateDelta::EnterMathMode => {
                new_state.in_math_mode = true;
            }
            ParsingStateDelta::ExitMathMode => {
                new_state.in_math_mode = false;
            }
            ParsingStateDelta::SetMathMode(value) => {
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
/// Parsing state deltas are returned by parsers to indicate how the parsing
/// state should change after parsing a construct.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsingStateDelta {
    /// Update parsing state attributes
    UpdateParsingState {
        attributes: 
    },

    /// Enter math mode.
    EnterMathMode,

    /// Exit math mode.
    ExitMathMode,

}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ContextDb;

    #[test]
    fn test_parsing_state_creation() {
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);
        assert!(!state.in_math_mode);
    }

    #[test]
    fn test_state_delta_application() {
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx);

        let new_state = state.apply_delta(&ParsingStateDelta::EnterMathMode);
        assert!(new_state.in_math_mode);

        let state2 = new_state.apply_delta(&ParsingStateDelta::ExitMathMode);
        assert!(!state2.in_math_mode);
    }

    #[test]
    fn test_with_math_mode() {
        let ctx = ContextDb::new();
        let state = ParsingState::new(&ctx).with_math_mode(true);
        assert!(state.in_math_mode);
    }
}
