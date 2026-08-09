//! Compile-time feature presence: the [`FeaturePresence`] vocabulary
//! ([`FeaturePresent`] / [`FeatureAbsent`]), the [`LangFeatures`] declaration bundle
//! named by [`Lang::Features`], the ready-made [`AllLangFeatures`] /
//! [`NoLangFeatures`] bundles, and the per-feature `LangHas*` bound traits.

use core::fmt;
use core::marker::PhantomData;

use super::lang::Lang;

mod sealed {
    /// Seals [`FeaturePresence`](super::FeaturePresence): the two markers
    /// [`FeaturePresent`](super::FeaturePresent) and
    /// [`FeatureAbsent`](super::FeatureAbsent) are the whole vocabulary.
    pub trait Sealed {}
}

impl sealed::Sealed for FeaturePresent {}
impl sealed::Sealed for FeatureAbsent {}

/// The compile-time answer to "does a language have this parsing feature at all?" —
/// the bound on every [`LangFeatures`] member. Exactly two types implement it:
/// [`FeaturePresent`] and [`FeatureAbsent`].
///
/// **Sealed**: a private supertrait keeps other implementations out. The vocabulary is
/// deliberately closed — a feature is present or absent, nothing else — so generic
/// machinery may rely on every presence answer being one of the two markers.
///
/// Deliberately **no** explicit `Send`/`Sync` requirements anywhere here: the two
/// possible [`Store`](FeaturePresence::Store) types are the stored type `T` itself and
/// a marker type containing nothing, so a store is `Send`/`Sync` exactly when `T` is —
/// the usual automatic rules apply unchanged.
pub trait FeaturePresence: sealed::Sealed + 'static {
    /// Whether the feature is present. A compile-time constant: generic code can
    /// branch on it, and the compiler removes the untaken arm when the language's
    /// declaration makes the value known.
    const PRESENT: bool;

    /// The storage a value of type `T` occupies under this presence answer: `T`
    /// itself when the feature is present ([`FeaturePresent`]), the zero-sized
    /// `PhantomData<T>` when it is absent ([`FeatureAbsent`]).
    ///
    /// **Not yet used by any field**: this type is reserved for per-feature storage —
    /// routing the token-rules blocks and the scope stack through it is planned but
    /// not implemented yet.
    ///
    /// The bounds are what generic code may do with any store: clone it, format it
    /// for debugging, and construct the default value (for the crate's rules data the
    /// default is the natural empty value — `false`, an empty string, an empty list,
    /// `None`). Equality is deliberately **not** promised: not every stored type has
    /// it (a list of scope providers, for example, has no equality comparison).
    type Store<T: Clone + fmt::Debug + Default>: Clone + fmt::Debug + Default;
}

/// The "feature is present" marker ([`FeaturePresence`]): the language has the
/// feature, and [`Store<T>`](FeaturePresence::Store) is `T` itself.
///
/// The store being `T` itself — no wrapper type — is a deliberate, relied-upon design
/// point: a concrete language with a feature present writes plain struct literals and
/// plain field reads for that feature's rules data, exactly as if the presence
/// machinery did not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeaturePresent;

impl FeaturePresence for FeaturePresent {
    const PRESENT: bool = true;
    type Store<T: Clone + fmt::Debug + Default> = T;
}

/// The "feature is absent" marker ([`FeaturePresence`]): the language has no such
/// feature — a compile-time fact — and [`Store<T>`](FeaturePresence::Store) is the
/// zero-sized `PhantomData<T>`.
///
/// *Absent* is distinct from the two runtime ways a feature can be off; the
/// [`LangFeatures`] docs define all three words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureAbsent;

impl FeaturePresence for FeatureAbsent {
    const PRESENT: bool = false;
    type Store<T: Clone + fmt::Debug + Default> = PhantomData<T>;
}

