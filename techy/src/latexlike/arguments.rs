//! [`argument_specs`]: the argument-code factory (Phase 7.7; ParserLibraryParity.md
//! N8) — pylatexenc's xparse-like argument shorthands (`LatexStandardArgumentParser`'s
//! codes) resolved **eagerly** into configured core [`ArgumentParser`]s. Plain
//! constructor functions, not a parser type: parser choice depends only on the code,
//! never on parse-time facts, and a malformed code is embedder input — an
//! [`Err`](ArgumentCodeError), not a panic and not a diagnostic.
//!
//! Two entry points, one code grammar: [`argument_specs`] (primary) takes one code
//! string per argument (`["o", "{"]`); [`argument_specs_from_str`] takes the compact
//! whole-spec strings pylatexenc's default spec database (a later phase's porting
//! target) and FLM's feature definitions are written in (`"o{"`), verbatim.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    EmbellishmentsArgumentParser, GroupArgumentParser, MarkerArgumentParser,
    OptionalGroupArgumentParser, VerbatimArgumentParser,
};
use crate::spec::{ArgumentParser, ArgumentSpec};
use crate::token::GroupRule;

use super::{GroupType, Latexlike};

/// A malformed argument code ([`argument_specs`] / [`argument_specs_from_str`]):
/// embedder input, reported eagerly at spec-construction time.
///
/// Errors locate themselves with two coordinates: `index` is the offending element
/// of [`argument_specs`]'s list (`None` when the codes came through
/// [`argument_specs_from_str`]'s single string), and `offset` is a byte offset into
/// that particular string — the element at `index`, or the whole compact string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgumentCodeError {
    /// The character at `offset` begins no known argument code.
    UnknownCode {
        /// The list element holding the code, if the codes came in as a list.
        index: Option<usize>,
        /// Byte offset into the code string.
        offset: usize,
        /// The offending character.
        code: char,
    },
    /// The code at `offset` requires delimiter/marker characters that are missing —
    /// the string ended, or whitespace stood where a parameter character must be
    /// (whitespace separates codes; it cannot be a parameter).
    TruncatedCode {
        /// The list element holding the code, if the codes came in as a list.
        index: Option<usize>,
        /// Byte offset of the code character itself.
        offset: usize,
        /// The code character whose parameters are missing.
        code: char,
    },
    /// A list element continues past its single code ([`argument_specs`] only —
    /// one code per element; in the compact string the next code simply follows).
    TrailingCode {
        /// The list element holding the code.
        index: usize,
        /// Byte offset of the first unexpected character.
        offset: usize,
        /// The first unexpected character.
        trailing: char,
    },
    /// A list element is empty or whitespace-only ([`argument_specs`] only — an
    /// empty *list* declares zero arguments; an empty *element* declares nothing).
    EmptyCode {
        /// The offending list element.
        index: usize,
    },
}

impl fmt::Display for ArgumentCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn at(f: &mut fmt::Formatter<'_>, index: Option<usize>, offset: usize) -> fmt::Result {
            match index {
                Some(index) => write!(f, "at offset {offset} of code string {index}"),
                None => write!(f, "at offset {offset}"),
            }
        }
        match self {
            ArgumentCodeError::UnknownCode { index, offset, code } => {
                write!(f, "unknown argument code ‘{code}’ ")?;
                at(f, *index, *offset)
            }
            ArgumentCodeError::TruncatedCode { index, offset, code } => {
                write!(f, "argument code ‘{code}’ ")?;
                at(f, *index, *offset)?;
                write!(f, " is missing its parameter character(s)")
            }
            ArgumentCodeError::TrailingCode { index, offset, trailing } => write!(
                f,
                "unexpected ‘{trailing}’ at offset {offset} of code string {index} \
                 (one argument code per string)"
            ),
            ArgumentCodeError::EmptyCode { index } => {
                write!(f, "code string {index} is empty (one argument code per string)")
            }
        }
    }
}

impl core::error::Error for ArgumentCodeError {}

