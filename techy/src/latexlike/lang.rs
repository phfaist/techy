//! The traits that let a language of your own reuse the preset's behavior.
//!
//! Nothing in the preset is written for [`Latexlike`] alone. Its token rules, its
//! specs, its construct parsers and its driver hooks are all generic over a *family*
//! of languages, so a language that wants LaTeX-like syntax but its own vocabularies,
//! node data, or state extension gets them without copying anything. The bound
//! naming that family is [`LatexlikeLang`] — conventionally written as the type
//! parameter `LLL` — and joining it is one line, `impl LatexlikeLang for MyLang {}`.
//!
//! To be admitted, a language's vocabularies must be able to supply the values the
//! preset's machinery needs: a math group class, a macro invocation form, a math
//! mode, and so on. Each vocabulary answers for itself, through one trait implemented
//! on the vocabulary type:
//!
//! - [`LatexlikeGroupType`] on [`Lang::GroupTypeId`] — content, math, and verbatim
//!   group classes;
//! - [`LatexlikeCallableType`] on [`Lang::CallableTypeId`] — the macro, environment,
//!   and specials invocation forms;
//! - [`LatexlikeMode`] on [`Lang::ModeId`] — the mode math interiors parse in;
//! - [`LatexlikeEvent`] on [`Lang::Event`] — the "leave the math context" event;
//! - [`LatexlikeInvocationSyntax`] on [`Lang::InvocationSyntax`] — the record of how
//!   an invocation was spelled.
//!
//! techy implements all five for the preset's own types ([`GroupType`],
//! [`CallableType`], [`Mode`], [`Event`], and
//! [`InvocationSyntaxData`](super::InvocationSyntaxData)), so a language that adopts
//! those as its associated types satisfies every bound with no code of its own. A
//! language that extends one of them with variants of its own implements the trait
//! instead, which is what guarantees the values the preset needs still exist.
//!
//! [`LatexlikeLang`] also carries the preset's language-level settings as methods
//! with defaults: the math-delimiter table ([`math_group_rules`](LatexlikeLang::math_group_rules)),
//! the characters a math interior forbids
//! ([`math_interior_forbidden_chars`](LatexlikeLang::math_interior_forbidden_chars)),
//! and the checks a parse runs at start-up
//! ([`check_parse_start`](LatexlikeLang::check_parse_start)). Overriding one changes
//! it for the whole family member, with no need to fork
//! [`default_token_rules`](super::default_token_rules).
//!
//! [Writing your own language](crate::guide::custom_lang) covers the [`Lang`] trait
//! these build on.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::error::Diagnostics;
use crate::source::{Source, TextContent};
use crate::state::{Lang, ParsingState};
use crate::token::GroupRule;

use super::driver::LatexlikeParseDriver;
use super::invocation_syntax::EnvironmentSyntax;
use super::{CallableType, Event, GroupType, MathGroupForm, Mode};

/// What the preset requires of a language's group-class vocabulary
/// ([`Lang::GroupTypeId`]).
///
/// Three classes have to exist for the preset's rules and parsers to work: the plain
/// content group `{…}`, the math group in a given [`MathGroupForm`], and the verbatim
/// region. This trait asks the vocabulary type for them, rather than demanding the
/// preset's own [`GroupType`], so a language that adds classes of its own still fits.
/// [`GroupType`] implements it, so adopting that enum costs no code.
///
/// # Coherence contracts
///
/// - `Self::math_group(f).math_form() == Some(f)` for every form `f`;
/// - `content_group().math_form()` and `verbatim_group().math_form()` are `None`;
/// - [`is_math`](LatexlikeGroupType::is_math) is `true` for every class whose
///   interior should parse as math.
///
/// # The `is_math` / `math_form` split
///
/// The two math queries are separate on purpose.
/// [`is_math`](LatexlikeGroupType::is_math) decides *parse behavior*: for a class
/// that answers `true`, the interior parses in math mode and the math delimiters stop
/// being openers there ([`math_group_interior_delta`](super::math_group_interior_delta)).
/// [`math_form`](LatexlikeGroupType::math_form) reports *presentation*, and is what a
/// reader of a finished tree calls
/// ([`NodeRef::math_form`](crate::core::node::NodeRef::math_form)).
///
/// A language with a math-like class that has no inline/display presentation
/// therefore overrides `is_math` to answer `true` for it while `math_form` stays
/// `None`.
pub trait LatexlikeGroupType: Copy {
    /// The plain content-group class (the preset's `{…}`).
    fn content_group() -> Self;

