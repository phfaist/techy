//! The preset's generalization layer: the per-vocabulary **role traits** and the
//! [`LatexlikeLang`] umbrella.
//!
//! The latexlike preset's components are generic over a *family* of languages
//! (conventional parameter `LLL`), not hard-wired to [`Latexlike`]: a framework
//! language with its own node exts, state ext, or extended vocabularies reuses the
//! preset's driver, rules, and specs without forking them. The mechanism is
//! role-based: each closed vocabulary the preset needs is described by a **role
//! trait** implemented *by the vocabulary type itself* — the preset asks a foreign
//! `GroupTypeId` "give me your math class" instead of demanding the preset's enum.
//! techy implements the role traits for the preset's own enums ([`GroupType`],
//! [`CallableType`], [`Mode`], [`Event`]), so a language adopting those enums as its
//! associated types satisfies every bound with zero code, while a language with
//! extended vocabularies implements them itself — which *guarantees* the
//! preset-required values exist and leaves the enum open for its own additions.
//!
//! [`LatexlikeLang`] bundles the vocabulary bounds and carries the preset's
//! language-level behavior defaults (the math-delimiter data, the math-interior
//! forbidden-character derivation) as **defaulted, overridable methods**. There is
//! deliberately **no blanket impl** — coherence would make the defaults
//! un-overridable; opting a language into the family is one line:
//! `impl LatexlikeLang for MyLang {}`.
//!
//! Evolution posture: the required role surface freezes at stabilization; future
//! roles and behaviors arrive as defaulted methods delegating to existing ones
//! (non-breaking), and a fallback-less new role is a conscious breaking change.
//! [`ClosedVocabulary`](crate::state::ClosedVocabulary) is deliberately **not** a
//! supertrait of any role trait ("provide, don't require"): enumeration-driven
//! tooling states that bound where it is used.

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

/// Role trait for a latexlike **group-class vocabulary**
/// ([`Lang::GroupTypeId`]): the three classes the preset's machinery needs —
/// content, math (with its [`MathGroupForm`] payload), verbatim — as constructors on
/// the host's own type, plus the math classifier/predicate pair.
///
/// # Coherence contracts
///
/// - `Self::math_group(f).math_form() == Some(f)` for every form `f`;
/// - `content_group().math_form()` and `verbatim_group().math_form()` are `None`;
/// - [`is_math`](LatexlikeGroupType::is_math) is `true` for every value the
///   language wants parsed as a math interior (the driver's math-interior delta keys on it).
///
/// # The `is_math` / `math_form` split
///
/// The split is deliberate: the preset's math-interior behavior function keys on **`is_math`**
/// (parse behavior — enter math mode, drop the math openers), while readers key on
/// **`math_form`** (presentation). An extending language with a math-*like* class
/// that has no inline/display presentation overrides `is_math` to answer `true` for
/// it while `math_form` stays `None` — decoupling the two.
pub trait LatexlikeGroupType: Copy {
    /// The plain content-group class (the preset's `{…}`).
    fn content_group() -> Self;

    /// The math-group class appearing in the given [`MathGroupForm`].
    fn math_group(form: MathGroupForm) -> Self;

    /// The verbatim (raw-text region) class.
    fn verbatim_group() -> Self;

    /// The math classifier: the declared form of a math class, `None` otherwise.
    fn math_form(self) -> Option<MathGroupForm>;

    /// The math predicate — what the preset's parse wiring keys on. Defaults to
    /// "carries a math form"; override to decouple parse behavior from
    /// presentation (see the trait docs).
    fn is_math(self) -> bool {
        self.math_form().is_some()
    }
}

/// Role trait for a latexlike **callable-type vocabulary**
/// ([`Lang::CallableTypeId`]): the macro/environment/specials trichotomy as
/// constructors on the host's own type, with matching predicates.
///
/// The accessor names carry the role *and* the vocabulary noun
/// (`macro_callable()`, not `macro()` — which is not even spellable: `macro` is a
/// Rust keyword); predicates are the bare role.
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

/// Role trait for a latexlike **mode vocabulary** ([`Lang::ModeId`]): the math
/// mode as a constructor on the host's own type, with its predicate — deliberately
/// nothing more.
///
/// There is **no text-mode constructor and no `is_text`**: the preset never
/// *invents* a non-math mode — leaving math restores an actual enclosing context
/// (its mode value included) found on the enclosing-state stack
/// ([`exit_math_context_delta`](super::exit_math_context_delta)), so the only
/// required vocabulary is "which mode do math interiors parse in" and "is this
/// state's mode math". This keeps the required surface on foreign mode vocabularies
/// minimal.
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

/// Role trait for a latexlike **event vocabulary** ([`Lang::Event`]): the
/// exit-math-context transition event as a constructor on the host's own event
/// type, with its recognizer.
///
/// The preset's text-restore is an *event* by design ([`Event::ExitMathContext`]):
/// the restoring delta depends on the enclosing-state stack at use time, so the
/// `LLL`-generic argument recipe must mint the event **in the host's own event
/// type** and the driver must recognize it back
/// ([`ParseDriver::resolve_state_event`](crate::engine::ParseDriver::resolve_state_event)
/// → [`exit_math_context_delta`](super::exit_math_context_delta)). A preset-side
/// wrapper event would violate vocabulary-stays-the-host's-own; an event-less
/// design cannot exist (the patch has no meaning without the context).
///
/// # Coherence contract
///
/// `Self::exit_math_context().is_exit_math_context() == true`, and the recognizer
/// answers `true` for exactly the events meaning "exit the math context". The
/// recognizer is a required method (events carry no equality bound); future preset
/// events arrive as defaulted methods where a coherent default exists.
pub trait LatexlikeEvent {
    /// The exit-math-context event: restore the innermost enclosing non-math
    /// context (consumed by the preset driver's event lowering).
    fn exit_math_context() -> Self;

