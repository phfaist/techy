//! Token reader implementation for string sources.

use super::{Token, TokenReader, TokenType};
use crate::source::Source;

/// A token reader that reads from a string.
pub struct StringTokenReader<'src, LS: LanguageSpecification>> {
    source: & 'src LS::Source,
    position: usize,
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




impl StringTokenReader<'src, LS: LanguageSpecification> {
    /// Create a new token reader for the given source.
    pub fn new(source: & 'src LS::Source) -> Self {
        Self {
            source,
            position: 0,
            tolerant_parsing: false,
        }
    }
    pub fn with_tolerant_parsing(mut self, bool tolerant_parsing) -> Self {
        self.tolerant_parsing = tolerant_parsing
        self
    }

    fn jump_to_position(&mut self, usize position) {
        self.position = position;
    }


    ................
    fn detect_whitespace<'a>(&self, s: &'a str) -> TokenReaderResult<(&'a str, usize)> {
        if !self.whitespace.enable_whitespace_handling {
            return Ok(("", 0));
        }
        let allowed = self.whitespace.whitespace_chars();
        Ok(read_prefix_matching_chars(allowed, s))
    }

    fn detect_macro<'a>(&self, s: &'a str) -> TokenReaderResult<Option<(&'a str, usize)>> {
        if ! self.macros.enable_macros {
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


impl TokenReader<'src, LS: LanguageSpecification>
for StringTokenReader<'src, LS: LanguageSpecification> {

    pub fn cur_pos(&self) -> LS::SourceLocation<'src>
    {
        self.source.make_pos(self.position, self.position)
    }


    pub fn move_to_token(&mut self, tok: &Token<'src>, rewind_pre_space: bool) {
        if rewind_pre_space {
            self.jump_to_position( tok.pos.start - tok.pre_space.len() );
        } else {
            self.jump_to_position( tok.pos.start );
        }
    }

    pub fn move_past_token(&mut self, tok: &Token<'src>, fastforward_post_space: bool) {
        if !fastforward_post_space {
            self.jump_to_position( tok.pos.end );
        } else {
            let post_space_len = match tok.get_post_space() {
                Some(s) => s.len(),
                None => 0
            };
            self.jump_to_position( tok.pos.end + post_space_len );
        }
    }

    pub fn peek_token(&mut self, parsing_state: &LS::ParsingState)
     -> Result<'src, Option<Token<'src>>> {
        // Returns `Err` if there is an error fetching tokens (IO error, whatever).
        // Returns `Ok(None)` if we reached the end of stream.
        // This behavior is similar to pylatexenc's peek_token_or_none().

        let (pre_space, pre_space_len) = self.impl_detect_whitespace(parsing_state);
        
        if let Some((paranls_start, paranls_end)) =
            self.impl_detect_multi_newline_paragraphs_in_whitespace(pre_space) {
            // there is actually a new paragraph separator (multiple newlines)
            return Ok(Some(Token::new(
                TokenType::NewlinesParagraphBreak(pre_space[paranls_start:paranls_end]),
                self.source.make_pos(self.position + paranls_start,
                                     self.position + paranls_end),
                pre_space[:paranls_start],
            )))
        }

        // read pre_space, no new paragraph at this point
        // is there anything left to read?
        if self.position >= self.source.content.len() {
            return Ok(None)
        }



    }
}







#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let mut reader = StringTokenReader::new("hello".to_string());
        let token = reader.next_token().unwrap().unwrap();
        assert!(matches!(token.token_type, TokenType::Char(_)));
    }

    #[test]
    fn test_macro() {
        let mut reader = StringTokenReader::new("\\textbf".to_string());
        let token = reader.next_token().unwrap().unwrap();
        match token.token_type {
            TokenType::Macro(name) => assert_eq!(name, "textbf"),
            _ => panic!("Expected macro token"),
        }
    }

    #[test]
    fn test_environment() {
        let mut reader = StringTokenReader::new("\\begin{equation}".to_string());
        let token = reader.next_token().unwrap().unwrap();
        match token.token_type {
            TokenType::BeginEnvironment(name) => assert_eq!(name, "equation"),
            _ => panic!("Expected begin environment token"),
        }
    }

    #[test]
    fn test_math_mode() {
        let mut reader = StringTokenReader::new("$x$".to_string());
        let token = reader.next_token().unwrap().unwrap();
        assert!(matches!(token.token_type, TokenType::MathModeInline));
    }

    #[test]
    fn test_comment() {
        let mut reader = StringTokenReader::new("% comment\n".to_string());
        let token = reader.next_token().unwrap().unwrap();
        match token.token_type {
            TokenType::Comment(text) => assert_eq!(text.trim(), "comment"),
            _ => panic!("Expected comment token"),
        }
    }

    #[test]
    fn test_braces() {
        let mut reader = StringTokenReader::new("{}".to_string());
        
        let token1 = reader.next_token().unwrap().unwrap();
        assert!(matches!(token1.token_type, TokenType::BraceOpen));
        
        let token2 = reader.next_token().unwrap().unwrap();
        assert!(matches!(token2.token_type, TokenType::BraceClose));
    }
}