/// A language's per-feature presence declarations: one member per parsing feature,
/// each answering at compile time whether the language has that feature at all.
/// [`Lang::Features`] names the implementing type.
///
/// The eight members mirror the [`TokenRules`](crate::token::TokenRules) feature
/// blocks — whitespace handling, paragraph breaks, group delimiters, command syntax,
/// comment syntax, the specials scan, forbidden characters — plus the definition
/// scope stack ([`ScopeStack`](crate::scopes::ScopeStack)).
///
/// Declaring a member [`FeatureAbsent`] means the feature is **absent**: the language
/// has no such feature, and no runtime data can change that. Absent is one of three
/// distinct ways a feature can be "off", each with its own word, never interchanged:
///
/// - **absent** — compile-time: the language has no such feature (declared here);
/// - **disabled** — runtime: the feature block's `enabled` flag is `false` while its
///   rules data stays in place, so a later state delta can re-enable it losslessly;
/// - **empty** — the feature's rules data itself holds nothing, so nothing is
///   recognized even with the flag `true`.
///
/// Ready-made bundles cover the two ends: [`AllLangFeatures`] (every member present)
/// and [`NoLangFeatures`] (every member absent). This trait is deliberately **not**
/// sealed: a language with any other combination defines its own bundle — a unit
/// struct with the eight members filled in.
pub trait LangFeatures: 'static {
    /// Presence of whitespace handling
    /// ([`WhitespaceRules`](crate::token::WhitespaceRules)).
    type Whitespace: FeaturePresence;

    /// Presence of paragraph-break detection
    /// ([`ParagraphRules`](crate::token::ParagraphRules)). Paragraph breaks are found
    /// inside whitespace runs, so interfaces that require paragraphs require
    /// whitespace too ([`LangHasParagraphs`]).
    type Paragraphs: FeaturePresence;

    /// Presence of group delimiters ([`GroupRules`](crate::token::GroupRules)).
    type Groups: FeaturePresence;

    /// Presence of command syntax ([`CommandRules`](crate::token::CommandRules)).
    type Commands: FeaturePresence;

    /// Presence of comment syntax ([`CommentRules`](crate::token::CommentRules)).
    type Comments: FeaturePresence;

    /// Presence of the specials scan ([`SpecialsRules`](crate::token::SpecialsRules)).
    type Specials: FeaturePresence;

    /// Presence of the forbidden-character check
    /// ([`ForbiddenCharsRules`](crate::token::ForbiddenCharsRules)). This feature has
    /// no runtime `enabled` flag — an empty character set already spells the runtime
    /// off — but it is a full member on the compile-time axis like every other.
    type ForbiddenChars: FeaturePresence;

    /// Presence of the definition scope stack
    /// ([`ScopeStack`](crate::scopes::ScopeStack)). Deliberately independent of
    /// [`Commands`](LangFeatures::Commands): a language may resolve its callables
    /// from a fixed table with no scoped definitions at all.
    type Scopes: FeaturePresence;
}

/// The every-feature-present bundle: each [`LangFeatures`] member is
/// [`FeaturePresent`]. The declaration for full-syntax languages — the
/// [`latexlike`](crate::latexlike) preset uses it, and
/// [`TrivialLang`](crate::state::TrivialLang)'s blanket implementation supplies it
/// for every trivial language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllLangFeatures;

impl LangFeatures for AllLangFeatures {
    type Whitespace = FeaturePresent;
    type Paragraphs = FeaturePresent;
    type Groups = FeaturePresent;
    type Commands = FeaturePresent;
    type Comments = FeaturePresent;
    type Specials = FeaturePresent;
    type ForbiddenChars = FeaturePresent;
    type Scopes = FeaturePresent;
}

/// The every-feature-absent bundle: each [`LangFeatures`] member is [`FeatureAbsent`]
/// — a language of plain content characters, with no whitespace handling, paragraph
/// breaks, groups, commands, comments, specials, forbidden characters, or scope
/// stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoLangFeatures;

impl LangFeatures for NoLangFeatures {
    type Whitespace = FeatureAbsent;
    type Paragraphs = FeatureAbsent;
    type Groups = FeatureAbsent;
    type Commands = FeatureAbsent;
    type Comments = FeatureAbsent;
    type Specials = FeatureAbsent;
    type ForbiddenChars = FeatureAbsent;
    type Scopes = FeatureAbsent;
}

/// Bound for whitespace-requiring code: a [`Lang`] whose
/// [`Features`](Lang::Features) declare whitespace handling present
/// ([`Whitespace`](LangFeatures::Whitespace) `=` [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasWhitespace` instead of spelling out the declaration equality.
pub trait LangHasWhitespace:
    Lang<Features: LangFeatures<Whitespace = FeaturePresent>>
{
}

impl<L: Lang> LangHasWhitespace for L where
    L::Features: LangFeatures<Whitespace = FeaturePresent>
{
}

/// Bound for paragraph-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare paragraph-break detection present
/// ([`Paragraphs`](LangFeatures::Paragraphs) `=` [`FeaturePresent`]) — and, because
/// this trait requires [`LangHasWhitespace`], whitespace handling too. Paragraph
/// breaks are whitespace runs containing two or more newlines: whitespace detection
/// is what finds them (the reader's paragraph-break check runs inside its whitespace
/// handling), so a language cannot satisfy this bound with whitespace absent.
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically.
pub trait LangHasParagraphs:
    LangHasWhitespace + Lang<Features: LangFeatures<Paragraphs = FeaturePresent>>
{
}