/// Build the argument structure described by xparse-like argument codes, one code
/// string per argument — one [`ArgumentSpec`] per element, in order (`["o", "{"]` =
/// an optional `[…]` then a mandatory argument), ready for
/// [`MacroSpec::new`](super::MacroSpec::new) and friends. Each element holds exactly
/// one code together with its parameter characters (the `t`/`r`/`d`/`v` forms:
/// `["t!", "r()", "v||"]`); surrounding whitespace is tolerated, anything more is a
/// loud [`TrailingCode`](ArgumentCodeError::TrailingCode). For the compact whole-spec
/// strings pylatexenc's spec database is written in, use [`argument_specs_from_str`].
///
/// | code | argument |
/// |---|---|
/// | `m` or `{` | mandatory: a `{…}` content group, or the single-expression fallback (`\frac12`) — [`GroupArgumentParser`] |
/// | `o` or `[` | optional `[…]` group (delimiters minted per use; a lone inner `{…}` group protects and unwraps) — [`OptionalGroupArgumentParser`] |
/// | `s` or `*` | optional `*` marker — [`MarkerArgumentParser`] |
/// | `t<c>` | optional single-character marker `<c>` — [`MarkerArgumentParser`] |
/// | `r<c1><c2>` | required group delimited by `<c1>`…`<c2>` (minted per use, no expression fallback) — [`GroupArgumentParser::with_rule`] |
/// | `d<c1><c2>` | optional group delimited by `<c1>`…`<c2>` — [`OptionalGroupArgumentParser`] |
/// | `v` | delimited verbatim, auto-matched delimiter (`\verb`-style) — [`VerbatimArgumentParser`] |
/// | `v<c1><c2>` | delimited verbatim with the prescribed delimiters — [`VerbatimArgumentParser`] |
/// | `e{<chars>}` | embellishments: one marker per character, each followed immediately by an expression, any order, each at most once — [`EmbellishmentsArgumentParser`] |
/// | `AnyDelimited` | mandatory group delimited by any of `{}` `[]` `()` `<>` (minted per use; contents keep only the matched pair) — [`GroupArgumentParser::any_of`] |
/// | `AnyDelimitedOptional` | the optional flavor (lone inner `{…}` protects and unwraps, like `o`) — [`OptionalGroupArgumentParser::any_of`] |
///
/// In list form the two `v` shapes need no disambiguation: `["v"]` is the
/// auto-matched-delimiter form, `["v||"]` the prescribed one (the whitespace rule of
/// the compact grammar lives with [`argument_specs_from_str`]). The word codes
/// `AnyDelimited`/`AnyDelimitedOptional` are **list-form only** — each is a whole
/// element (pylatexenc uses them as whole `arg_spec` strings the same way); in a
/// compact string they would read as an unknown code `A`.
///
/// The argument specs carry no names and no per-argument state deltas — attach those
/// via [`ArgumentSpec`]'s builders where needed (the factory is convenience, never a
/// requirement; any hand-built parser remains first-class).
///
/// ```
/// use std::sync::Arc;
/// use techy::engine::Language;
/// use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
/// use techy::scopes::Package;
/// use techy::state::ParsingStateDelta;
///
/// let mut package = Package::new("mydefs");
/// package.insert(
///     CallableType::Macro,
///     "includegraphics",
///     Arc::new(MacroSpec::new(argument_specs(["o", "{"]).unwrap())),
/// );
/// let language = Language::<Latexlike>::default()
///     .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(package)))
///     .unwrap();
///
/// let result = language.parse(r"\includegraphics[width=5cm]{fig.png}").unwrap();
/// let node = result.tree.root().child(0).unwrap();
/// assert!(node.arguments().unwrap().get(0).unwrap().is_provided());
/// assert_eq!(
///     node.argument_content_nodes(1).unwrap().iter().next().unwrap().chars(),
///     Some("fig.png"),
/// );
/// ```
pub fn argument_specs<I>(codes: I) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut specs = Vec::new();
    for (index, code) in codes.into_iter().enumerate() {
        // The word codes claim whole (trimmed) list elements before the
        // character-code scan sees them.
        if let Some(spec) = scan_word_code(code.as_ref().trim()) {
            specs.push(spec);
            continue;
        }
        let mut chars = code.as_ref().char_indices().peekable();
        let spec = scan_code(&mut chars, Some(index))?
            .ok_or(ArgumentCodeError::EmptyCode { index })?;
        if let Some((offset, trailing)) = chars.find(|(_, c)| !c.is_whitespace()) {
            return Err(ArgumentCodeError::TrailingCode { index, offset, trailing });
        }
        specs.push(spec);
    }
    Ok(specs)
}