    /// The math-group class appearing in the given [`MathGroupForm`].
    fn math_group(form: MathGroupForm) -> Self;

    /// The verbatim (raw-text region) class.
    fn verbatim_group() -> Self;

    /// The form this class appears in, when it is a math class; `None` otherwise.
    fn math_form(self) -> Option<MathGroupForm>;

    /// Whether interiors of this class parse as math.
    ///
    /// Defaults to "carries a math form". Override it to answer `true` for a
    /// math-like class with no inline/display presentation — see the trait
    /// documentation.
    fn is_math(self) -> bool {
        self.math_form().is_some()
    }
}

/// What the preset requires of a language's callable-type vocabulary
/// ([`Lang::CallableTypeId`]).
///
/// The three invocation forms — macro, environment, specials — have to exist, as
/// values of the language's own type. [`CallableType`] implements this, so adopting
/// that enum costs no code; a language that adds forms of its own implements it
/// instead.
///
/// The constructors are named for the role and the noun together
/// (`macro_callable()`, not `macro()`, which is not spellable in Rust); the
/// predicates are named for the role alone.
///
/// # Coherence contracts
///
/// Mirroring [`LatexlikeGroupType`]'s: `Self::macro_callable().is_macro()`,
/// `Self::environment_callable().is_environment()`, and
/// `Self::specials_callable().is_specials()` are all `true`, and each predicate
/// answers `true` for exactly the values playing that role. The defaults compare
/// against the constructors; a language where several values play one role
/// overrides the predicate.
pub trait LatexlikeCallableType: Copy + PartialEq {
    /// The macro invocation form (the preset's `\emph{…}` shape).
    fn macro_callable() -> Self;

    /// The environment invocation form (`\begin{…}…\end{…}`).
    fn environment_callable() -> Self;

    /// The specials invocation form (trigger character sequences: `~`, `---`).
    fn specials_callable() -> Self;

    /// Whether this value plays the macro role.
    fn is_macro(self) -> bool {
        self == Self::macro_callable()
    }

    /// Whether this value plays the environment role.
    fn is_environment(self) -> bool {
        self == Self::environment_callable()
    }

    /// Whether this value plays the specials role.
    fn is_specials(self) -> bool {
        self == Self::specials_callable()
    }
}

/// What the preset requires of a language's mode vocabulary ([`Lang::ModeId`]): the
/// math mode, and a way to recognize it.
///
/// [`Mode`] implements this, so adopting that enum costs no code.
///
/// There is deliberately no text-mode constructor and no `is_text`. The preset never
/// invents a non-math mode: leaving math restores an enclosing context that actually
/// exists, mode value included, found on the stack of enclosing states
/// ([`exit_math_context_delta`](super::exit_math_context_delta)). The only two things
/// the preset has to ask a mode vocabulary are which mode math interiors parse in,
/// and whether a given mode is math.
///
/// # Coherence contract
///
/// `Self::math_mode().is_math() == true`, and the predicate answers `true` for
/// exactly the modes the language considers math (override the equality default
/// for a language with several math-flavored modes).
pub trait LatexlikeMode: Copy + PartialEq {
    /// The mode math-group interiors parse in.
    fn math_mode() -> Self;

    /// Whether this mode is a math mode.
    fn is_math(self) -> bool {
        self == Self::math_mode()
    }
}

/// What the preset requires of a language's event vocabulary ([`Lang::Event`]): the
/// "leave the math context" event, and a way to recognize it.
///
/// [`Event`] implements this, so adopting that enum costs no code.
///
/// Leaving math is an *event* rather than a fixed state change because the change it
/// stands for depends on what encloses the point of use — see
/// [`Event::ExitMathContext`]. An argument recipe written for the whole family
/// (`\text{…}` and its relatives) creates the event in the language's own event type
/// here, and the driver recognizes it again when it resolves the event into a state
/// change
/// ([`ParseDriver::resolve_state_event`](crate::core::ParseDriver::resolve_state_event),
/// which delegates to [`exit_math_context_delta`](super::exit_math_context_delta)).
///
/// # Coherence contract
///
/// `Self::exit_math_context().is_exit_math_context() == true`, and the recognizer
/// answers `true` for exactly the events meaning "exit the math context". The
/// recognizer is a required method because events carry no equality bound.
pub trait LatexlikeEvent {
    /// The event asking to restore the innermost enclosing non-math context.
    fn exit_math_context() -> Self;

