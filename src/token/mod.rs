//! Token types and tokenization for LaTeX source code.
//!
//! This module defines the token types that make up LaTeX syntax and provides
//! traits for reading tokens from a source.

//pub mod reader;

use log::warn;

use crate::source::SourceLocation;

use std::fmt;


// // to move elsewhere
// pub trait LanguageSpecification {
//     //type Token;
// }



#[inline]
fn read_prefix_allowed_chars(allowed : & 'a str, s : & 'b str)
-> (& 'b str, int) {
    let match_len = s.char_indices()
        .find(|(_, c)| !allowed.contains(*c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let prefix = &s[..match_len]; // Zero-copy slice into original string
    return (prefix, match_len);
}


#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenCachedPrefixType {
    GroupOpen,
    GroupClose,
    Special,
}

pub struct TokenizationState {
    whitespace_chars: String,

    enable_groups: bool,
    group_delimiters: Vec<(String, String)>,

    enable_macros: bool,
    macro_escape_char: char,
    macro_alpha_chars: String,

    enable_environments: bool,

    enable_specials: bool,
    specials_strings: Vec<String>,

    enable_comments: bool,
    comment_chars: String,

    enable_multi_newline_paragraphs: bool,

    forbidden_characters: String,
    forbidden_specials: Vec<String>,

    // Cached data for fast prefix matching - parallel arrays
    // Stores owned copies sorted by decreasing length for greedy matching
    cached_prefix_strings: Vec<String>,
    cached_prefix_types: Vec<TokenCachedPrefixType>,
}

impl Default for TokenizationState {
    fn default() -> Self {
        let mut ts = Self {
            // Standard LaTeX whitespace: space, tab, newline, carriage return
            whitespace_chars: " \t\n".to_string(),

            // Groups enabled with standard braces
            enable_groups: true,
            group_delimiters: vec![("{".to_string(), "}".to_string())],

            // Macros enabled with backslash and standard alphabetic chars
            enable_macros: true,
            macro_escape_char: '\\',
            macro_alpha_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),

            // Environments enabled (requires macros)
            enable_environments: true,

            // Specials enabled with standard LaTeX special chars
            enable_specials: true,
            specials_strings: vec![],

            // Comments enabled with %
            enable_comments: true,
            comment_chars: "%".to_string(),

            // Forbidden characters - weird ascii space chars, maybe forbid entire
            // nonprintable range other than \t and \n?
            forbidden_characters: "\r\v\b".to_string(),
            forbidden_specials: vec![],

            // Multi-newline paragraph breaks enabled
            enable_multi_newline_paragraphs: true,

            // Will be populated by update_cached_prefix_strings_to_test()
            cached_prefix_strings: vec![],
            cached_prefix_types: vec![],
        };
        ts.update_cached_prefix_strings_to_test();
        ts
    }
}

impl TokenizationState {

    fn update_cached_prefix_strings_to_test(&mut self) {
        use std::collections::HashSet;

        let mut items: Vec<(String, TokenCachedPrefixType)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Helper to add unique items
        let mut add_unique = |s: &str, typ: TokenCachedPrefixType| {
            if !s.is_empty() && !seen.contains(s) {
                seen.insert(s.to_string());
                items.push((s.to_string(), typ));
            }
        };

        // Add group open & close delimiters
        if self.enable_groups {
            for (open, close) in &self.group_delimiters {
                add_unique(open, TokenCachedPrefixType::GroupOpen);
                add_unique(close, TokenCachedPrefixType::GroupClose);
            }
        }

        // Add special strings
        if self.enable_specials {
            for special in &self.specials_strings {
                add_unique(special, TokenCachedPrefixType::Special);
            }
        }

        // Sort by length (descending) - longer strings first to match greedily
        items.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        // Split into parallel arrays for better cache locality
        self.cached_prefix_strings = items.iter().map(|(s, _)| s.clone()).collect();
        self.cached_prefix_types = items.iter().map(|(_, t)| *t).collect();
    }

    // Getters for read-only access
    pub fn whitespace_chars(&self) -> &str {
        &self.whitespace_chars
    }

    pub fn forbidden_characters(&self) -> &str {
        &self.forbidden_characters
    }

    pub fn enable_groups(&self) -> bool {
        self.enable_groups
    }

    pub fn group_delimiters(&self) -> &[(String, String)] {
        &self.group_delimiters
    }

    pub fn enable_macros(&self) -> bool {
        self.enable_macros
    }

    pub fn macro_escape_char(&self) -> char {
        self.macro_escape_char
    }

    pub fn macro_alpha_chars(&self) -> &str {
        &self.macro_alpha_chars
    }

    pub fn enable_environments(&self) -> bool {
        self.enable_environments
    }

    pub fn enable_specials(&self) -> bool {
        self.enable_specials
    }

    pub fn specials_strings(&self) -> &[String] {
        &self.specials_strings
    }

    pub fn enable_comments(&self) -> bool {
        self.enable_comments
    }

