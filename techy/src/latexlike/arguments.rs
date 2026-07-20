//! [`argument_specs`]: the argument-code factory (Phase 7.7; ParserLibraryParity.md
//! N8) — pylatexenc's xparse-like argument shorthands (`LatexStandardArgumentParser`'s
//! codes) resolved **eagerly** into configured core [`ArgumentParser`]s. A plain
//! constructor function, not a parser type: parser choice depends only on the code,
//! never on parse-time facts, and a malformed code is embedder input — an
//! [`Err`](ArgumentCodeError), not a panic and not a diagnostic.
//!
//! The code strings are worth accepting verbatim: pylatexenc's default spec database
//! (a later phase's porting target) is written in them, as are FLM's feature
//! definitions.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    GroupArgumentParser, MarkerArgumentParser, OptionalGroupArgumentParser,
    VerbatimArgumentParser,
};
use crate::spec::{ArgumentParser, ArgumentSpec};
use crate::token::GroupRule;

use super::{GroupType, Latexlike};

/// A malformed argument-code string ([`argument_specs`]): embedder input, reported
/// eagerly at spec-construction time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgumentCodeError {
    /// The character at `offset` begins no known argument code.
    UnknownCode {
        /// Byte offset into the code string.
        offset: usize,
        /// The offending character.
        code: char,
    },
    /// The code at `offset` requires delimiter/marker characters that are missing —
    /// the string ended, or whitespace stood where a parameter character must be
    /// (whitespace separates codes; it cannot be a parameter).
    TruncatedCode {
        /// Byte offset of the code character itself.
        offset: usize,
        /// The code character whose parameters are missing.
        code: char,
    },
}

impl fmt::Display for ArgumentCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgumentCodeError::UnknownCode { offset, code } => {
                write!(f, "unknown argument code ‘{code}’ at offset {offset}")
            }
            ArgumentCodeError::TruncatedCode { offset, code } => write!(
                f,
                "argument code ‘{code}’ at offset {offset} is missing its parameter \
                 character(s)"
            ),
        }
    }
}

impl core::error::Error for ArgumentCodeError {}

