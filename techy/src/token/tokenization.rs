//! The [`Tokenization`] bundle — a language's tokenization declared as one type —
//! with the [`Token`] / [`StreamPosition`] projections and the standard
//! [`StdTokenization`].

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::source::Source;
use crate::state::Lang;

use super::reader::{StdStreamPosition, StdTokenReader, TokenReader};
use super::token::StdToken;

/// A language's tokenization, declared once at the type level: the token type its
/// readers produce, the stream-position type they hand out, and how the reader for a
/// parse over one source is built.
///
/// This is the type a language names as [`Lang::Tokenization`](crate::state::Lang::Tokenization).
/// It is implemented by a zero-sized type and never exists as a value — the trait
/// carries no method taking `self`. [`StdTokenization`] is the implementation this
/// crate provides; a language tokenized differently writes its own zero-sized type and
/// implements this trait for it.
///
/// The three members answer three separate questions, and splitting them is what lets
/// a driver stay written against a language without knowing which reader it will get:
/// what a token is (the [`Token`](Tokenization::Token) type), how a place in the stream
/// is named (the [`StreamPosition`](Tokenization::StreamPosition) type), and which
/// reader produces both ([`make_token_reader`](Tokenization::make_token_reader)).
///
/// # Implementing this trait
///
/// Declare a type with no fields, implement this trait for it, and name it as the
/// language's [`Lang::Tokenization`](crate::state::Lang::Tokenization). An
/// implementation that is generic over the language needs the bound
/// `L: Lang<Tokenization = MyTokenization>`:
///
/// ```
/// use std::sync::Arc;
/// use techy::core::{
///     Lang, StdStreamPosition, StdToken, StdTokenReader, TokenReader, Tokenization,
/// };
/// use techy::source::Source;
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyTokenization;
///
/// impl<L: Lang<Tokenization = MyTokenization>> Tokenization<L> for MyTokenization {
///     type Token = StdToken<L>;
///     type StreamPosition = StdStreamPosition;
///
///     fn make_token_reader<'s>(
///         source: &'s Arc<Source<L::SourceOrigin>>,
///     ) -> Box<dyn TokenReader<'s, L> + 's> {
///         // Any `TokenReader<'s, L>` goes here. This one reuses the standard reader,
///         // which is why the two types above are the standard ones.
///         Box::new(StdTokenReader::new(source))
///     }
/// }
///
/// #[derive(Debug, Clone, Copy)]
/// struct MyLang;
///
/// impl Lang for MyLang {
///     type Tokenization = MyTokenization;
///     // ... the remaining associated types as usual ...
/// #   type Features = techy::core::AllLangFeatures;
/// #   type GroupTypeId = u32;
/// #   type CallableTypeId = u32;
/// #   type ModeId = ();
/// #   type StateExt = ();
/// #   type Event = ();
/// #   type SessionExt = ();
/// #   type SourceOrigin = Option<String>;
/// #   type NodeExts = ();
/// #   type InvocationSyntax = ();
/// #   type Driver = techy::core::StdParseDriver;
/// #   fn make_node_ext(
/// #       _kind: &techy::core::node::NodeKind<Self>,
/// #       _span: &techy::source::SourceSpan<Self::SourceOrigin>,
/// #       _state: &Arc<techy::core::ParsingState<Self>>,
/// #       _children: techy::core::node::StagedChildren<'_, Self>,
/// #   ) -> Result<(), techy::core::node::NodeBuildError> {
/// #       Ok(())
/// #   }
/// }
/// ```
///
/// **Why the bound is needed.** [`Token<L>`](Token) and
/// [`StreamPosition<L>`](StreamPosition) are read off `L::Tokenization`. Until the
/// compiler knows that `L::Tokenization` is this very type, it cannot tell that the two
/// types declared here are the ones `L` works in, and a reader producing them — such as
/// [`StdTokenReader`](super::StdTokenReader), which produces
/// [`StdToken<L>`](super::StdToken) — is not accepted as a `TokenReader<'s, L>` (two
/// mismatched-type errors, one per member). Writing the bound the other way round,
/// `L::Tokenization: Tokenization<L, Token = …>`, does **not** work here: it names the
/// projection this impl is defining, so the impl compiles but the language that names
/// the type as its `Lang::Tokenization` then fails with an overflow error. Spelling the
/// self type `Self` in the bound says the same thing as the form above:
/// `impl<L: Lang<Tokenization = Self>> Tokenization<L> for MyTokenization`.
///
/// A tokenization written for one specific language does not need the bound, since
/// nothing is generic: `impl Tokenization<MyLang> for MyTokenization { … }`.
///
/// The `L::Tokenization: Tokenization<L, Token = …, StreamPosition = …>` form is the
/// right one *outside* an implementation of this trait — in a reader, a parser or a
/// helper that requires standard tokens but must still accept a language whose
/// tokenization type is its own. That is how [`StdTokenReader`](super::StdTokenReader)
/// states its own requirement.
pub trait Tokenization<L: Lang> {
    /// The token type this language's readers produce.
    ///
    /// **Opaque:** a token is a transient value that a
    /// [`TokenReader`](super::TokenReader) produces, construct parsers hold and pass
    /// around, and only a reader interprets. What a token *is* comes from
    /// [`token_kind`](super::TokenReader::token_kind) (a
    /// [`TokenKind`](super::TokenKind) view); where it is comes from the reader's span
    /// and position answers ([`source_span_of`](super::TokenReader::source_span_of),
    /// [`source_span_between`](super::TokenReader::source_span_between),
    /// [`position_at`](super::TokenReader::position_at)). Nothing else reads anything
    /// off a token. That is what lets a reader serve tokens from more than one source
    /// during one parse — a macro expander, say — while construct parsers stay written
    /// against one API. A reader does that at one nesting level only for a language
    /// that declares [`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)
    /// `= false` ([`TokenReader`](super::TokenReader) contract clause 8).
    ///
    /// The bounds are what the machinery needs of any token: `Clone` (parsers keep a
    /// token while they read on), `Debug` (diagnostics and test failures), `PartialEq`
    /// (equality compares what the reader recorded — test harnesses compare tokens
    /// produced by two readers over the same content), and `Send + Sync` (a token may
    /// travel with a parse that crosses threads).
    ///
    /// Languages tokenized by [`StdTokenReader`](super::StdTokenReader) — every
    /// language of this crate — use [`StdToken<L>`](super::StdToken), through
    /// [`StdTokenization`].
    type Token: Clone + fmt::Debug + PartialEq + Send + Sync;

