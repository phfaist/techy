//! Tokenization state configuration for LaTeX-like markup languages.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TokenCachedPrefixType {
    GroupOpen,
    GroupClose,
    Special,
}

/// Helper function to read a prefix of allowed characters from a string.
#[inline]
fn read_prefix_allowed_chars<'a, 'b>(allowed: &'a str, s: &'b str) -> (&'b str, usize) {
    let match_len = s
        .char_indices()
        .find(|(_, c)| !allowed.contains(*c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let prefix = &s[..match_len]; // Zero-copy slice into original string
    (prefix, match_len)
}

/// Configuration for tokenization behavior.
///
/// Controls which language features are enabled and how tokens are recognized.
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
            // Standard LaTeX whitespace: space, tab, newline
            whitespace_chars: " \t\n".to_string(),

            // Groups enabled with standard braces
            enable_groups: true,
            group_delimiters: vec![("{".to_string(), "}".to_string())],

            // Macros enabled with backslash and standard alphabetic chars
            enable_macros: true,
            macro_escape_char: '\\',
            macro_alpha_chars:
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),

            // Environments enabled (requires macros)
            enable_environments: true,

            // Specials enabled with standard LaTeX special chars
            enable_specials: true,
            specials_strings: vec![],

            // Comments enabled with %
            enable_comments: true,
            comment_chars: "%".to_string(),

            // Forbidden characters - use Unicode escape sequences
            // \r (carriage return), \x0B (vertical tab), \x08 (backspace)
            forbidden_characters: "\r\x0B\x08".to_string(),
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

    pub(crate) fn read_macro_alpha_chars_prefix<'a>(&self, s: &'a str) -> (&'a str, usize) {
        let allowed = self.macro_alpha_chars();
        read_prefix_allowed_chars(allowed, s)
    }

    pub(crate) fn read_whitespace<'a>(&self, s: &'a str) -> (&'a str, usize) {
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
pub struct TokenizationStateBuilder {
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