    /// Whether this event is the exit-math-context event.
    fn is_exit_math_context(&self) -> bool;
}

/// What the preset requires of a language's invocation-syntax payload
/// ([`Lang::InvocationSyntax`]): a way to record, and read back, which of the three
/// invocation forms a callable node was written in.
///
/// The payload is what a `Callable` node stores about its own spelling — the escape
/// character a macro was triggered by, the whitespace that ended its name, the pieces
/// of an environment's `\begin` and `\end`. Requiring this trait is what lets the
/// preset's parsers and its source recomposer
/// ([`source_recomposer`](super::source_recomposer)) work with any family member's
/// payload type. [`InvocationSyntaxData`](super::InvocationSyntaxData) implements it
/// for any environment record `Env`.
///
/// The associated [`Env`](LatexlikeInvocationSyntax::Env) type is the environment
/// side of that record ([`EnvironmentSyntax`]), and is the one place to customize
/// what gets recorded for an environment: a language chooses its record by choosing
/// its payload type.
///
/// # Coherence contracts
///
/// Mirroring the other role traits': `macro_form(e, p).macro_syntax() ==
/// Some((e, &p))`, `environment_form(env).environment_syntax() == Some(&env)`,
/// `specials_form().is_specials() == true`, and each accessor answers `Some`/
/// `true` for exactly the values playing that form.
pub trait LatexlikeInvocationSyntax<L: LatexlikeLang> {
    /// The environment-side record type ([`EnvironmentSyntax`]).
    type Env: EnvironmentSyntax<L>;

    /// The macro form: a command-triggered invocation's recorded facts (escape
    /// character + the trigger token's syntactic post-space).
    fn macro_form(escape_char: char, post_space: TextContent) -> Self;

    /// The environment form, over the filled environment-side record.
    fn environment_form(env: Self::Env) -> Self;

    /// The specials form (records nothing beyond the node's as-written `name`).
    fn specials_form() -> Self;

    /// The macro facts, when this payload plays the macro form.
    fn macro_syntax(&self) -> Option<(char, &TextContent)>;

    /// The environment record, when this payload plays the environment form.
    fn environment_syntax(&self) -> Option<&Self::Env>;

    /// Whether this payload plays the specials form.
    fn is_specials(&self) -> bool;
}