    /// The type naming a place in this language's token stream — the value a
    /// [`TokenReader`](super::TokenReader) hands out from
    /// [`position_here`](super::TokenReader::position_here) and
    /// [`position_at`](super::TokenReader::position_at), and accepts back at
    /// [`move_to_position`](super::TokenReader::move_to_position).
    ///
    /// **Opaque, with equality only:** a construct parser holds a stream position and
    /// gives it back to the reader; it never interprets one. There is deliberately no
    /// way to build a position from a number, and no ordering — only equality, which is
    /// what "did the reader move?" needs. This is what lets a reader serving several
    /// sources during one parse name places its own way, while parsers stay written
    /// against one API — a reader serves several sources at one nesting level only for
    /// a language that declares
    /// [`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`
    /// ([`TokenReader`](super::TokenReader) contract clause 8), and the positions on
    /// the two sides of a source seam compare equal (that contract's *Seams* section).
    ///
    /// Languages tokenized by [`StdTokenReader`](super::StdTokenReader) use
    /// [`StdStreamPosition`](super::StdStreamPosition), through [`StdTokenization`].
    type StreamPosition: Clone + fmt::Debug + PartialEq + Eq + Send + Sync;

    /// Build the reader for one parse over `source`.
    ///
    /// Static — there is no receiver, because a `Tokenization` is a type and never a
    /// value. A reader that needs runtime data reads it from the parsing state passed
    /// to [`peek`](super::TokenReader::peek), or is built instead by a driver
    /// overriding
    /// [`ParseDriver::make_token_reader`](crate::engine::ParseDriver::make_token_reader) —
    /// the per-instance override, whose default body calls this function.
    fn make_token_reader<'s>(
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's>;
}

/// The token type of `L` — the projection through
/// [`Lang::Tokenization`](crate::state::Lang::Tokenization).
///
/// Spell a language's token type `Token<L>`; the contract it satisfies is documented on
/// [`Tokenization::Token`].
pub type Token<L> = <<L as Lang>::Tokenization as Tokenization<L>>::Token;

