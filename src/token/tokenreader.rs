//! Token reader trait and implementations.

use crate::source::SourceLocation;
use crate::error::TokenizerError;
use super::token::Token;
use crate::parser::LanguageSpecification;


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
pub trait TokenReader<'src, LS: LanguageSpecification> : 'src {

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
    fn peek_token(&mut self, parsing_state: &LS::ParsingState) -> Result<'src, Option<Token<'src>>>;

    /// Parse a token at the current position and advance the position past it.
    ///
    /// Same as `peek_token()`, but also updates the internal position pointer.  Returns
    /// Ok(None) if we reached the end of the token stream.
    fn next_token(&mut self, parsing_state: &LS::ParsingState)
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
    fn cur_pos(&self) -> LS::SourceLocation<'src>;
}








#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenPrefixType {
    GroupOpen,
    GroupClose,
    // if ambiguous (exists as open or close delimiter/might depend on context):
    GroupOpenOrClose,
}





/// Helper function to read a prefix of matching characters from a string.
#[inline]
fn read_prefix_matching_chars<'a, 'b>(matching_chars: &'a str, s: &'b str) -> (&'b str, usize) {
    let match_bytes_len = s
        .char_indices()
        .find(|(_, c)| !matching_chars.contains(*c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let prefix = &s[..match_bytes_len]; // Zero-copy slice into original string
    (prefix, match_bytes_len)
}


trait TokenParserImplementer<'src, LS: LanguageSpecification, TokenReader : TokenReader<'src, LS>> {
    
    fn detect_whitespace<'a>(
        &self,
        token_reader : & TokenReader,
        parsing_state : & LS::ParsingState
    ) -> TokenReader::Result {
        if !parsing_state.data().whitespace.enable_whitespace_handling {
            return Ok(("", 0));
        }
        let allowed = parsing_state.data().whitespace.whitespace_chars;
        Ok(read_prefix_matching_chars(allowed, s))
    }

    fn detect_macro<'a>(
        &self,
        token_reader : & TokenReader,
        parsing_state : & LS::ParsingState
    ) -> Result<Option<(&'a str, usize)>> {
        if ! parsing_state.data.macros.enable_macros {
            return Ok(None);
        }
        match s.chars().next() {
            Some(c) if c == self.data.macros.macro_escape_char => {
                // indeed a macro.
                if s.len() <= 1 {
                    // string ends before macro name!
                    Err(TokenizerError::new(
                        "Expected macro name, got end of string",

                        Token::new()
                    ))
                }
                let allowed = self.data.macros.macro_alpha_chars();
                let alpha = read_prefix_matching_chars(allowed, &s[1..])
                if alpha.len() {
                    let end = 1 + alpha.len()
                    Some(&s[:end], end)
                } else {

                }
            }
            Some(_) => {
                None
            }
            None => {
                // This is impossible normally, since the tokenizer/parser wouldn't call
                // detect_macro() if we're already at the end of the token stream.
                None
            }
        }
    }

    fn detect_delimiters_prefix<'a>(&self, s : &'a str)
     -> Option<(&'a str, usize, TokenPrefixType)> {
        if ! self.groups.enable_groups {
            return None;
        }
        match cached_prefix_strings.iter().find(|&x| s.starts_with(x.0)) {
            Some((delim, dtype)) => {
                Some(delim, delim.len(), dtype)
            }
            None => {
                None
            }
        }
    }

    fn detect_comment_start<'a>(&self, s : &'a str) -> Option<(&'a str, usize)> {
        if ! self.comments.enable_comments {
            return None
        }
        if s.starts_with(self.comments.comment_start) {
            let length = self.comments.comment_start.len();
            return Some((s[:length], length))
        }
        None
    }

    fn detect_multi_newline_paragraphs_in_whitespace(&self, ws : &'a str) -> Option<(usize, usize)> {
        const NEWLINE : char = '\n';

        if ! self.multi_newline_paragraphs.enable_multi_newline_paragraphs {
            return None;
        }

        // Count newlines and track positions
        let mut first_newline_pos = None;
        let mut last_newline_pos = None;
        let mut newline_count = 0;

        for (i, ch) in ws.char_indices() {
            if ch == NEWLINE {
                if first_newline_pos.is_none() {
                    first_newline_pos = Some(i);
                }
                last_newline_pos = Some(i);
                newline_count += 1;
            }
        }

        if newline_count >= 2 {
            // Return range from first newline to one past the last newline
            let start = first_newline_pos.unwrap();
            let end = last_newline_pos.unwrap() + NEWLINE.len_utf8();
            Some((start, end))
        } else {
            None
        }
    }

    fn detect_forbidden(&self, s: &'a str) -> Option<(&'a str, usize)> {
        let forbidden = self.forbidden.forbidden_characters();
        let (prefix, len) = read_prefix_matching_chars(forbidden, s);
        if len > 0 {
            Some((prefix, len))
        } else {
            None
        }
    }
}