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
    /// against one API.
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
    /// against one API.
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
    /// the per-instance door, whose default body calls this function.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    use crate::source::SourceSpan;
    use crate::state::{AllLangFeatures, ParsingState};
    use alloc::string::String;

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
}