/// A language that can use the latexlike preset: the bound every generic preset
/// component takes.
///
/// A [`Lang`] qualifies when its vocabularies play the preset's roles
/// ([`LatexlikeGroupType`], [`LatexlikeCallableType`], [`LatexlikeMode`],
/// [`LatexlikeEvent`], and on the invocation-syntax payload
/// [`LatexlikeInvocationSyntax`] together with
/// [`FromInvocation`](crate::core::constructs::FromInvocation)) and its driver
/// implements [`LatexlikeParseDriver`]. Every generic item of the preset takes this
/// bound, conventionally under the parameter name `LLL`.
///
/// Opting in is explicit and one line — `impl LatexlikeLang for MyLang {}` — which is
/// exactly how [`Latexlike`](super::Latexlike) itself joins. There is no blanket
/// implementation over the vocabulary bounds, because a blanket implementation would
/// fix the defaulted methods below and leave no way to override them per language.
///
/// A custom driver joins the same way, `impl LatexlikeParseDriver<MyLang> for
/// MyDriver {}`, since every hook of [`LatexlikeParseDriver`] has a default.
///
/// The trait deliberately does *not* require
/// [`ClosedVocabulary`](crate::core::ClosedVocabulary) of the vocabularies. That
/// bound is stated at the places that actually enumerate a vocabulary, so a language
/// whose vocabularies are open still fits here.
///
/// Every latexlike language has every parsing feature: the bound fixes
/// [`Features`](Lang::Features) to
/// [`AllLangFeatures`](crate::core::AllLangFeatures), so token rules are stored
/// plainly and no per-feature gating shows up anywhere in the family.
///
/// # Language-level settings
///
/// The methods below have defaults that reproduce the preset's own behavior.
/// Override one to change it for a whole language, rather than forking
/// [`default_token_rules`](super::default_token_rules) or the driver.
pub trait LatexlikeLang:
    Lang<
        Features = crate::state::AllLangFeatures,
        GroupTypeId: LatexlikeGroupType,
        CallableTypeId: LatexlikeCallableType,
        ModeId: LatexlikeMode,
        Event: LatexlikeEvent,
        Driver: LatexlikeParseDriver<Self>,
        InvocationSyntax: LatexlikeInvocationSyntax<Self>
                              + crate::constructs::FromInvocation<Self>,
    >
{
    /// The language's math-delimiter pairs — the group rules
    /// [`default_token_rules`](super::default_token_rules) puts in its seed state.
    ///
    /// The default is the four familiar pairs, each declaring its
    /// [`MathGroupForm`] through the language's own
    /// [`math_group`](LatexlikeGroupType::math_group) constructor: `$…$` (inline),
    /// `$$…$$` (display), `\(…\)` (inline), `\[…\]` (display). Override to give the
    /// language fewer pairs, extra pairs, or different spellings.
    fn math_group_rules() -> Vec<Arc<GroupRule<Self>>> {
        fn rule<L: Lang>(
            group_type: L::GroupTypeId,
            open: &str,
            close: &str,
        ) -> Arc<GroupRule<L>> {
            Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
        }
        vec![
            rule(Self::GroupTypeId::math_group(MathGroupForm::Inline), "$", "$"),
            rule(Self::GroupTypeId::math_group(MathGroupForm::Display), "$$", "$$"),
            rule(Self::GroupTypeId::math_group(MathGroupForm::Inline), r"\(", r"\)"),
            rule(Self::GroupTypeId::math_group(MathGroupForm::Display), r"\[", r"\]"),
        ]
    }

    /// The characters a math interior forbids, given the math-delimiter rules that
    /// were removed on entering it.
    ///
    /// LaTeX-like languages do not nest math, so entering a math group removes the
    /// math delimiters from the group rules in force. A stray `$` inside math would
    /// then be silent content; forbidding the character instead turns it into a
    /// diagnostic. [`math_group_interior_delta`](super::math_group_interior_delta)
    /// calls this with the rules it removed and merges the answer into the forbidden
    /// characters the surrounding state already had.
    ///
    /// The default returns every single-character open or close spelling among
    /// `removed`, deduplicated — for the four standard pairs that is exactly `$`,
    /// derived rather than written as a literal. Multi-character spellings need no
    /// entry: with their rules removed they cannot open a group anyway, a leading
    /// character they share is already covered (`$$` by `$`), and an escape-led
    /// spelling such as `\(` is read as a command instead. Override this in a
    /// language whose math delimiters are all multi-character and that still wants
    /// some character diagnosed.
    fn math_interior_forbidden_chars(removed: &[Arc<GroupRule<Self>>]) -> String {
        let mut chars = String::new();
        for rule in removed {
            for delimiter in [&rule.open, &rule.close] {
                let mut delimiter_chars = delimiter.chars();
                if let (Some(c), None) = (delimiter_chars.next(), delimiter_chars.next())
                {
                    if !chars.contains(c) {
                        chars.push(c);
                    }
                }
            }
        }
        chars
    }

    /// Checks the language runs once at the start of each root parse, reporting
    /// through `diagnostics`.
    ///
    /// [`LatexlikeDriver`](super::LatexlikeDriver) calls this from
    /// [`observe_parse_start`](crate::core::ParseDriver::observe_parse_start), at the
    /// first moment where both the seed state and the diagnostic sink are available.
    /// It is the place for warnings about how a document's definitions were
    /// registered, as opposed to anything in the document itself. The default checks
    /// nothing.
    ///
    /// [`Latexlike`](super::Latexlike) overrides it to warn about a provider whose
    /// commands can never be reached because no escape character in force triggers
    /// them
    /// ([`check_provider_commands_shadowed_by_escape`](crate::core::specs::check_provider_commands_shadowed_by_escape)).
    /// That check enumerates the vocabularies, so a language can adopt it with the
    /// same one-line override as long as its own vocabularies implement
    /// [`ClosedVocabulary`](crate::core::ClosedVocabulary); a language whose
    /// vocabularies are open keeps the default and simply goes without the check.
    fn check_parse_start(
        source: &Arc<Source<Self::SourceOrigin>>,
        seed: &Arc<ParsingState<Self>>,
        diagnostics: &mut Diagnostics<Self::SourceOrigin>,
    ) {
        let _ = (source, seed, diagnostics);
    }
}

// --- the preset's own vocabularies play the roles --------------------------------
//
// These impls are what makes "adopt the preset enums, get the family for free"
// true: a Lang whose associated types are the preset enums satisfies every
// LatexlikeLang vocabulary bound with zero code.