impl<L: Lang> LangHasParagraphs for L where
    L::Features: LangFeatures<Whitespace = FeaturePresent, Paragraphs = FeaturePresent>
{
}

/// Bound for group-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare group delimiters present ([`Groups`](LangFeatures::Groups) `=`
/// [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasGroups` instead of spelling out the declaration equality.
pub trait LangHasGroups: Lang<Features: LangFeatures<Groups = FeaturePresent>> {}

impl<L: Lang> LangHasGroups for L where L::Features: LangFeatures<Groups = FeaturePresent>
{
}

/// Bound for command-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare command syntax present ([`Commands`](LangFeatures::Commands) `=`
/// [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasCommands` instead of spelling out the declaration equality.
pub trait LangHasCommands: Lang<Features: LangFeatures<Commands = FeaturePresent>> {}

impl<L: Lang> LangHasCommands for L where
    L::Features: LangFeatures<Commands = FeaturePresent>
{
}

/// Bound for comment-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare comment syntax present ([`Comments`](LangFeatures::Comments) `=`
/// [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasComments` instead of spelling out the declaration equality.
pub trait LangHasComments: Lang<Features: LangFeatures<Comments = FeaturePresent>> {}

impl<L: Lang> LangHasComments for L where
    L::Features: LangFeatures<Comments = FeaturePresent>
{
}

/// Bound for specials-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare the specials scan present ([`Specials`](LangFeatures::Specials) `=`
/// [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasSpecials` instead of spelling out the declaration equality.
pub trait LangHasSpecials: Lang<Features: LangFeatures<Specials = FeaturePresent>> {}

impl<L: Lang> LangHasSpecials for L where
    L::Features: LangFeatures<Specials = FeaturePresent>
{
}

/// Bound for code requiring the forbidden-character check: a [`Lang`] whose
/// [`Features`](Lang::Features) declare it present
/// ([`ForbiddenChars`](LangFeatures::ForbiddenChars) `=` [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasForbiddenChars` instead of spelling out the declaration equality.
pub trait LangHasForbiddenChars:
    Lang<Features: LangFeatures<ForbiddenChars = FeaturePresent>>
{
}

impl<L: Lang> LangHasForbiddenChars for L where
    L::Features: LangFeatures<ForbiddenChars = FeaturePresent>
{
}

/// Bound for scope-requiring code: a [`Lang`] whose [`Features`](Lang::Features)
/// declare the definition scope stack present ([`Scopes`](LangFeatures::Scopes) `=`
/// [`FeaturePresent`]).
///
/// Never implemented by hand: a blanket implementation covers every qualifying
/// [`Lang`] automatically, so the trait is purely a bound — write
/// `L: LangHasScopes` instead of spelling out the declaration equality.
pub trait LangHasScopes: Lang<Features: LangFeatures<Scopes = FeaturePresent>> {}

impl<L: Lang> LangHasScopes for L where L::Features: LangFeatures<Scopes = FeaturePresent>
{
}

/// Everything verified here is a type-level fact, checked when the test profile
/// *compiles*: instantiating the assertion functions (taking their function pointers)
/// forces the bound checks, and the `const` assertions evaluate at compile time. No
/// `#[test]` runs — there is nothing left to check at run time.
#[cfg(test)]
mod compile_checks {
    use super::*;
    use crate::scopes::SpecsProvider;
    use crate::state::TrivialLang;
    use crate::token::{CommandRule, CommentRule, GroupRule};
    use alloc::sync::Arc;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct FeatLang;
    impl TrivialLang for FeatLang {}

    /// The chosen `Store` GAT bounds; every planned per-feature storage payload type
    /// must satisfy them.
    fn assert_store_payload<T: Clone + fmt::Debug + Default>() {}

    // One instantiation per payload the per-feature storage will route through
    // `Store<T>`: the rules-block field types, their override (`Option<…>`) mirrors,
    // and the scope stack's provider list.
    const _: &[fn()] = &[
        // Rules-block payloads.
        assert_store_payload::<bool>,
        assert_store_payload::<Arc<str>>,
        assert_store_payload::<Vec<Arc<GroupRule<FeatLang>>>>,
        assert_store_payload::<Vec<Arc<CommandRule>>>,
        assert_store_payload::<Vec<Arc<CommentRule>>>,
        assert_store_payload::<Option<Arc<GroupRule<FeatLang>>>>,
        // Override payloads (`Option<…>` of each block field).
        assert_store_payload::<Option<bool>>,
        assert_store_payload::<Option<Arc<str>>>,
        assert_store_payload::<Option<Vec<Arc<GroupRule<FeatLang>>>>>,
        assert_store_payload::<Option<Vec<Arc<CommandRule>>>>,
        assert_store_payload::<Option<Vec<Arc<CommentRule>>>>,
        assert_store_payload::<Option<Option<Arc<GroupRule<FeatLang>>>>>,
        // The scope stack's provider list — the payload that rules `PartialEq`/`Eq`
        // out of the GAT bounds: `dyn SpecsProvider` has no equality.
        assert_store_payload::<Vec<Arc<dyn SpecsProvider<FeatLang>>>>,
    ];