/// Resolve a whole-element word code (`AnyDelimited` / `AnyDelimitedOptional`) —
/// list-form only (see [`argument_specs`]).
fn scan_word_code(code: &str) -> Option<Arc<ArgumentSpec<Latexlike>>> {
    let parser: Arc<dyn ArgumentParser<Latexlike>> = match code {
        "AnyDelimited" => Arc::new(GroupArgumentParser::any_of(any_delimited_rules())),
        "AnyDelimitedOptional" => Arc::new(
            OptionalGroupArgumentParser::any_of(any_delimited_rules())
                .with_unwrap_lone_group(GroupType::Content),
        ),
        _ => return None,
    };
    Some(Arc::new(ArgumentSpec::new(parser)))
}

/// The default delimiter alternatives of the `AnyDelimited` codes (pylatexenc's
/// list): `{}`, `[]`, `()`, `<>`, minted as content-class rules per use.
fn any_delimited_rules() -> Vec<Arc<GroupRule<Latexlike>>> {
    [('{', '}'), ('[', ']'), ('(', ')'), ('<', '>')]
        .into_iter()
        .map(|(open, close)| minted_rule(open, close))
        .collect()
}

/// [`argument_specs`] from the compact form: all codes concatenated in one string
/// (`"o{"`, `"mo s t! r() d<> v"`) — pylatexenc's default spec database (a later
/// phase's porting target) and FLM's feature definitions are written in these
/// strings, worth accepting verbatim. Codes may be separated by whitespace;
/// parameters (the `t`/`r`/`d`/`v` characters) must follow their code immediately.
///
/// **The `v` disambiguation rule:** `v` immediately followed by a non-whitespace
/// character reads that and the next character as its prescribed delimiters (`"v||"`);
/// a bare auto-delimiter `v` must therefore stand last or be separated from the next
/// code by whitespace (`"v {"` — whereas `"v{"` is a truncated `v{…?`).
pub fn argument_specs_from_str(
    codes: &str,
) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
    let mut specs = Vec::new();
    let mut chars = codes.char_indices().peekable();
    while let Some(spec) = scan_code(&mut chars, None)? {
        specs.push(spec);
    }
    Ok(specs)
}

/// Scan one code (with its parameter characters) off `chars`, skipping leading
/// whitespace; `Ok(None)` when the string is exhausted first. `index` is the list
/// coordinate threaded into errors (`None` in the compact-string form).
fn scan_code(
    chars: &mut core::iter::Peekable<core::str::CharIndices<'_>>,
    index: Option<usize>,
) -> Result<Option<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
    let (offset, code) = loop {
        match chars.next() {
            Some((_, c)) if c.is_whitespace() => continue,
            Some(pair) => break pair,
            None => return Ok(None),
        }
    };
    let parameter = |chars: &mut core::iter::Peekable<core::str::CharIndices>| {
        match chars.next() {
            Some((_, c)) if !c.is_whitespace() => Ok(c),
            _ => Err(ArgumentCodeError::TruncatedCode { index, offset, code }),
        }
    };
    let parser: Arc<dyn ArgumentParser<Latexlike>> = match code {
        'm' | '{' => Arc::new(GroupArgumentParser::new(GroupType::Content)),
        'o' | '[' => Arc::new(optional_group_parser('[', ']')),
        's' | '*' => Arc::new(MarkerArgumentParser::new("*")),
        't' => Arc::new(MarkerArgumentParser::new(String::from(parameter(&mut *chars)?))),
        'r' => {
            let open = parameter(&mut *chars)?;
            let close = parameter(&mut *chars)?;
            Arc::new(GroupArgumentParser::with_rule(minted_rule(open, close)))
        }
        'd' => {
            let open = parameter(&mut *chars)?;
            let close = parameter(&mut *chars)?;
            Arc::new(optional_group_parser(open, close))
        }
        'e' => {
            // `e{<chars>}`: the braces must follow immediately; one marker per
            // character between them, at least one, no whitespace (whitespace
            // separates codes; it cannot be a parameter).
            if parameter(&mut *chars)? != '{' {
                return Err(ArgumentCodeError::TruncatedCode { index, offset, code });
            }
            let mut markers: Vec<String> = Vec::new();
            loop {
                match chars.next() {
                    Some((_, '}')) => break,
                    Some((_, c)) if !c.is_whitespace() => markers.push(String::from(c)),
                    _ => {
                        return Err(ArgumentCodeError::TruncatedCode { index, offset, code })
                    }
                }
            }
            if markers.is_empty() {
                return Err(ArgumentCodeError::TruncatedCode { index, offset, code });
            }
            Arc::new(EmbellishmentsArgumentParser::new(markers))
        }
        'v' => match chars.peek() {
            Some((_, c)) if !c.is_whitespace() => {
                let open = parameter(&mut *chars)?;
                let close = parameter(&mut *chars)?;
                Arc::new(
                    VerbatimArgumentParser::new(GroupType::Verbatim)
                        .with_delimiters(open, close),
                )
            }
            _ => Arc::new(VerbatimArgumentParser::new(GroupType::Verbatim)),
        },
        _ => return Err(ArgumentCodeError::UnknownCode { index, offset, code }),
    };
    Ok(Some(Arc::new(ArgumentSpec::new(parser))))
}

