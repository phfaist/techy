//! Task 1+2: the "notely" language definition on techy's generic machinery.
//!
//! notely syntax:
//! - commands:  `@name(...)...` — escape char `@`, lowercase names
//! - groups:    `[...]` in running text (class `Square`); `(...)` exists only as
//!   argument delimiters minted per-argument (class `Paren`)
//! - comments:  `# ...` to end of line
//! - NO math mode: `ModeId = ()`, no math group rules anywhere.

use std::sync::Arc;

use techy::engine::{CommandResolution, ParseDriver};
use techy::error::Recovery;
use techy::scopes::ScopeStack;
use techy::state::{ClosedVocabulary, Lang, StateData};
use techy::token::{
    CommandRule, CommentRule, GroupRule, SpecialsMatch, Token, TokenResult, TokenRules,
    TriggerChars, WhitespaceRules,
};
use techy::state::ParsingState;

/// notely's closed group-class vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotelyGroup {
    /// `[...]` content groups in running text.
    Square,
    /// `(...)` argument delimiters, minted per-argument by the specs.
    Paren,
}

impl ClosedVocabulary for NotelyGroup {
    const ALL: &'static [Self] = &[NotelyGroup::Square, NotelyGroup::Paren];
}

/// notely's closed invocation-form vocabulary: `@command`s and specials shorthands
/// (`-->`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NotelyCallable {
    Command,
    Shorthand,
}

impl ClosedVocabulary for NotelyCallable {
    const ALL: &'static [Self] = &[NotelyCallable::Command, NotelyCallable::Shorthand];
}

/// The compile-time language bundle (a ZST, per the Lang docs).
#[derive(Debug, Clone, Copy)]
pub struct Notely;

impl Lang for Notely {
    type GroupTypeId = NotelyGroup;
    type CallableTypeId = NotelyCallable;
    type ModeId = (); // no modes: notely has no math
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type Driver = NotelyDriver;

    fn initial_state_data() -> StateData<Self> {
        StateData {
            rules: TokenRules {
                enable_whitespace: true,
                whitespace: WhitespaceRules { chars: " \t\n\r".into() },
                enable_multi_newline_paragraphs: true,
                enable_groups: true,
                groups: vec![square_rule()],
                temporary_groups: Vec::new(),
                enable_commands: true,
                commands: vec![Arc::new(CommandRule {
                    escape_char: '@',
                    name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
                })],
                enable_comments: true,
                comments: vec![Arc::new(CommentRule { start: "#".into() })],
                enable_specials: true,
                forbidden_chars: "".into(),
                expecting_group_close: None,
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }
    }

    // Specials wiring: both hooks delegate to the scope stack's standard folds
    // (shorthand definitions then live in packages like any other callable).
    fn scan_specials<'s>(
        state: &ParsingState<Self>,
        content: &'s str,
        pos: usize,
    ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
        state.scopes().scan_specials(state, content, pos)
    }

    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        data.scopes.specials_trigger_chars()
    }
}

/// The `[...]` running-text group rule (part of the seed state).
pub fn square_rule() -> Arc<GroupRule<Notely>> {
    Arc::new(GroupRule {
        group_type: NotelyGroup::Square,
        open: "[".into(),
        close: "]".into(),
    })
}

/// The `(...)` argument-delimiter rule — NOT in the seed rules; specs mint it
/// per-argument (`GroupArgumentParser::with_rule`), so parens in running text
/// stay ordinary characters.
pub fn paren_rule() -> Arc<GroupRule<Notely>> {
    Arc::new(GroupRule {
        group_type: NotelyGroup::Paren,
        open: "(".into(),
        close: ")".into(),
    })
}

/// notely's parse driver: recovery knob + command resolution via the scope stack.
/// Everything else keeps the trait defaults.
#[derive(Debug, Clone, Copy)]
pub struct NotelyDriver {
    pub recovery: Recovery,
}

impl NotelyDriver {
    pub fn new(recovery: Recovery) -> NotelyDriver {
        NotelyDriver { recovery }
    }
}

impl Default for NotelyDriver {
    fn default() -> Self {
        NotelyDriver { recovery: Recovery::Strict }
    }
}

impl ParseDriver<Notely> for NotelyDriver {
    fn recovery(&self) -> Recovery {
        self.recovery
    }

    fn resolve_command(
        &self,
        state: &ParsingState<Notely>,
        token: &Token<'_, Notely>,
    ) -> CommandResolution<Notely> {
        CommandResolution::resolve_via_scopes(state, token, NotelyCallable::Command)
    }
}