/// The stream-position type of `L` — the projection through
/// [`Lang::Tokenization`](crate::state::Lang::Tokenization).
///
/// Spell a language's stream-position type `StreamPosition<L>`; the contract it
/// satisfies is documented on [`Tokenization::StreamPosition`].
pub type StreamPosition<L> = <<L as Lang>::Tokenization as Tokenization<L>>::StreamPosition;

/// The standard tokenization: [`StdToken<L>`](super::StdToken) tokens,
/// [`StdStreamPosition`](super::StdStreamPosition) positions, and readers built by
/// [`StdTokenReader::new`](super::StdTokenReader::new).
///
/// This is what [`TrivialLang`](crate::state::TrivialLang) and every language of this
/// crate declare as their [`Lang::Tokenization`](crate::state::Lang::Tokenization).
#[derive(Debug, Clone, Copy)]
pub struct StdTokenization;

// The bound is `Tokenization = StdTokenization`, not the equality on `Token` /
// `StreamPosition`: the latter form is a cycle through the very projection this impl
// defines, and trips the recursion limit (E0275). Everywhere *else* — code that must
// also accept a language whose own `Tokenization` produces standard tokens — the
// equality form is the one to write; see [§dd-dr:tokenization].
impl<L: Lang<Tokenization = StdTokenization>> Tokenization<L> for StdTokenization {
    type Token = StdToken<L>;
    type StreamPosition = StdStreamPosition;