/// The minted per-use content-class rule of the `o`/`r`/`d` codes.
fn minted_rule(open: char, close: char) -> Arc<GroupRule<Latexlike>> {
    Arc::new(GroupRule {
        group_type: GroupType::Content,
        open: String::from(open),
        close: String::from(close),
    })
}

/// The optional-group shape shared by `o` and `d`: minted delimiters, with the
/// protective lone `{…}` group unwrapping (the parse-time resolution of pylatexenc's
/// `unwrap_double_group` accessor default).
fn optional_group_parser(open: char, close: char) -> OptionalGroupArgumentParser<Latexlike> {
    OptionalGroupArgumentParser::new(minted_rule(open, close))
        .with_unwrap_lone_group(GroupType::Content)
}

#[cfg(test)]
mod tests {
    use super::super::{CallableType, LatexlikeDriver, MacroSpec};
    use super::*;
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::Package;
    use crate::state::ParsingStateDelta;
    use alloc::format;
    use alloc::string::ToString;

    // --- the code grammar ---------------------------------------------------------------

    /// The parser type each code resolved to, read off the spec's Debug rendering
    /// (each standard parser type names itself there).
    fn parser_debug(spec: &ArgumentSpec<Latexlike>) -> String {
        format!("{:?}", spec.parser)
    }

    #[test]
    fn empty_lists_and_whitespace_only_compact_strings_declare_no_arguments() {
        assert!(argument_specs(Vec::<&str>::new()).unwrap().is_empty());
        assert!(argument_specs_from_str("").unwrap().is_empty());
        assert!(argument_specs_from_str("  \t ").unwrap().is_empty());
    }

    #[test]
    fn the_codes_resolve_to_their_parsers() {
        let specs = argument_specs(["m", "o", "s", "t!", "r()", "d<>", "v"]).unwrap();
        assert_eq!(specs.len(), 7);
        assert!(parser_debug(&specs[0]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[0]).contains("group_type: Content"));
        assert!(parser_debug(&specs[1]).contains("OptionalGroupArgumentParser"));
        assert!(parser_debug(&specs[2]).contains("marker: \"*\""));
        assert!(parser_debug(&specs[3]).contains("marker: \"!\""));
        assert!(parser_debug(&specs[4]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[4]).contains("rule"));
        assert!(parser_debug(&specs[5]).contains("OptionalGroupArgumentParser"));
        assert!(parser_debug(&specs[6]).contains("VerbatimArgumentParser"));
        // None of the factory's specs carry names or state deltas.
        assert!(specs.iter().all(|spec| spec.name.is_none()));
        assert!(specs.iter().all(|spec| spec.parsing_state_delta.is_none()));
    }

    #[test]
    fn the_shorthand_aliases_match_their_letters() {
        let letters = argument_specs(["m", "o", "s"]).unwrap();
        let aliases = argument_specs(["{", "[", "*"]).unwrap();
        for (letter, alias) in letters.iter().zip(&aliases) {
            assert_eq!(parser_debug(letter), parser_debug(alias));
        }
    }

    #[test]
    fn the_compact_string_matches_the_list_form() {
        let compact = argument_specs_from_str("mo s t! r() d<> v").unwrap();
        let listed = argument_specs(["m", "o", "s", "t!", "r()", "d<>", "v"]).unwrap();
        assert_eq!(compact.len(), listed.len());
        for (c, l) in compact.iter().zip(&listed) {
            assert_eq!(parser_debug(c), parser_debug(l));
        }
    }