impl LatexlikeGroupType for GroupType {
    fn content_group() -> GroupType {
        GroupType::Content
    }

    fn math_group(form: MathGroupForm) -> GroupType {
        GroupType::Math(form)
    }

    fn verbatim_group() -> GroupType {
        GroupType::Verbatim
    }

    fn math_form(self) -> Option<MathGroupForm> {
        match self {
            GroupType::Math(form) => Some(form),
            _ => None,
        }
    }
}

impl LatexlikeCallableType for CallableType {
    fn macro_callable() -> CallableType {
        CallableType::Macro
    }

    fn environment_callable() -> CallableType {
        CallableType::Environment
    }

    fn specials_callable() -> CallableType {
        CallableType::Specials
    }
}

impl LatexlikeMode for Mode {
    fn math_mode() -> Mode {
        Mode::Math
    }
}

impl LatexlikeEvent for Event {
    fn exit_math_context() -> Event {
        Event::ExitMathContext
    }

    fn is_exit_math_context(&self) -> bool {
        matches!(self, Event::ExitMathContext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_enums_satisfy_the_role_coherence_contracts() {
        // Group: constructor/classifier round-trip + the is_math default.
        for form in [MathGroupForm::Inline, MathGroupForm::Display] {
            let class = GroupType::math_group(form);
            assert_eq!(class.math_form(), Some(form));
            assert!(class.is_math());
        }
        assert_eq!(GroupType::content_group().math_form(), None);
        assert!(!GroupType::content_group().is_math());
        assert_eq!(GroupType::verbatim_group().math_form(), None);
        assert!(!GroupType::verbatim_group().is_math());

        // Callable: constructor/predicate coherence, roles mutually exclusive.
        assert!(CallableType::macro_callable().is_macro());
        assert!(!CallableType::macro_callable().is_environment());
        assert!(CallableType::environment_callable().is_environment());
        assert!(CallableType::specials_callable().is_specials());
        assert!(!CallableType::specials_callable().is_macro());

        // Mode: only the math constructor + predicate (no text-mode vocabulary).
        assert!(Mode::math_mode().is_math());
        assert!(!Mode::Text.is_math());

        // Event: constructor/recognizer coherence.
        assert!(Event::exit_math_context().is_exit_math_context());
    }

    #[test]
    fn default_math_group_rules_declare_the_canonical_pairs_with_forms() {
        use super::super::Latexlike;
        let rules = <Latexlike as LatexlikeLang>::math_group_rules();
        let facts: alloc::vec::Vec<(&str, &str, Option<MathGroupForm>)> = rules
            .iter()
            .map(|rule| (&*rule.open, &*rule.close, rule.group_type.math_form()))
            .collect();
        assert_eq!(
            facts,
            [
                ("$", "$", Some(MathGroupForm::Inline)),
                ("$$", "$$", Some(MathGroupForm::Display)),
                (r"\(", r"\)", Some(MathGroupForm::Inline)),
                (r"\[", r"\]", Some(MathGroupForm::Display)),
            ]
        );
    }

    #[test]
    fn forbidden_chars_derive_from_the_removed_rules_not_a_literal() {
        use super::super::Latexlike;
        // The canonical rules: exactly `$` (from the single-char `$…$` pair;
        // `$$` adds nothing new, `\(`/`\[` are multi-char).
        let removed = <Latexlike as LatexlikeLang>::math_group_rules();
        assert_eq!(
            <Latexlike as LatexlikeLang>::math_interior_forbidden_chars(&removed),
            "$"
        );

        // A custom single-char pair contributes both its delimiters; deduped.
        let removed = vec![Arc::new(GroupRule {
            group_type: GroupType::math_group(MathGroupForm::Display),
            open: "«".into(),
            close: "»".into(),
        })];
        assert_eq!(
            <Latexlike as LatexlikeLang>::math_interior_forbidden_chars(&removed),
            "«»"
        );

        // Exclusively multi-char delimiters: nothing derived (the documented
        // override point).
        let removed = vec![Arc::new(GroupRule {
            group_type: GroupType::math_group(MathGroupForm::Display),
            open: "$$".into(),
            close: "$$".into(),
        })];
        assert_eq!(
            <Latexlike as LatexlikeLang>::math_interior_forbidden_chars(&removed),
            ""
        );
    }
}