    fn make_token_reader<'s>(
        source: &'s Arc<Source<L::SourceOrigin>>,
    ) -> Box<dyn TokenReader<'s, L> + 's> {
        Box::new(StdTokenReader::new(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Language, ParseDriver, StdParseDriver};
    use crate::node::{NodeBuildError, NodeKind, StagedChildren};
    use crate::source::{SourcePos, SourceSpan};
    use crate::state::{AllLangFeatures, ParsingState};
    use alloc::string::String;
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::super::{TokenEdge, TokenKind, TokenResult};

    /// A tokenization type of a language's own, working in the standard token and
    /// stream-position types — the "reader over standard tokens" pattern. It is what
    /// the equality bounds elsewhere in the crate
    /// (`L::Tokenization: Tokenization<L, Token = StdToken<L>, …>`) exist for: a
    /// `Lang<Tokenization = StdTokenization>` bound would shut such a language out of
    /// `StdTokenReader` and of the ready-made driver.
    #[derive(Debug, Clone, Copy)]
    struct OwnTokenization;

    impl<L: Lang<Tokenization = OwnTokenization>> Tokenization<L> for OwnTokenization {
        type Token = StdToken<L>;
        type StreamPosition = StdStreamPosition;

        fn make_token_reader<'s>(
            source: &'s Arc<Source<L::SourceOrigin>>,
        ) -> Box<dyn TokenReader<'s, L> + 's> {
            // The wrapper case in miniature: the standard reader serves this language
            // although the language's tokenization type is its own.
            Box::new(StdTokenReader::new(source))
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct OwnLang;

    /// An empty driver impl — every `ParseDriver` item defaulted, tokenization
    /// included.
    #[derive(Debug, Clone, Copy)]
    struct BareDriver;

    impl ParseDriver<OwnLang> for BareDriver {}

    impl Lang for OwnLang {
        type Features = AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = OwnTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = BareDriver;

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: StagedChildren<'_, Self>,
        ) -> Result<(), NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn a_language_with_its_own_tokenization_over_standard_tokens_parses() {
        // The defaulted `make_token_reader` builds the reader `OwnTokenization` names,
        // and `StdTokenReader` serves this language although its tokenization type is
        // not `StdTokenization`.
        let language: Language<OwnLang> =
            Language::new(BareDriver, ParsingState::lang_initial().expect("seed state"));
        let result = language.parse("hello").expect("parse");
        assert_eq!(result.tree.root().span_content(), "hello");
    }

    #[test]
    fn the_ready_made_driver_carries_no_tokenization_bound() {
        // Compilation is the assertion: `StdParseDriver` serves a language whose
        // tokenization type is its own.
        fn assert_drives<D: ParseDriver<OwnLang>>() {}
        assert_drives::<StdParseDriver<(), Option<String>>>();
    }

    // --- the driver's per-instance override --------------------------------------
    //
    // `CountingLang` declares `StdTokenization`, so its language-side reader is a plain
    // `StdTokenReader`. `CountingDriver` overrides `make_token_reader` and hands out a
    // reader built from a counter the *driver instance* holds; the counter is what
    // shows which of the two readers served the parse.

    /// A reader that answers every question through an inner `StdTokenReader` and
    /// counts the tokens it was asked for.
    struct CountingReader<'s> {
        inner: StdTokenReader<'s>,
        peeks: &'s AtomicUsize,
    }

    impl<'s> CountingReader<'s> {
        fn inner(&self) -> &dyn TokenReader<'s, CountingLang> {
            &self.inner
        }
        fn inner_mut(&mut self) -> &mut dyn TokenReader<'s, CountingLang> {
            &mut self.inner
        }
    }

    impl<'s> TokenReader<'s, CountingLang> for CountingReader<'s> {
        fn peek(
            &mut self,
            state: &Arc<ParsingState<CountingLang>>,
        ) -> TokenResult<CountingLang, StdToken<CountingLang>> {
            self.peeks.fetch_add(1, Ordering::Relaxed);
            self.inner_mut().peek(state)
        }
        fn move_to(&mut self, tok: &StdToken<CountingLang>, edge: TokenEdge) {
            self.inner_mut().move_to(tok, edge);
        }
        fn move_to_position(&mut self, at: &StdStreamPosition) {
            self.inner_mut().move_to_position(at);
        }
        fn token_kind<'t>(
            &self,
            tok: &'t StdToken<CountingLang>,
        ) -> TokenKind<'t, CountingLang>
        where
            's: 't,
        {
            self.inner().token_kind(tok)
        }
        fn source_span_between(
            &self,
            tok: &StdToken<CountingLang>,
            a: TokenEdge,
            b: TokenEdge,
        ) -> SourceSpan {
            self.inner().source_span_between(tok, a, b)
        }
        fn position_here(&self) -> StdStreamPosition {
            self.inner().position_here()
        }
        fn position_at(
            &self,
            tok: &StdToken<CountingLang>,
            edge: TokenEdge,
        ) -> StdStreamPosition {
            self.inner().position_at(tok, edge)
        }
        fn source_position_at(&self, at: &StdStreamPosition) -> SourcePos {
            self.inner().source_position_at(at)
        }
        fn source_span_within(
            &self,
            begin: &StdStreamPosition,
            end: &StdStreamPosition,
        ) -> Option<SourceSpan> {
            self.inner().source_span_within(begin, end)
        }
        fn source_span_describing(
            &self,
            begin: &StdStreamPosition,
            end: &StdStreamPosition,
        ) -> SourceSpan {
            self.inner().source_span_describing(begin, end)
        }
    }

    /// Per-parse counting is not what a real driver holds (drivers carry configuration,
    /// not parse state); here it is the probe that says whose reader ran.
    #[derive(Debug, Default)]
    struct CountingDriver {
        peeks: Arc<AtomicUsize>,
    }

    impl ParseDriver<CountingLang> for CountingDriver {
        fn make_token_reader<'s>(
            &'s self,
            source: &'s Arc<Source<Option<String>>>,
        ) -> Box<dyn TokenReader<'s, CountingLang> + 's> {
            Box::new(CountingReader { inner: StdTokenReader::new(source), peeks: &self.peeks })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct CountingLang;

    impl Lang for CountingLang {
        type Features = AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = CountingDriver;

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: StagedChildren<'_, Self>,
        ) -> Result<(), NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn a_driver_override_installs_its_own_reader_over_the_language_declaration() {
        let peeks = Arc::new(AtomicUsize::new(0));
        let language: Language<CountingLang> = Language::new(
            CountingDriver { peeks: Arc::clone(&peeks) },
            ParsingState::lang_initial().expect("seed state"),
        );
        let result = language.parse("hi").expect("parse");

        // The parse ran, and every token it read came through the driver's reader —
        // the language's own `StdTokenization` reader never served it.
        assert_eq!(result.tree.root().span_content(), "hi");
        assert!(peeks.load(Ordering::Relaxed) > 0, "the driver's reader never ran");
    }
}
