//! Token reader implementation for string sources.

use super::{Token, TokenReader, TokenType};
use crate::error::{ParseError, Result};
use crate::source::Span;

/// A token reader that reads from a string.
pub struct StringTokenReader {
    source: String,
    position: usize,
    peeked: Option<Option<Token>>,
}

impl StringTokenReader {
    /// Create a new token reader for the given source.
    pub fn new(source: String) -> Self {
        Self {
            source,
            position: 0,
            peeked: None,
        }
    }

    /// Read the next token from the source.
    fn read_token(&mut self) -> Result<Option<Token>> {
        // Skip whitespace and track it
        let pre_space = self.consume_whitespace();
        
        if self.position >= self.source.len() {
            return Ok(None);
        }

        let start = self.position;
        let ch = self.current_char()?;

        match ch {
            '\\' => self.read_escape_sequence(start, pre_space),
            '{' => {
                self.position += 1;
                Ok(Some(Token::new(
                    TokenType::BraceOpen,
                    Span::new(start, self.position),
                    pre_space,
                )))
            }
            '}' => {
                self.position += 1;
                Ok(Some(Token::new(
                    TokenType::BraceClose,
                    Span::new(start, self.position),
                    pre_space,
                )))
            }
            '[' => {
                self.position += 1;
                Ok(Some(Token::new(
                    TokenType::BracketOpen,
                    Span::new(start, self.position),
                    pre_space,
                )))
            }
            ']' => {
                self.position += 1;
                Ok(Some(Token::new(
                    TokenType::BracketClose,
                    Span::new(start, self.position),
                    pre_space,
                )))
            }
            '$' => self.read_math_delimiter(start, pre_space),
            '%' => self.read_comment(start, pre_space),
            '&' | '~' | '#' => {
                // Special characters
                self.position += 1;
                Ok(Some(Token::new(
                    TokenType::Specials(ch.to_string()),
                    Span::new(start, self.position),
                    pre_space,
                )))
            }
            _ => self.read_chars(start, pre_space),
        }
    }

    /// Consume and return whitespace.
    fn consume_whitespace(&mut self) -> String {
        let start = self.position;
        while self.position < self.source.len() {
            match self.source.chars().nth(self.position) {
                Some(ch) if ch.is_whitespace() => self.position += 1,
                _ => break,
            }
        }
        self.source[start..self.position].to_string()
    }

    /// Get current character.
    fn current_char(&self) -> Result<char> {
        self.source
            .chars()
            .nth(self.position)
            .ok_or_else(|| ParseError::UnexpectedEndOfInput(self.position))
    }

    /// Read an escape sequence (backslash followed by command).
    fn read_escape_sequence(&mut self, start: usize, pre_space: String) -> Result<Option<Token>> {
        self.position += 1; // Skip backslash

        if self.position >= self.source.len() {
            return Err(ParseError::UnexpectedEndOfInput(self.position));
        }

        let ch = self.current_char()?;

        // Check for special escape sequences
        if ch == '(' || ch == ')' || ch == '[' || ch == ']' {
            self.position += 1;
            let token_type = match ch {
                '(' => TokenType::MathModeInline,
                ')' => TokenType::MathModeInline,
                '[' => TokenType::MathModeDisplay,
                ']' => TokenType::MathModeDisplay,
                _ => unreachable!(),
            };
            return Ok(Some(Token::new(
                token_type,
                Span::new(start, self.position),
                pre_space,
            )));
        }

        // Read command name (alphanumeric characters)
        let cmd_start = self.position;
        if ch.is_alphabetic() {
            while self.position < self.source.len() {
                match self.source.chars().nth(self.position) {
                    Some(c) if c.is_alphabetic() => self.position += 1,
                    _ => break,
                }
            }
        } else {
            // Single non-alphabetic character after backslash
            self.position += 1;
        }

        let cmd_name = self.source[cmd_start..self.position].to_string();

        // Check for \begin{...} and \end{...}
        if cmd_name == "begin" || cmd_name == "end" {
            return self.read_environment_delimiter(cmd_name, start, pre_space);
        }

        Ok(Some(Token::new(
            TokenType::Macro(cmd_name),
            Span::new(start, self.position),
            pre_space,
        )))
    }