/// Build the argument structure described by an xparse-like code string — one
/// [`ArgumentSpec`] per code, in order (`"o{"` = an optional `[…]` then a mandatory
/// argument), ready for [`MacroSpec::new`](super::MacroSpec::new) and friends. Codes
/// may be separated by whitespace; parameters (the `t`/`r`/`d`/`v` characters) must
/// follow their code immediately.
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
///
/// **The `v` disambiguation rule:** `v` immediately followed by a non-whitespace
/// character reads that and the next character as its prescribed delimiters (`"v||"`);
/// a bare auto-delimiter `v` must therefore stand last or be separated from the next
/// code by whitespace (`"v {"` — whereas `"v{"` is a truncated `v{…?`). Codes deferred
/// beyond Phase 7.7: `e{…}` (embellishments) and `AnyDelimited`.
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
///     Arc::new(MacroSpec::new(argument_specs("o{").unwrap())),
/// );
/// let language = Language::<Latexlike>::default()
///     .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(package)))
///     .unwrap();
///
/// let result = language.parse(r"\includegraphics[width=5cm]{fig.png}").unwrap();
/// let node = result.tree.root().child(0).unwrap();
/// assert!(node.arguments().unwrap().get(0).unwrap().is_provided());
/// assert_eq!(
///     node.argument_content_nodes(1).unwrap().next().unwrap().chars(),
///     Some("fig.png"),
/// );
/// ```
pub fn argument_specs(
    codes: &str,
) -> Result<Vec<Arc<ArgumentSpec<Latexlike>>>, ArgumentCodeError> {
    let mut specs = Vec::new();
    let mut chars = codes.char_indices().peekable();
    while let Some((offset, code)) = chars.next() {
        if code.is_whitespace() {
            continue;
        }
        let parameter = |chars: &mut core::iter::Peekable<core::str::CharIndices>| {
            match chars.next() {
                Some((_, c)) if !c.is_whitespace() => Ok(c),
                _ => Err(ArgumentCodeError::TruncatedCode { offset, code }),
            }
        };
        let parser: Arc<dyn ArgumentParser<Latexlike>> = match code {
            'm' | '{' => Arc::new(GroupArgumentParser::new(GroupType::Content)),
            'o' | '[' => Arc::new(optional_group_parser('[', ']')),
            's' | '*' => Arc::new(MarkerArgumentParser::new("*")),
            't' => Arc::new(MarkerArgumentParser::new(String::from(parameter(&mut chars)?))),
            'r' => {
                let open = parameter(&mut chars)?;
                let close = parameter(&mut chars)?;
                Arc::new(GroupArgumentParser::with_rule(minted_rule(open, close)))
            }
            'd' => {
                let open = parameter(&mut chars)?;
                let close = parameter(&mut chars)?;
                Arc::new(optional_group_parser(open, close))
            }
            'v' => match chars.peek() {
                Some((_, c)) if !c.is_whitespace() => {
                    let open = parameter(&mut chars)?;
                    let close = parameter(&mut chars)?;
                    Arc::new(
                        VerbatimArgumentParser::new(GroupType::Verbatim)
                            .with_delimiters(open, close),
                    )
                }
                _ => Arc::new(VerbatimArgumentParser::new(GroupType::Verbatim)),
            },
            _ => return Err(ArgumentCodeError::UnknownCode { offset, code }),
        };
        specs.push(Arc::new(ArgumentSpec::new(parser)));
    }
    Ok(specs)
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
    fn empty_and_whitespace_only_code_strings_declare_no_arguments() {
        assert!(argument_specs("").unwrap().is_empty());
        assert!(argument_specs("  \t ").unwrap().is_empty());
    }

    #[test]
    fn the_codes_resolve_to_their_parsers() {
        let specs = argument_specs("mo s t! r() d<> v").unwrap();
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
        let letters = argument_specs("mos").unwrap();
        let aliases = argument_specs("{[*").unwrap();
        for (letter, alias) in letters.iter().zip(&aliases) {
            assert_eq!(parser_debug(letter), parser_debug(alias));
        }
    }

    #[test]
    fn v_takes_delimiters_exactly_when_followed_directly() {
        let auto = argument_specs("v").unwrap();
        assert!(parser_debug(&auto[0]).contains("delimiters: None"));

        let fixed = argument_specs("v||").unwrap();
        assert!(parser_debug(&fixed[0]).contains("delimiters: Some(('|', '|'))"));

        // Whitespace separates: a bare `v` before another code.
        let separated = argument_specs("v {").unwrap();
        assert_eq!(separated.len(), 2);
        assert!(parser_debug(&separated[0]).contains("delimiters: None"));
        assert!(parser_debug(&separated[1]).contains("GroupArgumentParser"));

        // Directly followed means the delimiters must both be there.
        assert_eq!(
            argument_specs("v{").unwrap_err(),
            ArgumentCodeError::TruncatedCode { offset: 0, code: 'v' }
        );
    }

    #[test]
    fn malformed_codes_report_offset_and_code() {
        assert_eq!(
            argument_specs("m x").unwrap_err(),
            ArgumentCodeError::UnknownCode { offset: 2, code: 'x' }
        );
        assert_eq!(
            argument_specs("t").unwrap_err(),
            ArgumentCodeError::TruncatedCode { offset: 0, code: 't' }
        );
        // Whitespace cannot be a parameter character.
        assert_eq!(
            argument_specs("t !").unwrap_err(),
            ArgumentCodeError::TruncatedCode { offset: 0, code: 't' }
        );
        assert_eq!(
            argument_specs("or(").unwrap_err(),
            ArgumentCodeError::TruncatedCode { offset: 1, code: 'r' }
        );
        assert_eq!(
            argument_specs("x").unwrap_err().to_string(),
            "unknown argument code ‘x’ at offset 0"
        );
        assert_eq!(
            argument_specs("d<").unwrap_err().to_string(),
            "argument code ‘d’ at offset 0 is missing its parameter character(s)"
        );
    }

    // --- end-to-end through the preset (the stdarg port slice) ---------------------------

    /// A language defining `\m` with the given argument codes.
    fn language(recovery: Recovery, codes: &str) -> Language<Latexlike> {
        let mut package = Package::new("factory-tests");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(argument_specs(codes).unwrap())),
        );
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
            .map(|child| child.chars().unwrap_or("<non-chars>").to_string())
            .collect()
    }

    #[test]
    fn m_code_parses_a_brace_group_with_the_expression_fallback() {
        // pylatexenc test_arg_m_0: content = the group's children, spans exact.
        let result = parse_ok("m", r"\m{mandatory argument} (more stuff)");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..22);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().collect();
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
        let region: Vec<_> = m.argument_nodes(0).unwrap().collect();
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
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().collect();
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