    // The presence constants, marker by marker…
    const _: () = assert!(FeaturePresent::PRESENT);
    const _: () = assert!(!FeatureAbsent::PRESENT);

    // …and bundle by bundle.
    const fn every_member_present<F: LangFeatures>() -> bool {
        <F::Whitespace as FeaturePresence>::PRESENT
            && <F::Paragraphs as FeaturePresence>::PRESENT
            && <F::Groups as FeaturePresence>::PRESENT
            && <F::Commands as FeaturePresence>::PRESENT
            && <F::Comments as FeaturePresence>::PRESENT
            && <F::Specials as FeaturePresence>::PRESENT
            && <F::ForbiddenChars as FeaturePresence>::PRESENT
            && <F::Scopes as FeaturePresence>::PRESENT
    }
    const fn no_member_present<F: LangFeatures>() -> bool {
        !<F::Whitespace as FeaturePresence>::PRESENT
            && !<F::Paragraphs as FeaturePresence>::PRESENT
            && !<F::Groups as FeaturePresence>::PRESENT
            && !<F::Commands as FeaturePresence>::PRESENT
            && !<F::Comments as FeaturePresence>::PRESENT
            && !<F::Specials as FeaturePresence>::PRESENT
            && !<F::ForbiddenChars as FeaturePresence>::PRESENT
            && !<F::Scopes as FeaturePresence>::PRESENT
    }
    const _: () = assert!(every_member_present::<AllLangFeatures>());
    const _: () = assert!(no_member_present::<NoLangFeatures>());

    // The absent store is zero-sized; the present store is the payload itself
    // (`size_of` equality is the storage half; the literal below is the type half).
    const _: () = assert!(
        core::mem::size_of::<<FeatureAbsent as FeaturePresence>::Store<Vec<Arc<CommandRule>>>>()
            == 0
    );
    const _: () = assert!(
        core::mem::size_of::<<FeaturePresent as FeaturePresence>::Store<u64>>()
            == core::mem::size_of::<u64>()
    );

    /// Storage shaped like the planned per-feature fields: the type routes through
    /// the presence GAT.
    struct GroupStoreProbe<L: Lang> {
        payload: <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<Vec<u32>>,
    }

    /// The load-bearing normalization check on the subtrait spelling: under
    /// `L: LangHasGroups` the store type is *known* to be the payload type, so
    /// bounded generic code writes a plain struct literal…
    fn plain_literal_under_the_bound<L: LangHasGroups>() -> GroupStoreProbe<L> {
        GroupStoreProbe { payload: alloc::vec![1, 2, 3] }
    }

    /// …and reads the field back as the payload type.
    fn plain_read_under_the_bound<L: LangHasGroups>(probe: &GroupStoreProbe<L>) -> usize {
        probe.payload.len()
    }

    /// Unbounded generic code reaches any declaration as a compile-time constant.
    fn const_guard_unbounded<L: Lang>() -> bool {
        <L::Features as LangFeatures>::Groups::PRESENT
    }

    /// A trivial language (`Features = AllLangFeatures` via the blanket impl)
    /// satisfies all eight subtraits — the paragraphs-implies-whitespace edge
    /// included.
    fn requires_every_feature<L>()
    where
        L: LangHasWhitespace
            + LangHasParagraphs
            + LangHasGroups
            + LangHasCommands
            + LangHasComments
            + LangHasSpecials
            + LangHasForbiddenChars
            + LangHasScopes,
    {
    }

    // Monomorphize the probes for the concrete test language: taking the function
    // pointers checks that `FeatLang` satisfies the bounds.
    const _: fn() -> GroupStoreProbe<FeatLang> = plain_literal_under_the_bound::<FeatLang>;
    const _: fn(&GroupStoreProbe<FeatLang>) -> usize = plain_read_under_the_bound::<FeatLang>;
    const _: fn() -> bool = const_guard_unbounded::<FeatLang>;
    const _: fn() = requires_every_feature::<FeatLang>;
}