    /// Read \begin{name} or \end{name}.
    fn read_environment_delimiter(
        &mut self,
        cmd: String,
        start: usize,
        pre_space: String,
    ) -> Result<Option<Token>> {
        // Skip whitespace
        while self.position < self.source.len() {
            match self.source.chars().nth(self.position) {
                Some(ch) if ch.is_whitespace() => self.position += 1,
                _ => break,
            }
        }

        // Expect opening brace
        if self.position >= self.source.len() || self.current_char()? != '{' {
            // Treat as regular macro if no brace follows
            return Ok(Some(Token::new(
                TokenType::Macro(cmd),
                Span::new(start, self.position),
                pre_space,
            )));
        }

        self.position += 1; // Skip {

        // Read environment name
        let env_start = self.position;
        while self.position < self.source.len() {
            match self.source.chars().nth(self.position) {
                Some('}') => break,
                Some(_) => self.position += 1,
                None => return Err(ParseError::UnexpectedEndOfInput(self.position)),
            }
        }

        let env_name = self.source[env_start..self.position].to_string();
        
        if self.position >= self.source.len() {
            return Err(ParseError::UnexpectedEndOfInput(self.position));
        }

        self.position += 1; // Skip }

        let token_type = if cmd == "begin" {
            TokenType::BeginEnvironment(env_name)
        } else {
            TokenType::EndEnvironment(env_name)
        };

        Ok(Some(Token::new(
            token_type,
            Span::new(start, self.position),
            pre_space,
        )))
    }

    /// Read math mode delimiter ($).
    fn read_math_delimiter(&mut self, start: usize, pre_space: String) -> Result<Option<Token>> {
        self.position += 1;

        // Check for double $$
        if self.position < self.source.len() && self.current_char()? == '$' {
            self.position += 1;
            Ok(Some(Token::new(
                TokenType::MathModeDisplay,
                Span::new(start, self.position),
                pre_space,
            )))
        } else {
            Ok(Some(Token::new(
                TokenType::MathModeInline,
                Span::new(start, self.position),
                pre_space,
            )))
        }
    }

    /// Read a comment (% to end of line).
    fn read_comment(&mut self, start: usize, pre_space: String) -> Result<Option<Token>> {
        self.position += 1; // Skip %

        let comment_start = self.position;
        while self.position < self.source.len() {
            match self.source.chars().nth(self.position) {
                Some('\n') => break,
                Some(_) => self.position += 1,
                None => break,
            }
        }

        let comment = self.source[comment_start..self.position].to_string();

        // Consume the newline
        if self.position < self.source.len() {
            self.position += 1;
        }

        Ok(Some(Token::new(
            TokenType::Comment(comment),
            Span::new(start, self.position),
            pre_space,
        )))
    }

    /// Read regular characters (not special).
    fn read_chars(&mut self, start: usize, pre_space: String) -> Result<Option<Token>> {
        let char_start = self.position;
        
        while self.position < self.source.len() {
            match self.source.chars().nth(self.position) {
                Some(ch) if is_special_char(ch) => break,
                Some(ch) if ch.is_whitespace() => break,
                Some(_) => self.position += 1,
                None => break,
            }
        }

        let chars = self.source[char_start..self.position].to_string();

        Ok(Some(Token::new(
            TokenType::Char(chars),
            Span::new(start, self.position),
            pre_space,
        )))
    }
}

impl TokenReader for StringTokenReader {
    fn peek_token(&mut self) -> Result<Option<Token>> {
        if self.peeked.is_none() {
            self.peeked = Some(self.read_token()?);
        }
        Ok(self.peeked.clone().flatten())
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        if let Some(peeked) = self.peeked.take() {
            Ok(peeked)
        } else {
            self.read_token()
        }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.source.len()
    }
}

/// Check if a character is special in LaTeX.
fn is_special_char(ch: char) -> bool {
    matches!(ch, '\\' | '{' | '}' | '[' | ']' | '$' | '%' | '&' | '~' | '#')
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
