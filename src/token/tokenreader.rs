//! Token reader trait and implementations.

use crate::source::SourceLocation;
use crate::error::TokenizerError;
use super::token::Token;
use super::tokenizationstate::TokenizationState;


/// Result type for tokenization operations.
pub type Result<'src, T> = std::result::Result<T, TokenizerError<'src>>;


/// Trait for reading tokens from a source.
///
/// A token reader transforms input characters into tokens and maintains an internal
/// position pointer. This trait mirrors the API of pylatexenc's LatexTokenReaderBase.
///
/// Token readers should at minimum implement:
/// - `peek_token()` - parse token without advancing position
/// - `move_to_token()` - rewind to a specific token's position
/// - `move_past_token()` - advance past a specific token
/// - `cur_pos()` - get current position
///
/// The `'src` lifetime represents the source text and is independent of the TokenReader's
/// lifetime. Tokens reference the source text, not the TokenReader itself.
///
/// Parsers can obtain character-level access to input stream (effectively bypassing
/// tokenization) by suitable choices in TokenizationState (no space chars, disable
/// macros, environments, specials, groups, etc.).
pub trait TokenReader<'src>: 'src {
    /// Move the internal position pointer to the position of the given token.
    ///
    /// After calling this, `peek_token()` or `next_token()` should read the given
    /// token again.
    ///
    /// If `rewind_pre_space` is true, the position is set to include the whitespace
    /// that precedes the token; if false, the position is set to the actual token
    /// after the preceding whitespace.
    fn move_to_token(&mut self, tok: &Token<'src>, rewind_pre_space: bool);

    /// Move the internal position pointer immediately past the given token.
    ///
    /// After calling this, `peek_token()` or `next_token()` should return the
    /// token that follows `tok` in the input stream.
    ///
    /// If `fastforward_post_space` is true, any whitespace that follows the token
    /// (for macro and comment tokens) is also skipped.
    fn move_past_token(&mut self, tok: &Token<'src>, fastforward_post_space: bool);

    /// Parse a single token at the current position without advancing the position.
    ///
    /// The internal position pointer is not updated. Subsequent calls with the same
    /// parsing state should return the same token.
    ///
    /// Returns `Err` if there is an error fetching tokens (IO error, whatever).
    /// Returns `Ok(None)` if we reached the end of stream.
    /// This behavior is similar to pylatexenc's peek_token_or_none().
    fn peek_token(&mut self, tok_state: &TokenizationState) -> Result<'src, Option<Token<'src>>>;

    /// Parse a token at the current position and advance the position past it.
    ///
    /// Same as `peek_token()`, but also updates the internal position pointer.  Returns
    /// Ok(None) if we reached the end of the token stream.
    fn next_token(&mut self, tok_state: &TokenizationState)
     -> Result<'src, Option<Token<'src>>> {
        match self.peek_token(tok_state)? {
            None => Ok(None),
            Some(token) => {
                self.move_past_token(&token, true);
                Ok(Some(token))
            }
        }
    }

    /// Return the current internal position pointer's state.
    fn cur_pos(&self) -> SourceLocation<'src>;
}