    #[test]
    fn list_elements_hold_one_code_and_tolerate_surrounding_whitespace() {
        let specs = argument_specs([" m ", "\tt!", "v "]).unwrap();
        assert_eq!(specs.len(), 3);
        assert!(parser_debug(&specs[0]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[1]).contains("marker: \"!\""));
        // Trailing whitespace does not turn `v` into the prescribed-delimiter form.
        assert!(parser_debug(&specs[2]).contains("delimiters: None"));
    }

    #[test]
    fn v_takes_delimiters_exactly_when_followed_directly() {
        let auto = argument_specs_from_str("v").unwrap();
        assert!(parser_debug(&auto[0]).contains("delimiters: None"));

        let fixed = argument_specs_from_str("v||").unwrap();
        assert!(parser_debug(&fixed[0]).contains("delimiters: Some(('|', '|'))"));

        // Whitespace separates: a bare `v` before another code.
        let separated = argument_specs_from_str("v {").unwrap();
        assert_eq!(separated.len(), 2);
        assert!(parser_debug(&separated[0]).contains("delimiters: None"));
        assert!(parser_debug(&separated[1]).contains("GroupArgumentParser"));

        // Directly followed means the delimiters must both be there.
        assert_eq!(
            argument_specs_from_str("v{").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 'v' }
        );