    pub fn comment_chars(&self) -> &str {
        &self.comment_chars
    }

    pub fn enable_multi_newline_paragraphs(&self) -> bool {
        self.enable_multi_newline_paragraphs
    }


    fn read_macro_alpha_chars_prefix(&self, s: &str) -> (&str, usize) {
        let allowed = self.macro_alpha_chars();
        read_prefix_allowed_chars(allowed, s)
    }

    fn read_whitespace(&self, s: &str) -> (&str, usize) {
        let allowed = self.whitespace_chars();
        read_prefix_allowed_chars(allowed, s)
    }

    /// Create a derived tokenization state with modified fields.
    /// Only updates the cached prefix list if fields affecting it are changed.
    pub fn derive(&self) -> TokenizationStateBuilder {
        TokenizationStateBuilder {
            base: self.clone(),
            cache_needs_update: false,
        }
    }
}

/// Builder for creating derived TokenizationState objects.
struct TokenizationStateBuilder {
    base: TokenizationState,
    cache_needs_update: bool,
}

impl TokenizationStateBuilder {
    pub fn whitespace_chars(mut self, value: String) -> Self {
        self.base.whitespace_chars = value;
        self
    }

    pub fn forbidden_characters(mut self, value: String) -> Self {
        self.base.forbidden_characters = value;
        self
    }

    pub fn enable_groups(mut self, value: bool) -> Self {
        if self.base.enable_groups != value {
            self.base.enable_groups = value;
            self.cache_needs_update = true;
        }
        self
    }

    pub fn group_delimiters(mut self, value: Vec<(String, String)>) -> Self {
        self.base.group_delimiters = value;
        self.cache_needs_update = true;
        self
    }

    pub fn enable_macros(mut self, value: bool) -> Self {
        self.base.enable_macros = value;
        self
    }

    pub fn macro_escape_char(mut self, value: char) -> Self {
        self.base.macro_escape_char = value;
        self
    }

    pub fn macro_alpha_chars(mut self, value: String) -> Self {
        self.base.macro_alpha_chars = value;
        self
    }

    pub fn enable_environments(mut self, value: bool) -> Self {
        self.base.enable_environments = value;
        self
    }

    pub fn enable_specials(mut self, value: bool) -> Self {
        if self.base.enable_specials != value {
            self.base.enable_specials = value;
            self.cache_needs_update = true;
        }
        self
    }

    pub fn specials_strings(mut self, value: Vec<String>) -> Self {
        self.base.specials_strings = value;
        self.cache_needs_update = true;
        self
    }

    pub fn enable_comments(mut self, value: bool) -> Self {
        self.base.enable_comments = value;
        self
    }

    pub fn comment_chars(mut self, value: String) -> Self {
        self.base.comment_chars = value;
        self
    }

    pub fn enable_multi_newline_paragraphs(mut self, value: bool) -> Self {
        self.base.enable_multi_newline_paragraphs = value;
        self
    }

    pub fn forbidden_specials(mut self, value: Vec<String>) -> Self {
        self.base.forbidden_specials = value;
        self
    }

    /// Build the final TokenizationState, updating cache only if needed.
    pub fn build(mut self) -> TokenizationState {
        if self.cache_needs_update {
            self.base.update_cached_prefix_strings_to_test();
        }
        self.base
    }
}

impl Clone for TokenizationState {
    fn clone(&self) -> Self {
        Self {
            whitespace_chars: self.whitespace_chars.clone(),
            enable_groups: self.enable_groups,
            group_delimiters: self.group_delimiters.clone(),
            enable_macros: self.enable_macros,
            macro_escape_char: self.macro_escape_char,
            macro_alpha_chars: self.macro_alpha_chars.clone(),
            enable_environments: self.enable_environments,
            enable_specials: self.enable_specials,
            specials_strings: self.specials_strings.clone(),
            enable_comments: self.enable_comments,
            comment_chars: self.comment_chars.clone(),
            enable_multi_newline_paragraphs: self.enable_multi_newline_paragraphs,
            forbidden_characters: self.forbidden_characters.clone(),
            forbidden_specials: self.forbidden_specials.clone(),
            cached_prefix_strings: self.cached_prefix_strings.clone(),
            cached_prefix_types: self.cached_prefix_types.clone(),
        }
    }
}