    /// Whether this event is the exit-math-context event.
    fn is_exit_math_context(&self) -> bool;
}

/// Role trait for a latexlike **invocation-syntax payload**
/// ([`Lang::InvocationSyntax`]): the macro / environment / specials invocation
/// forms as constructors on the host's own payload type, with the matching
/// accessors — the fifth member of the role-trait roster, so the preset's staging
/// sites (the invocation and specials sites, the environment composition) and its
/// source recomposer work over any family member's payload type.
///
/// The associated [`Env`](LatexlikeInvocationSyntax::Env) names the
/// environment-side record ([`EnvironmentSyntax`]) — the single customization
/// entry for environment-syntax recording: a language picks its record by picking
/// its payload type (the preset enum
/// [`InvocationSyntaxData<Env>`](super::InvocationSyntaxData) implements this
/// trait for any `Env`).
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

/// The latexlike **language-family** trait: a [`Lang`] whose vocabularies implement
/// the latexlike role traits ([`LatexlikeGroupType`], [`LatexlikeCallableType`],
/// [`LatexlikeMode`], [`LatexlikeEvent`], and — on the invocation-syntax
/// payload — [`LatexlikeInvocationSyntax`] +
/// [`FromInvocation`](crate::constructs::FromInvocation)) and whose driver
/// implements the preset's driver extension ([`LatexlikeParseDriver`]) — the bound
/// every generic preset component takes (conventional parameter `LLL`), plus the
/// preset's language-level behavior defaults as **overridable defaulted
/// methods**.
///
/// Opting in is explicit and one line — `impl LatexlikeLang for MyLang {}` — and
/// [`Latexlike`](super::Latexlike) itself opts in exactly that way. There is
/// deliberately **no blanket impl** over the vocabulary bounds: coherence would
/// make the defaulted methods un-overridable (a blanket impl's defaults cannot be
/// specialized per language).
///
/// The driver bound mirrors the core's own `Driver: `[`ParseDriver`](crate::engine::ParseDriver)
/// clause ([`Lang::Driver`]) one layer up: a custom driver for a latexlike language
/// opts in with its own one-liner — `impl LatexlikeParseDriver<MyLang> for MyDriver
/// {}` — since every hook of [`LatexlikeParseDriver`] is defaulted.
///
/// [`ClosedVocabulary`](crate::state::ClosedVocabulary) is *not* a supertrait —
/// it stays the opt-in tooling bound, stated where enumeration is actually used.
///
/// Every latexlike language has **every parsing feature**: the bound pins
/// [`Features`](Lang::Features) to
/// [`AllLangFeatures`](crate::state::AllLangFeatures), so the preset's rules data
/// is stored plainly and none of the per-feature storage machinery is visible
/// anywhere in the family.
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
    /// The math-delimiter group rules of this language's canonical token rules —
    /// the data behind [`default_token_rules`](super::default_token_rules).
    ///
    /// The default is the preset's four pairs, each declaring its
    /// [`MathGroupForm`] through the language's own
    /// [`math_group`](LatexlikeGroupType::math_group) constructor: `$…$` (inline),
    /// `$$…$$` (display), `\(…\)` (inline), `\[…\]` (display). Override to change
    /// the family's delimiter table (fewer pairs, different spellings, extra
    /// pairs) without forking `default_token_rules`.
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

    /// The characters to forbid inside a math-group interior, **derived from the
    /// math-class rules removed at entry** — the generalization of the preset's
    /// "merge `$` into the forbidden set" behavior, consumed by
    /// [`math_group_interior_delta`](super::math_group_interior_delta) (which
    /// merges the result into the outer state's existing forbidden set).
    ///
    /// The default returns every **single-character** open or close spelling among
    /// `removed` (deduplicated) — under the canonical rules that is exactly `$`,
    /// with no literal `$` written anywhere: LaTeX forbids nested math, so a stray
    /// single-character math delimiter inside math must become a diagnostic rather
    /// than silent content. Multi-character spellings need no entry: with the
    /// rules removed they cannot open anyway, their single-character prefixes are
    /// covered here (`$$` by `$`), and escape-led spellings (`\(`) fall through to
    /// the command path. A language whose math delimiters are exclusively
    /// multi-character overrides this to name the characters it wants diagnosed.
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

    /// The language's **parse-initialization checks**, fired once per root parse
    /// by [`LatexlikeDriver`](super::LatexlikeDriver)'s
    /// [`observe_parse_start`](crate::engine::ParseDriver::observe_parse_start)
    /// hook — registration-sanity diagnostics at the layering-correct moment (the
    /// sink is live, the seed's escape characters are known). The default checks
    /// nothing.
    ///
    /// [`Latexlike`](super::Latexlike) overrides this with the all-escape-shadowed
    /// provider check
    /// ([`check_provider_commands_shadowed_by_escape`](crate::scopes::check_provider_commands_shadowed_by_escape)) —
    /// legal there because the concrete preset vocabularies implement
    /// [`ClosedVocabulary`](crate::state::ClosedVocabulary). A family member whose
    /// vocabularies are enumerable opts in with the same one-line override (the
    /// bound holds monomorphically at that call site — "provide, don't require":
    /// the trait itself never demands enumeration); one whose vocabularies are not
    /// enumerable simply keeps the default, and the check is gracefully absent.
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