        // In list form there is nothing to disambiguate.
        let auto = argument_specs(["v"]).unwrap();
        assert!(parser_debug(&auto[0]).contains("delimiters: None"));
        let fixed = argument_specs(["v||"]).unwrap();
        assert!(parser_debug(&fixed[0]).contains("delimiters: Some(('|', '|'))"));
    }

    #[test]
    fn malformed_compact_strings_report_offset_and_code() {
        assert_eq!(
            argument_specs_from_str("m x").unwrap_err(),
            ArgumentCodeError::UnknownCode { index: None, offset: 2, code: 'x' }
        );
        assert_eq!(
            argument_specs_from_str("t").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 't' }
        );
        // Whitespace cannot be a parameter character.
        assert_eq!(
            argument_specs_from_str("t !").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 't' }
        );
        assert_eq!(
            argument_specs_from_str("or(").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 1, code: 'r' }
        );
        assert_eq!(
            argument_specs_from_str("x").unwrap_err().to_string(),
            "unknown argument code ‘x’ at offset 0"
        );
        assert_eq!(
            argument_specs_from_str("d<").unwrap_err().to_string(),
            "argument code ‘d’ at offset 0 is missing its parameter character(s)"
        );
    }

    #[test]
    fn malformed_code_lists_report_index_offset_and_code() {
        assert_eq!(
            argument_specs(["m", "x"]).unwrap_err(),
            ArgumentCodeError::UnknownCode { index: Some(1), offset: 0, code: 'x' }
        );
        assert_eq!(
            argument_specs(["o", "r("]).unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: Some(1), offset: 0, code: 'r' }
        );
        // One code per element: a second code is trailing, not concatenated.
        assert_eq!(
            argument_specs(["mo"]).unwrap_err(),
            ArgumentCodeError::TrailingCode { index: 0, offset: 1, trailing: 'o' }
        );
        assert_eq!(
            argument_specs(["m", "o m"]).unwrap_err(),
            ArgumentCodeError::TrailingCode { index: 1, offset: 2, trailing: 'm' }
        );
        // Empty elements are bugs, not zero-argument declarations.
        assert_eq!(
            argument_specs(["m", ""]).unwrap_err(),
            ArgumentCodeError::EmptyCode { index: 1 }
        );
        assert_eq!(
            argument_specs([" \t"]).unwrap_err(),
            ArgumentCodeError::EmptyCode { index: 0 }
        );
        // Display strings name the list element.
        assert_eq!(
            argument_specs(["x"]).unwrap_err().to_string(),
            "unknown argument code ‘x’ at offset 0 of code string 0"
        );
        assert_eq!(
            argument_specs(["r<"]).unwrap_err().to_string(),
            "argument code ‘r’ at offset 0 of code string 0 is missing its parameter \
             character(s)"
        );
        assert_eq!(
            argument_specs(["mo"]).unwrap_err().to_string(),
            "unexpected ‘o’ at offset 1 of code string 0 (one argument code per string)"
        );
        assert_eq!(
            argument_specs([""]).unwrap_err().to_string(),
            "code string 0 is empty (one argument code per string)"
        );
    }

    // --- end-to-end through the preset (the stdarg port slice) ---------------------------

    /// A language defining `\m` with the given argument codes (a compact string, or a
    /// single word code like `AnyDelimited`, which only the list form accepts).
    fn language(recovery: Recovery, codes: &str) -> Language<Latexlike> {
        let specs = argument_specs_from_str(codes)
            .or_else(|_| argument_specs([codes]))
            .unwrap();
        let mut package = Package::new("factory-tests");
        package.insert(CallableType::Macro, "m", Arc::new(MacroSpec::new(specs)));
        Language::new(LatexlikeDriver::new(recovery))
            .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(package)))
            .unwrap()
    }

    fn parse_ok(codes: &str, input: &str) -> ParseResult<Latexlike> {
        let result = language(Recovery::Strict, codes).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn macro_node(result: &ParseResult<Latexlike>) -> NodeRef<'_, Latexlike> {
        let node = result.tree.root().child(0).expect("the macro node");
        assert_eq!(node.macro_name(), Some("m"));
        node
    }

    fn content_chars(node: NodeRef<'_, Latexlike>, i: usize) -> String {
        node.argument_content_nodes(i)
            .expect("provided argument")
            .iter().map(|child| child.chars().unwrap_or("<non-chars>").to_string())
            .collect()
    }

    #[test]
    fn m_code_parses_a_brace_group_with_the_expression_fallback() {
        // pylatexenc test_arg_m_0: content = the group's children, spans exact.
        let result = parse_ok("m", r"\m{mandatory argument} (more stuff)");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..22);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("mandatory argument"));
        assert_eq!(content[0].span().range(), 3..21);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" (more stuff)"));

        // The single-expression fallback.
        let result = parse_ok("mm", r"\m 1 2");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "1");
        assert_eq!(content_chars(m, 1), "2");
    }

    #[test]
    fn m_code_keeps_a_leading_comment_as_region_noise() {
        // pylatexenc test_arg_m_precomment: the comment always stays in the region
        // (techy keeps it as a node; `return_full_node_list` dissolved into regions),
        // and the content designation excludes it.
        let result = parse_ok("m", "\\m %comment here\n{mandatory argument}");
        let m = macro_node(&result);
        let region: Vec<_> = m.argument_nodes(0).unwrap().iter().collect();
        assert_eq!(region.len(), 2);
        assert_eq!(region[0].comment(), Some("comment here"));
        assert_eq!(content_chars(m, 0), "mandatory argument");
    }

    #[test]
    fn o_code_parses_an_optional_bracket_group() {
        let result = parse_ok("om", r"\m[opt]{x}");
        let m = macro_node(&result);
        assert!(m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(content_chars(m, 0), "opt");

        // Absent: silent, nothing consumed — `[` only ever opens right there.
        let result = parse_ok("om", r"\m{x} [not an option]");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(content_chars(m, 1), "x");
    }

    #[test]
    fn o_code_unwraps_a_lone_protective_brace_group() {
        // `[{…}]` protecting a literal `]`: the content designation resolves to the
        // inner group's children (pylatexenc's unwrap_double_group, at parse time).
        let result = parse_ok("o", r"\m[{a]b}]");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "a]b");
    }

    #[test]
    fn s_and_t_codes_parse_optional_markers() {
        // pylatexenc test_arg_star_0/_1 (the marker's pre-space becomes noise).
        let result = parse_ok("sm", r"\m*{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "*");

        let result = parse_ok("sm", r"\m {x}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        let result = parse_ok("t!m", r"\m!{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "!");
    }

    #[test]
    fn r_code_parses_a_required_delimited_group() {
        let result = parse_ok("r()", r"\m(a,b) x");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..7);
        assert_eq!(content_chars(m, 0), "a,b");
        let group = m.child(0).unwrap();
        assert_eq!(group.group_delimiters(), Some(("(", ")")));
        assert_eq!(group.group_type(), Some(GroupType::Content));

        // Nested pairs balance; braces protect the closer.
        let result = parse_ok("r()", r"\m(a(b)c)");
        assert_eq!(content_chars(macro_node(&result), 0), "a<non-chars>c");
        let result = parse_ok("r()", r"\m(a{b)c}d)");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content[1].group_delimiters(), Some(("{", "}")));
    }

    #[test]
    fn r_code_missing_is_diagnosed_with_no_expression_fallback() {
        let err = language(Recovery::Strict, "r()").parse(r"\m x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        let result = language(Recovery::Tolerant, "r()").parse(r"\m x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        // `x` was not swallowed as an expression: it stays sibling content (the
        // blank is the trigger's own syntactic post-space).
        assert_eq!(m.post_space(), Some(" "));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("x"));
    }

    #[test]
    fn d_code_parses_an_optional_delimited_group() {
        let result = parse_ok("d<>m", r"\m<opt>{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "opt");

        let result = parse_ok("d<>m", r"\m{x}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
    }

    #[test]
    fn e_code_and_word_codes_resolve_to_their_parsers() {
        let specs = argument_specs(["e{^_}", "AnyDelimited", " AnyDelimitedOptional "]).unwrap();
        assert!(parser_debug(&specs[0]).contains("EmbellishmentsArgumentParser"));
        assert!(parser_debug(&specs[0]).contains("markers: [\"^\", \"_\"]"));
        assert!(parser_debug(&specs[1]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[1]).contains("rules"));
        assert!(parser_debug(&specs[2]).contains("OptionalGroupArgumentParser"));

        // `e{…}` also reads in the compact form; the word codes are list-form only.
        let compact = argument_specs_from_str("me{^_}o").unwrap();
        assert_eq!(compact.len(), 3);
        assert!(parser_debug(&compact[1]).contains("EmbellishmentsArgumentParser"));
        assert_eq!(
            argument_specs_from_str("AnyDelimited").unwrap_err(),
            ArgumentCodeError::UnknownCode { index: None, offset: 0, code: 'A' }
        );

        // Malformed `e` codes: missing braces, empty marker set, whitespace inside,
        // unterminated set.
        for bad in ["e", "ex", "e{}", "e{^", "e{^ _}"] {
            assert_eq!(
                argument_specs([bad]).unwrap_err(),
                ArgumentCodeError::TruncatedCode { index: Some(0), offset: 0, code: 'e' },
                "code {bad:?}"
            );
        }
    }

    #[test]
    fn e_code_parses_embellishments_span_exactly() {
        // pylatexenc test_arg_embelishments_1 (`e{_^`}` on `^{test}_x{more stuff}`,
        // shifted by the `\m` trigger): one wrapper group per matched pair, the
        // marker as its opening delimiter, the following `{more stuff}` untouched.
        let result = parse_ok("e{_^`}", "\\m^{test}_x{more stuff} stuff");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..11);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);

        let sup = content[0];
        assert_eq!(sup.group_delimiters(), Some(("^", "")));
        assert_eq!(sup.group_type(), None); // classless synthesized wrapper
        assert_eq!(sup.span().range(), 2..9);
        let sup_value = sup.child(0).unwrap();
        assert_eq!(sup_value.group_delimiters(), Some(("{", "}")));
        assert_eq!(sup_value.span().range(), 3..9);
        assert_eq!(sup_value.child(0).unwrap().chars(), Some("test"));

        let sub = content[1];
        assert_eq!(sub.group_delimiters(), Some(("_", "")));
        assert_eq!(sub.span().range(), 9..11);
        assert_eq!(sub.child(0).unwrap().chars(), Some("x"));

        // The next group belongs to the enclosing content.
        assert!(result.tree.root().child(1).unwrap().is_group());

        // The extraction helper reads the run by marker.
        let fields = crate::node::extract::split_embellishments(
            m.argument_content_nodes(0).unwrap(),
        )
        .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(
            crate::node::extract::content_as_chars(fields.get("^").unwrap().value_content().unwrap())
                .unwrap(),
            "test"
        );
        assert_eq!(
            crate::node::extract::content_as_chars(fields.get("_").unwrap().value().unwrap())
                .unwrap(),
            "x"
        );
        assert!(fields.get("`").is_none());
    }

    #[test]
    fn e_code_embellishments_match_in_any_order_each_at_most_once() {
        // pylatexenc test_arg_embelishments_2: no marker at all — silently absent.
        let result = parse_ok("e{_^}", r"\m more stuff");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        // Any source order.
        let result = parse_ok("e{^_}", r"\m_{b}^{a}");
        let m = macro_node(&result);
        let fields =
            crate::node::extract::split_embellishments(m.argument_content_nodes(0).unwrap())
                .unwrap();
        let keys: Vec<_> = fields.iter().map(|entry| entry.key().to_string()).collect();
        assert_eq!(keys, ["_", "^"]);

        // Each marker at most once: the second `^{b}` stays enclosing content.
        let result = parse_ok("e{^_}", r"\m^{a}^{b}");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..6);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("^"));
    }

    #[test]
    fn e_code_allows_noise_before_markers_but_not_after() {
        // Noise ahead of each marker: whitespace and comments become region noise.
        let result = parse_ok("e{^_}", "\\m %pre\n^{a} _{b}!");
        let m = macro_node(&result);
        let region: Vec<_> = m.argument_nodes(0).unwrap().iter().collect();
        assert!(region.iter().any(|node| node.comment().is_some()));
        let fields =
            crate::node::extract::split_embellishments(m.argument_content_nodes(0).unwrap())
                .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("!"));

        // Marker + expression are atomic: whitespace after the marker unmatches it
        // whole (silently — `^ {a}` is ordinary enclosing content)…
        let result = parse_ok("e{^_}", r"\m^ {a}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("^ "));
        assert!(result.tree.root().child(2).unwrap().is_group());

        // …and so does a comment after the marker.
        let result = parse_ok("e{^_}", "\\m^%c\n{a}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("^"));

        // A committed pair before the violation stays: `_{b}` parses, the dangling
        // `^` ends the run.
        let result = parse_ok("e{^_}", r"\m_{b}^ {a}");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].group_delimiters(), Some(("_", "")));
    }

    #[test]
    fn any_delimited_code_takes_any_default_pair_and_narrows_the_contents() {
        // pylatexenc test_arg_any_delimited_angleb.
        let result = parse_ok("AnyDelimited", r"\m<delimited>more stuff");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..13);
        let group = m.child(0).unwrap();
        assert_eq!(group.group_delimiters(), Some(("<", ">")));
        assert_eq!(group.group_type(), Some(GroupType::Content));
        assert_eq!(content_chars(m, 0), "delimited");
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("more stuff"));

        // Braces (a default pair and the base rule alike) still match…
        let result = parse_ok("AnyDelimited", r"\m{x}");
        assert_eq!(content_chars(macro_node(&result), 0), "x");

        // pylatexenc test_multidelim_sg: inside `<…>`, braces protect a stray `>`…
        let result = parse_ok("AnyDelimited", r"\m<Hello {there>}>");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0].chars(), Some("Hello "));
        assert_eq!(content[1].group_delimiters(), Some(("{", "}")));
        assert_eq!(content[1].child(0).unwrap().chars(), Some("there>"));

        // …and test_multidelim_sg2: the matched pair nests, the unmatched `]` is a
        // plain character (the contents keep only the encountered pair).
        let result = parse_ok("AnyDelimited", r"\m<Hello <there]>>");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
        assert_eq!(content[1].group_delimiters(), Some(("<", ">")));
        assert_eq!(content[1].child(0).unwrap().chars(), Some("there]"));
    }

    #[test]
    fn any_delimited_mandatory_vs_optional() {
        // pylatexenc test_multidelim_mandatory: no pair opens — diagnosed.
        let err = language(Recovery::Strict, "AnyDelimited").parse(r"\m x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        // pylatexenc test_multidelim_optional: absent is silent.
        let result = parse_ok("AnyDelimitedOptional", r"\m juice");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        // The optional flavor unwraps a lone protective brace group, like `o`.
        let result = parse_ok("AnyDelimitedOptional", r"\m[{a]b}]");
        assert_eq!(content_chars(macro_node(&result), 0), "a]b");
    }

    #[test]
    fn v_codes_parse_delimited_verbatim() {
        // Auto-matched delimiter: raw content, comment and escape chars included.
        let result = parse_ok("v", r"\m|a%\x{|z");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), r"a%\x{");
        let group = m.child(0).unwrap();
        assert_eq!(group.group_type(), Some(GroupType::Verbatim));
        assert_eq!(group.group_delimiters(), Some(("|", "|")));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("z"));

        // Prescribed delimiters.
        let result = parse_ok("v+-", r"\m+ab-");
        assert_eq!(content_chars(macro_node(&result), 0), "ab");
    }
}
