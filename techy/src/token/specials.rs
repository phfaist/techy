//! The specials-scan interface: how the tokenizer asks the language (preset) whether a
//! callable-triggering character sequence starts at the current position.
//!
//! Specials trigger strings are the one part of tokenization that is *not* plain
//! [`TokenRules`](super::TokenRules) data: a language may have many trigger strings in
//! scope, changing with scope-stack ops, so their recognition is delegated to a
//! [`Lang`](crate::state::Lang) hook (`Lang::scan_specials`) instead of being enumerated in
//! the rules. Recognition and resolution happen in one call: a
//! [`SpecialsMatch`] carries the resolved spec, and the matched text is the name, which
//! removes normalization and scoping mismatches between scanning and lookup by
//! construction.
//!
//! The hot path is protected by [`TriggerChars`]: the set of characters that may start a
//! specials trigger, reported by `Lang::specials_trigger_chars` and cached per parsing
//! state (rebuilt only at transitions, like the
//! [`PrefixTable`](super::PrefixTable)). The scan hook is consulted only when the current
//! character is in the set.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::source::Span;
use crate::spec::CallableSpec;
use crate::state::Lang;

use super::error::TokenErrorKind;

/// A successful specials scan: a callable trigger matched at the scanned position.
///
/// Returned by `Lang::scan_specials(state, content, pos)`. Positions are absolute byte
/// offsets into `content` (not relative to the scanned tail), so spans in errors and in the
/// resulting token need no offset arithmetic.
pub struct SpecialsMatch<L: Lang> {
    /// Byte offset one past the end of the matched trigger.
    ///
    /// The matched text is `content[pos..end]`, and that text *is* the specials name
    /// the reader records on the token — which is why the match carries no separate
    /// name.
    ///
    /// **Contract:** the match must be non-empty and boundary-aligned — `end` must
    /// satisfy `pos < end <= content.len()` and fall on a `char` boundary. A
    /// zero-width match would produce a token that never advances the parse (an
    /// infinite loop); an out-of-range or mid-codepoint `end` would produce an invalid
    /// span. The standard reader validates the contract at the call site and reports a
    /// violation as an unrecoverable implementation error — never a panic.
    pub end: usize,
    /// The invocation form the trigger resolved to (recorded on the token; the dispatch
    /// loop needs it to build an `Invocation`). Recognition = resolution, and a
    /// resolution is the *(callable type, spec)* pair — the same shape as
    /// [`ResolvedCallable`](crate::engine::ResolvedCallable).
    pub callable_type: L::CallableTypeId,
    /// The resolved behavior spec. Never absent: unknown-name policy (fallback specs) is
    /// the scan implementation's business, applied *before* returning a match.
    pub spec: Arc<dyn CallableSpec<L>>,
}

impl<L: Lang> fmt::Debug for SpecialsMatch<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecialsMatch")
            .field("end", &self.end)
            .field("callable_type", &self.callable_type)
            .field("spec", &self.spec)
            .finish()
    }
}

/// A failed specials scan, in the scanned content's own coordinates.
///
/// The scan hook detects a trigger in a `&str`. It knows neither the reader's token
/// type nor the reader's stream positions, so it cannot describe how to carry on past
/// the failure — recovery is the reader's business. A scan failure is therefore a plain
/// condition plus a byte range: the reader turns it into a
/// [`TokenError`](super::TokenError) qualified by its own source, with no recovery, and
/// that error aborts the parse even under tolerant recovery.
///
/// A document-level condition the scan *can* carry on from is expressed the way the
/// hook expresses everything else — as a match to a spec whose parser diagnoses it.
#[derive(Debug, Clone)]
pub struct SpecialsScanError {
    /// What went wrong.
    pub kind: TokenErrorKind,
    /// Where in the scanned content it went wrong.
    pub span: Span,
}

impl fmt::Display for SpecialsScanError {
    /// The condition's own wording, followed by the byte range it was reported at —
    /// the range is the only locating information this error carries (it names no
    /// source: the scanned content is the reader's, not the hook's).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at bytes {}..{})", self.kind, self.span.start(), self.span.end())
    }
}

impl core::error::Error for SpecialsScanError {}

/// The characters that may start a specials trigger in some parsing state.
///
/// Computed by `Lang::specials_trigger_chars(&StateData<L>)` when a state is frozen and
/// cached on the state instance. A scan hook whose triggers cannot be summarized by first
/// characters (fully dynamic recognition) returns [`TriggerChars::Any`] — correct but paying
/// the scan on every character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerChars {
    /// Only these characters can start a trigger; the empty string means "no specials at
    /// all" (the default — the scan hook is then never consulted).
    Only(String),
    /// Any character may start a trigger; the scan hook is consulted at every content
    /// position. Conservative fallback for dynamic scanners.
    Any,
}

impl TriggerChars {
    /// Whether a trigger could start with `c` (i.e. whether the scan hook must be asked).
    pub fn may_start(&self, c: char) -> bool {
        match self {
            TriggerChars::Only(chars) => chars.contains(c),
            TriggerChars::Any => true,
        }
    }

    /// The union of two filters: a character may start a trigger in the union iff it may
    /// in either operand ([`Any`](TriggerChars::Any) absorbs). The fold behind a
    /// multi-provider scan: the state caches one filter for the whole
    /// [`ScopeStack`](crate::scopes::ScopeStack), unioned over its providers at freeze.
    #[must_use]
    pub fn union(&self, other: &TriggerChars) -> TriggerChars {
        match (self, other) {
            (TriggerChars::Any, _) | (_, TriggerChars::Any) => TriggerChars::Any,
            (TriggerChars::Only(a), TriggerChars::Only(b)) => {
                let mut chars = a.clone();
                chars.extend(b.chars().filter(|&c| !a.contains(c)));
                TriggerChars::Only(chars)
            }
        }
    }
}

impl Default for TriggerChars {
    /// No specials: the scan hook is never consulted.
    fn default() -> Self {
        TriggerChars::Only(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_chars_membership() {
        let none = TriggerChars::default();
        assert!(!none.may_start('~'));

        let some = TriggerChars::Only("~&".into());
        assert!(some.may_start('~'));
        assert!(some.may_start('&'));
        assert!(!some.may_start('x'));

        assert!(TriggerChars::Any.may_start('x'));
    }
}