/// Types of tokens in LaTeX.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// Regular character(s) (text content).
    Char { content: String },

    /// A macro/command (e.g., `\textbf`).
    Macro { macro_name: String, post_space: String },

    /// Beginning of an environment (e.g., `\begin{equation}`).
    BeginEnvironment { environment_name: String },

    /// End of an environment (e.g., `\end{equation}`).
    EndEnvironment { environment_name: String },

    /// A comment (typically starting with `%`).
    Comment { comment: String, post_space: String },

    /// Typically an opening brace `{`
    GroupOpen { delimiter: String },

    /// Typically a closing brace `}`.
    GroupClose { delimiter: String },

    /// Paragraph break marker (space with multiple newlines, from first
    /// newline to final space after final newline), with possible pre_space
    /// before first newline.
    NewlinesParagraphBreak { space_chars: String },

    /// Special characters with meaning in LaTeX (e.g., `&`, `~`, `#`).
    Specials { specials_chars: String },
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Char { chars } => write!(f, "Char(‘{}’)", chars),
            TokenType::Macro { macro_name, post_space } => {
                if post_space.is_empty() {
                    write!(f, "Macro(\\{})", macro_name)
                } else {
                    write!(f, "Macro(\\{}, post_space={:?})", macro_name, post_space)
                }
            }
            TokenType::BeginEnvironment { environment_name } => {
                write!(f, "BeginEnvironment(‘{}’)", environment_name)
            }
            TokenType::EndEnvironment { environment_name } => {
                write!(f, "EndEnvironment(‘{}’)", environment_name)
            }
            TokenType::Comment { comment, post_space } => {
                if post_space.is_empty() {
                    write!(f, "Comment(‘{}’)", comment)
                } else {
                    write!(f, "Comment(‘{}’, post_space={:?})", comment, post_space)
                }
            }
            TokenType::BraceOpen { delimiter } => {
                write!(f, "BraceOpen(‘{}’)", delimiter)
            }
            TokenType::BraceClose { delimiter } => {
                write!(f, "BraceClose(‘{}’)", delimiter)
            }
            TokenType::NewlinesParagraphBreak { space_chars } => {
                write!(f, "NewlinesParagraphBreak({:?})", space_chars)
            }
            TokenType::Specials { specials_chars } => {
                write!(f, "Specials(‘{}’)", specials_chars)
            }
        }
    }
}

/// A token with source location and whitespace information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The type of token.
    pub token_type: TokenType,
    /// The source location where this token appears.
    pub pos: SourceLocation,
    /// Whitespace that appeared before this token.
    pub pre_space: String,
}

impl Token {
    /// Create a new token.
    pub fn new(token_type: TokenType, pos: SourceLocation, pre_space: String)
    -> Self {
        Self {
            token_type,
            pos,
            pre_space,
        }
    }
}

pub type Result<'src, T> = std::Result<T, ParseError<'src>::TokenizerError>;

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
/// Parsers can obtain character-level access to input stream (effectively bypassing
/// tokenization) by suitable choices in TokenizationState (no space chars, disable
/// macros, environments, specials, groups, etc.).
pub trait TokenReader<'src>
{
    /// Move the internal position pointer to the position of the given token.
    ///
    /// After calling this, `peek_token()` or `next_token()` should read the given
    /// token again.
    ///
    /// If `rewind_pre_space` is true, the position is set to include the whitespace
    /// that precedes the token; if false, the position is set to the actual token
    /// after the preceding whitespace.
    fn move_to_token(&mut self, tok: &Token, rewind_pre_space: bool);

    /// Move the internal position pointer immediately past the given token.
    ///
    /// After calling this, `peek_token()` or `next_token()` should return the
    /// token that follows `tok` in the input stream.
    ///
    /// If `fastforward_post_space` is true, any whitespace that follows the token
    /// (for macro and comment tokens) is also skipped.
    fn move_past_token(&mut self, tok: &Token, fastforward_post_space: bool);

    /// Parse a single token at the current position without advancing the position.
    ///
    /// The internal position pointer is not updated. Subsequent calls with the same
    /// parsing state should return the same token.
    ///
    /// Returns `Err` with an end-of-stream error if there are no remaining tokens.
    fn peek_token(&mut self, tok_state: &TokenizationState) -> Result<Token>;

    /// Convenience method that calls `peek_token()` but returns `Ok(None)` instead
    /// of an error on end-of-stream.
    fn peek_token_or_none(&mut self, tok_state: &TokenizationState)
        -> Result<Option<Token>>
    {
        match self.peek_token(tok_state) {
            Ok(tok) => Ok(Some(tok)),
            Err(e) if e.is_end_of_stream() => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Parse a token at the current position and advance the position past it.
    ///
    /// Same as `peek_token()`, but also updates the internal position pointer.
    fn next_token(&mut self, tok_state: &TokenizationState) -> Result<Token> {
        let tok = self.peek_token(tok_state)?;
        self.move_past_token(&tok, true);
        Ok(tok)
    }

    /// Return the current internal position pointer's state.
    fn cur_pos(&self) -> SourceLocation<'src>;
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_display() {
        let token_type = TokenType::Macro {
            macro_name: "textbf".to_string(),
            post_space: String::new(),
        };
        assert_eq!(format!("{}", token_type), "Macro(\\textbf)");

        let token_type_with_space = TokenType::Macro {
            macro_name: "textbf".to_string(),
            post_space: " ".to_string(),
        };
        assert_eq!(format!("{}", token_type_with_space), "Macro(\\textbf, post_space=\" \")");
    }
}
