//! Representative absent-feature test languages, through the public API only.
//!
//! The lang-features plan calls for four representative languages. The
//! all-features-present language is the existing suite (every test language uses
//! `TrivialLang`'s `AllLangFeatures`, and the latexlike preset pins it); this file
//! adds the other three, each implementing [`Lang`] by hand with a partial or empty
//! feature declaration:
//!
//! - [`support::PlainCharsLang`] — `NoLangFeatures`: every feature absent. Its seed
//!   rules are `TokenRules::empty()` — with per-feature storage gating, that is the
//!   only writable value: carrying rules data for an absent feature is a
//!   compile-time error, not a runtime no-op. The parses pin the behavior record:
//!   every construct reads as plain character content.
//! - [`support::GroupsOnlyLang`] — groups present, the seven other features absent:
//!   the seed populates exactly the groups block (the rest spreads from
//!   `TokenRules::empty()`), braces parse as `Group` nodes, and whitespace
//!   characters are ordinary content tokens (never folded into a token's
//!   pre-space).
//! - [`support::CommandsWithoutScopesLang`] — commands and whitespace present,
//!   everything else absent (the lattice ruling: callables do **not** imply
//!   scopes). The seed populates exactly those two blocks. Commands resolve through
//!   a fixed table on the driver — no scope stack. A state change carrying data for
//!   an absent feature is not representable at all: the override fields and the
//!   delta's scope-op list are zero-sized stores for absent features, and the
//!   scope-op builders and the verbatim recipe carry feature bounds — writing such
//!   a delta is a compile-time error, not a runtime report (the positive halves of
//!   those compile facts are pinned in `feature_composition`). The scope stack
//!   itself is stored the same way: with the feature absent it is permanently
//!   empty, and applying a scope op to it directly reports `ScopesAbsent`.
//!
//! Composition note for the commands language: whitespace is present because command
//! tokenization consumes post-space through `skip_whitespace`; groups stay absent
//! because zero-argument callables need no group machinery (the argument parsers
//! that mint temporary group rules are exactly the ones that do — they carry a
//! `LangHasGroups` bound).
//!
//! [`Lang`]: techy::core::Lang

mod support {
    use std::sync::Arc;

    use techy::core::constructs::FromInvocation;
    use techy::core::node::{validate_tree, NodeKind, NodeRef, StagedChildren};
    use techy::core::specs::{
        CallableSpec, CommandResolution, ResolvedCallable, ScopeStack, StdCallableSpec,
    };
    use techy::core::{
        CommandResolver, CommandRule, CommandRules, FeatureAbsent, FeaturePresent,
        GroupRule, GroupRules, Lang, LangFeatures, Language, NoLangFeatures, ParseResult,
        ParsingState, SpecialsMatch, StateData, StdParseDriver, Token, TokenKind,
        TokenResult, TokenRules, TriggerChars, WhitespaceRules,
    };
    use techy::error::Recovery;
    use techy::source::SourceSpan;

    /// The callable type of the fixed-table commands (an arbitrary closed-vocabulary
    /// value; `u32` is the test default).
    pub const CT_COMMAND: u32 = 1;

    // Seed note: each language's `initial_state_data` writes plain literals for its
    // *present* feature blocks and spreads the rest from `TokenRules::empty()`. With
    // per-feature storage gating, that is the only shape that compiles — an absent
    // feature's field is the zero-sized store, so a rules literal for it is a type
    // error (the fully-populated seeds these languages used before the gating are
    // unwritable by design).

    /// The shared specials scan of the test languages that declare specials absent
    /// while implementing the hooks anyway: `~` would trigger a zero-argument
    /// callable — if the specials feature existed.
    fn scan_tilde<'s, L: Lang<CallableTypeId = u32>>(
        content: &'s str,
        pos: usize,
    ) -> Option<SpecialsMatch<'s, L>> {
        content[pos..].starts_with('~').then(|| SpecialsMatch {
            end: pos + 1,
            callable_type: CT_COMMAND,
            name: &content[pos..pos + 1],
            spec: Arc::new(StdCallableSpec::default()),
        })
    }

    // --- PlainCharsLang: NoLangFeatures ---------------------------------------------

    /// Every feature absent (`NoLangFeatures`). The seed rules are
    /// `TokenRules::empty()` — the only writable value, since every field is the
    /// zero-sized store — yet the specials hooks are implemented: they may have no
    /// effect.
    #[derive(Debug, Clone, Copy)]
    pub struct PlainCharsLang;

    impl Lang for PlainCharsLang {
        type Features = NoLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: TokenRules::empty(),
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            Ok(scan_tilde(content, pos))
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Option<String>>,
            _state: &Arc<ParsingState<Self>>,
            _children: StagedChildren<'_, Self>,
        ) {
        }
    }

    // --- GroupsOnlyLang: groups present, everything else absent ----------------------

    /// The groups-only feature declaration: `Groups` present, the seven other
    /// members absent.
    pub struct GroupsOnlyLangFeatures;

    impl LangFeatures for GroupsOnlyLangFeatures {
        type Whitespace = FeatureAbsent;
        type Paragraphs = FeatureAbsent;
        type Groups = FeaturePresent;
        type Commands = FeatureAbsent;
        type Comments = FeatureAbsent;
        type Specials = FeatureAbsent;
        type ForbiddenChars = FeatureAbsent;
        type Scopes = FeatureAbsent;
    }

    /// Group delimiters are the language's only feature; only the groups block can
    /// carry data (every other rules field is the zero-sized store).
    #[derive(Debug, Clone, Copy)]
    pub struct GroupsOnlyLang;

    impl Lang for GroupsOnlyLang {
        type Features = GroupsOnlyLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: TokenRules {
                    groups: GroupRules {
                        enabled: true,
                        rules: vec![Arc::new(GroupRule {
                            group_type: 0,
                            open: "{".into(),
                            close: "}".into(),
                        })],
                        temporary: vec![Arc::new(GroupRule {
                            group_type: 0,
                            open: "[".into(),
                            close: "]".into(),
                        })],
                        expecting_close: None,
                    },
                    ..TokenRules::empty()
                },
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }

        fn scan_specials<'s>(
            _state: &ParsingState<Self>,
            content: &'s str,
            pos: usize,
        ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
            Ok(scan_tilde(content, pos))
        }

        fn specials_trigger_chars(_data: &StateData<Self>) -> TriggerChars {
            TriggerChars::Only("~".into())
        }

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Option<String>>,
            _state: &Arc<ParsingState<Self>>,
            _children: StagedChildren<'_, Self>,
        ) {
        }
    }

    // --- CommandsWithoutScopesLang: commands + whitespace present --------------------

    /// The callables-without-scopes declaration: `Commands` and `Whitespace` present
    /// (command tokenization consumes post-space through whitespace skipping),
    /// everything else absent — `Scopes` above all: callables do not imply scopes.
    pub struct CommandsWithoutScopesLangFeatures;

    impl LangFeatures for CommandsWithoutScopesLangFeatures {
        type Whitespace = FeaturePresent;
        type Paragraphs = FeatureAbsent;
        type Groups = FeatureAbsent;
        type Commands = FeaturePresent;
        type Comments = FeatureAbsent;
        type Specials = FeatureAbsent;
        type ForbiddenChars = FeatureAbsent;
        type Scopes = FeatureAbsent;
    }

    /// Commands resolve through [`FixedTableResolver`] — a fixed command table on the
    /// driver, no scope stack (the motivating case for the scopes-independent
    /// commands feature: a fixed command set with no `\newcommand`).
    #[derive(Debug, Clone, Copy)]
    pub struct CommandsWithoutScopesLang;

    impl Lang for CommandsWithoutScopesLang {
        type Features = CommandsWithoutScopesLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver<FixedTableResolver>;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: TokenRules {
                    whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
                    commands: CommandRules {
                        enabled: true,
                        rules: vec![Arc::new(CommandRule {
                            escape_char: '\\',
                            name_chars:
                                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
                                    .into(),
                        })],
                    },
                    ..TokenRules::empty()
                },
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }

        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Option<String>>,
            _state: &Arc<ParsingState<Self>>,
            _children: StagedChildren<'_, Self>,
        ) {
        }
    }

    /// The fixed command table: every name maps to a compile-time-known spec — no
    /// scope stack consulted. (The M2-transitional `\def`/`\raw`/`\verb` entries,
    /// whose after-effect deltas carried data for absent features, are gone: such
    /// deltas are no longer representable — the M3 storage gating made writing them
    /// a compile-time error.)
    #[derive(Debug, Clone, Copy)]
    pub struct FixedTableResolver;

    impl CommandResolver<CommandsWithoutScopesLang> for FixedTableResolver {
        fn resolve_command(
            &self,
            _state: &ParsingState<CommandsWithoutScopesLang>,
            token: &Token<'_, CommandsWithoutScopesLang>,
        ) -> CommandResolution<CommandsWithoutScopesLang> {
            let TokenKind::Command { name, .. } = &token.kind else {
                return CommandResolution::Unresolved { detail: None };
            };
            let spec: Arc<dyn CallableSpec<CommandsWithoutScopesLang>> = match *name {
                // A plain zero-argument callable.
                "mark" => Arc::new(StdCallableSpec::default()),
                _ => return CommandResolution::Unresolved { detail: None },
            };
            CommandResolution::Resolved(ResolvedCallable {
                callable_type: CT_COMMAND,
                spec,
            })
        }
    }

    // --- shared harness ---------------------------------------------------------------

    /// `{byte range} {summary}` per node — the exact-shape currency
    /// (mirrors the acceptance suite's helper, generic over the language).
    pub fn outline<'t, L: Lang>(
        nodes: impl IntoIterator<Item = NodeRef<'t, L>>,
    ) -> Vec<String> {
        nodes
            .into_iter()
            .map(|node| format!("{:?} {}", node.span().range(), node.summary()))
            .collect()
    }

    /// The whole tree's outline in document order (strict/tolerant comparison key).
    pub fn fingerprint<L: Lang>(result: &ParseResult<L>) -> Vec<String> {
        outline(result.tree.root().descendants())
    }

    /// Parse a happy-path input in **both** recovery modes: assert tree invariants
    /// and zero diagnostics in each, and that the two trees are identical — then
    /// return the strict result for the caller's shape assertions.
    pub fn parse_ok_in<L: Lang>(
        language_for: impl Fn(Recovery) -> Language<L>,
        input: &str,
    ) -> ParseResult<L>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let result = language_for(Recovery::Strict).parse(input).unwrap();
        validate_tree(&result.tree).unwrap();
        assert!(
            result.diagnostics.is_empty(),
            "unexpected strict diagnostics: {:?}",
            result.diagnostics
        );
        let tolerant_result = language_for(Recovery::Tolerant).parse(input).unwrap();
        validate_tree(&tolerant_result.tree).unwrap();
        assert!(
            tolerant_result.diagnostics.is_empty(),
            "unexpected tolerant diagnostics: {:?}",
            tolerant_result.diagnostics
        );
        assert_eq!(
            fingerprint(&result),
            fingerprint(&tolerant_result),
            "strict and tolerant trees diverge on a happy-path input"
        );
        result
    }
}

// -------------------------------------------------------------------------------------
// NoLangFeatures: no feature exists, no rules data is even representable
// -------------------------------------------------------------------------------------

mod plain_chars {
    use super::support::*;
    use std::sync::Arc;
    use techy::core::{
        Language, ParsingState, ParsingStateDelta, StdParseDriver, TokenRulesOverrides,
    };
    use techy::error::Recovery;

    fn language(recovery: Recovery) -> Language<PlainCharsLang> {
        Language::new(StdParseDriver::new(recovery, ()), ParsingState::lang_initial())
    }

    // The input spells every construct — a command escape, a brace group, a `%`
    // comment start, whitespace, the `~` the implemented specials hooks would scan,
    // an `@` — and the language has no features (nor can its rules carry any data:
    // every field of its `TokenRules` is the zero-sized store), so the whole input
    // is one plain chars node. This is the behavior record the populated-seed
    // variant of this test pinned before storage gating made such a seed a
    // compile-time error.
    #[test]
    fn every_construct_spelling_parses_as_plain_character_content() {
        let input = "a\\cmd{b} %c\n\nd~e @f";
        let result = parse_ok_in(language, input);

        let root = result.tree.root();
        assert_eq!(root.child_count(), 1, "outline: {:?}", outline(root.children()));
        let content = root.child(0).unwrap();
        assert!(content.is_chars());
        assert_eq!(content.chars(), Some(input));
        // Spans the entire input: no paragraph split on the double newline, no
        // token error on the forbidden `@`.
        assert_eq!(content.span().range(), 0..19);
        for node in root.descendants() {
            assert!(
                !node.is_group() && !node.is_callable() && !node.is_comment(),
                "unexpected non-chars node: {}",
                node.summary()
            );
        }
    }

    // (The M2-transitional pin "override data for every feature is reported in
    // declaration order" is gone: the delta it built — override data on all eight
    // absent features plus a scope op — is no longer representable. The override
    // fields and the scope-op list of a `ParsingStateDelta<PlainCharsLang>` are
    // zero-sized stores, and the scope-op builders require `LangHasScopes`; writing
    // that delta is now a compile-time error, which is the ruled M3 outcome.)

    // The empty delta derives cleanly even with every feature absent, and the
    // accessors keep answering neutrally.
    #[test]
    fn an_empty_delta_derives_cleanly() {
        let derived = ParsingState::<PlainCharsLang>::lang_initial()
            .derived(&ParsingStateDelta::new())
            .unwrap();
        assert_eq!(derived.rules().whitespace_chars(), "");
    }

    // Ruled 2026-08-10: `disable_all()` is the scoped off for every feature the
    // language *has* — it consults the compile-time declarations. With every feature
    // absent there is nothing for it to mention: the value is the default (every
    // gated field the zero-sized store), and deriving with it succeeds like the
    // empty delta does.
    #[test]
    fn disable_all_mentions_nothing_under_an_all_absent_language() {
        let overrides = TokenRulesOverrides::<PlainCharsLang>::disable_all();
        assert_eq!(overrides, TokenRulesOverrides::default());

        let derived = ParsingState::<PlainCharsLang>::lang_initial()
            .derived(&ParsingStateDelta::new().rules(overrides))
            .expect("disable_all() applies cleanly whatever the language declares");
        // Nothing was flipped — there is nothing to flip: absent features store no
        // data, and the accessor keeps the neutral answer.
        assert!(!derived.rules().commands_enabled());
    }

    // Ruled 2026-08-10: content dispatch intercepts a `Comment` token under an
    // absent comments feature exactly like the other absent features' token kinds —
    // an implementation error that aborts even under tolerant recovery.
    // `StdTokenReader` can never emit the token here (the reader never produces an
    // absent feature's token kinds), so the violating token source is written by
    // hand and the nodes parser is driven directly over it.
    #[test]
    fn a_comment_token_from_a_violating_token_source_is_an_implementation_error() {
        use techy::core::constructs::{
            ConstructParser, ImplementationError, NodesParser, ParseContext, StopSpec,
        };
        use techy::core::{ParserSession, Token, TokenKind, TokenReader, TokenResult};
        use techy::error::DiagnosticInfo;
        use techy::source::{Source, Span};

        struct CommentEmittingReader;
        impl<'s> TokenReader<'s, PlainCharsLang> for CommentEmittingReader {
            fn peek(
                &mut self,
                _state: &Arc<ParsingState<PlainCharsLang>>,
            ) -> TokenResult<'s, PlainCharsLang, Token<'s, PlainCharsLang>> {
                // Spells the `%c` comment of the plain-parse test above — but as a
                // `Comment` token, which the language's declaration rules out.
                Ok(Token::new(
                    TokenKind::Comment {
                        start: Span::new(0, 1),
                        content: "c",
                        post_space: Span::empty(2),
                    },
                    Span::new(0, 2),
                    Span::empty(0),
                ))
            }
            fn move_past(&mut self, _tok: &Token<'s, PlainCharsLang>, _skip: bool) {}
            fn move_to(&mut self, _tok: &Token<'s, PlainCharsLang>, _rewind: bool) {}
            fn move_to_pos(&mut self, _pos: usize) {}
            fn pos(&self) -> usize {
                0
            }
        }

        let driver = StdParseDriver::new(Recovery::Tolerant, ());
        let mut session = ParserSession::new();
        let mut reader = CommentEmittingReader;
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::new(Source::new("%c")),
            Arc::new(ParsingState::<PlainCharsLang>::lang_initial()),
            &mut session,
            &driver,
        );

        let err = NodesParser::new(StopSpec::none()).parse(&mut cx).unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
    }
}

// -------------------------------------------------------------------------------------
// Groups only: one feature present, seven absent
// -------------------------------------------------------------------------------------

mod groups_only {
    use super::support::*;
    use std::sync::Arc;
    use techy::core::{
        Language, ParsingState, ParsingStateDelta, StdParseDriver, StdTokenReader, Token,
        TokenKind, TokenReader, TokenRulesOverrides,
    };
    use techy::error::Recovery;
    use techy::source::Span;

    fn language(recovery: Recovery) -> Language<GroupsOnlyLang> {
        Language::new(StdParseDriver::new(recovery, ()), ParsingState::lang_initial())
    }

    // Braces parse as `Group` nodes; the command escape, `%` comment start, `~`
    // specials trigger, and forbidden `@` are all ordinary characters, and the
    // double newline splits nothing.
    #[test]
    fn braces_parse_as_group_nodes_while_other_constructs_read_as_plain_content() {
        let input = "x{a b}y%z\n\n\\w ~@";
        let result = parse_ok_in(language, input);

        assert_eq!(
            outline(result.tree.root().children()),
            ["0..1 chars(x)", "1..6 group(0 { })", "6..16 chars(y%z\n\n\\w ~@)"]
        );
        let group = result.tree.root().child(1).unwrap();
        assert!(group.is_group());
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
        assert_eq!(outline(group.children()), ["2..5 chars(a b)"]);
    }

    // What whitespace-absent means for the reader: whitespace characters are never
    // skipped into a token's pre-space — they are content characters like any other,
    // and every token's pre-space is empty.
    #[test]
    fn whitespace_characters_are_ordinary_content_tokens_with_empty_pre_space() {
        let state = Arc::new(ParsingState::<GroupsOnlyLang>::lang_initial());
        let mut reader = StdTokenReader::new(" {");

        let token: Token<'_, GroupsOnlyLang> = reader.peek(&state).unwrap();
        assert_eq!(token.kind, TokenKind::Char(' '));
        assert_eq!(token.span, Span::new(0, 1));
        assert_eq!(token.pre_space, Span::empty(0));

        reader.move_past(&token, true);
        let token = reader.peek(&state).unwrap();
        assert!(matches!(&token.kind, TokenKind::GroupOpen { delim: "{", .. }));
        assert_eq!(token.pre_space, Span::empty(1));
    }

    // Ruled 2026-08-10: `disable_all()` flips exactly the present features' gates —
    // here just groups; the absent features' fields are zero-sized stores that can
    // carry nothing, so applying it always succeeds.
    #[test]
    fn disable_all_flips_only_the_groups_gate_and_applies_cleanly() {
        let overrides = TokenRulesOverrides::<GroupsOnlyLang>::disable_all();
        let mut expected = TokenRulesOverrides::<GroupsOnlyLang>::default();
        expected.groups.enabled = Some(false);
        assert_eq!(overrides, expected);

        let derived = ParsingState::<GroupsOnlyLang>::lang_initial()
            .derived(&ParsingStateDelta::new().rules(overrides))
            .expect("disable_all() applies cleanly under a partially-absent language");
        assert!(!derived.rules().groups_enabled());
    }
}

// -------------------------------------------------------------------------------------
// Commands without scopes: callables do not imply scopes
// -------------------------------------------------------------------------------------

mod commands_without_scopes {
    use super::support::*;
    use std::sync::Arc;
    use techy::core::specs::{Package, ScopeOp, ScopeOpError, ScopeStack, SpecsProvider};
    use techy::core::{
        Language, ParsingState, ParsingStateDelta, StdParseDriver, TokenRulesOverrides,
        WhitespaceOverrides,
    };
    use techy::error::Recovery;

    fn language(recovery: Recovery) -> Language<CommandsWithoutScopesLang> {
        Language::new(
            StdParseDriver::new(recovery, FixedTableResolver),
            ParsingState::lang_initial(),
        )
    }

    // `\mark` resolves through the driver's fixed table into a `Callable` node — no
    // scope stack anywhere. Groups, comments, and forbidden chars are absent (their
    // rules fields cannot carry data), so `{h}`, `%i`, and `@` read as ordinary
    // characters; whitespace is live (the command consumes its post-space).
    #[test]
    fn commands_parse_into_callable_nodes_from_a_fixed_table() {
        let input = r"\mark {h} %i @";
        let result = parse_ok_in(language, input);

        assert_eq!(
            outline(result.tree.root().children()),
            ["0..6 1(mark)", "6..14 chars({h} %i @)"]
        );
        let callable = result.tree.root().child(0).unwrap();
        assert!(callable.is_callable());
        assert_eq!(callable.name(), Some("mark"));
        assert_eq!(callable.callable_type(), Some(CT_COMMAND));
    }

    // Whitespace present, paragraphs absent: a double newline is consumable
    // whitespace like any other — no paragraph break splits the run.
    #[test]
    fn double_newlines_do_not_split_paragraphs() {
        let result = parse_ok_in(language, "a\n\nb");
        assert_eq!(outline(result.tree.root().children()), ["0..4 chars(a\n\nb)"]);
    }

    // (Four M2-transitional pins are gone, all made unwritable by the M3 storage
    // gating — the ruled outcome: a scope-op after-effect (`\def`) and a
    // comments-override after-effect (`\raw`) no longer have a representable delta
    // to route through the in-parse funnel; the out-of-parse scope-op `DeriveError`
    // report needed `.scope_op(…)`, which now requires `LangHasScopes`; and the
    // explicit comments-data override was a `comments:` field literal on a
    // zero-sized store. Their M3 replacements are the compile facts in
    // `feature_composition` and the positive application tests kept here.)

    // A delta touching only present features applies cleanly — and data for an
    // absent feature cannot ride along: a `comments:` (or any other absent-feature)
    // literal on this language's `TokenRulesOverrides` is a compile-time type error.
    #[test]
    fn overrides_for_present_features_apply_cleanly() {
        let seed = ParsingState::<CommandsWithoutScopesLang>::lang_initial();
        let derived = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                whitespace: WhitespaceOverrides { enabled: None, chars: Some("Z".into()) },
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        assert_eq!(derived.rules().whitespace_chars(), "Z");
    }

    // Ruled 2026-08-10: under this two-present-features language, `disable_all()`
    // names exactly whitespace and commands — the absent features' fields are
    // zero-sized stores carrying nothing — and the delta it seeds applies cleanly.
    #[test]
    fn disable_all_names_only_the_present_features_and_applies_cleanly() {
        let overrides = TokenRulesOverrides::<CommandsWithoutScopesLang>::disable_all();
        let mut expected = TokenRulesOverrides::<CommandsWithoutScopesLang>::default();
        expected.whitespace.enabled = Some(false);
        expected.commands.enabled = Some(false);
        assert_eq!(overrides, expected);

        let derived = ParsingState::<CommandsWithoutScopesLang>::lang_initial()
            .derived(&ParsingStateDelta::new().rules(overrides))
            .expect("disable_all() applies cleanly under a partially-absent language");
        assert!(!derived.rules().whitespace_enabled());
        assert!(!derived.rules().commands_enabled());
        // The absent blocks store no data; their accessors answer neutrally.
        assert!(!derived.rules().comments_enabled());
    }

    // (The M2-transitional verbatim pin is gone the same way: `verbatim_state_delta`
    // now carries the `LangHasGroups` bound the plan promised, so
    // `verbatim_state_delta::<CommandsWithoutScopesLang>` does not compile at all —
    // the application-time error it pinned has no delta left to trigger it. The
    // positive half of the compile fact is pinned in `feature_composition`.)

    // Scopes absent: the state still stores a scope stack — the field exists for
    // every language — but its storage is the zero-sized store, so the stack is
    // permanently empty and every read gives the empty-stack answer.
    #[test]
    fn the_scope_stack_of_a_scopes_absent_language_is_permanently_empty() {
        let state = ParsingState::<CommandsWithoutScopesLang>::lang_initial();
        assert!(state.scopes().is_empty());
        assert!(state.scopes().providers().is_empty());
        assert_eq!(state.scopes().len(), 0);
    }

    // The one remaining runtime answer of the scope feature: `apply_op` called
    // directly on a stack of a scopes-absent language reports `ScopesAbsent` — loud,
    // no panic, nothing applied. A state delta can never trigger this (its scope-op
    // list is storage-gated and the builders carry the `LangHasScopes` bound); only
    // a direct call reaches it.
    #[test]
    fn apply_op_on_a_scopes_absent_stack_reports_scopes_absent_without_panicking() {
        let mut stack: ScopeStack<CommandsWithoutScopesLang> = ScopeStack::new();
        let provider: Arc<dyn SpecsProvider<CommandsWithoutScopesLang>> =
            Arc::new(Package::new("late"));
        let error = stack.apply_op(&ScopeOp::Push(provider)).unwrap_err();
        assert_eq!(error, ScopeOpError::ScopesAbsent);
        assert!(stack.is_empty());
    }
}

// -------------------------------------------------------------------------------------
// The compositions, pinned at the type level
// -------------------------------------------------------------------------------------

mod feature_composition {
    use super::support::{CommandsWithoutScopesLang, GroupsOnlyLang};
    use std::sync::Arc;
    use techy::core::constructs::verbatim_state_delta;
    use techy::core::specs::{ScopeOp, ScopeStack, SpecsProvider};
    use techy::core::{
        FeaturePresence, GroupRule, Lang, LangFeatures, LangHasCommands, LangHasGroups,
        LangHasScopes, LangHasWhitespace, ParsingStateDelta, TrivialLang,
    };

    fn requires_groups<L: LangHasGroups>() {}
    fn requires_commands<L: LangHasCommands>() {}
    fn requires_whitespace<L: LangHasWhitespace>() {}

    // Positive bound checks: taking the function pointers forces them when the test
    // profile compiles.
    const _: fn() = requires_groups::<GroupsOnlyLang>;
    const _: fn() = requires_commands::<CommandsWithoutScopesLang>;
    const _: fn() = requires_whitespace::<CommandsWithoutScopesLang>;

    // The positive halves of the M3 compile facts that retired the transitional
    // runtime pins (the negative halves — a groups-absent or scopes-absent language
    // in these positions — do not compile, which is the fact itself):
    //
    // `verbatim_state_delta` requires the groups feature (its terminator is groups
    // data); a groups-present language satisfies the bound.
    fn mint_verbatim_delta<L: LangHasGroups>(
        terminator: Arc<GroupRule<L>>,
    ) -> ParsingStateDelta<L> {
        verbatim_state_delta(terminator)
    }
    const _: fn(Arc<GroupRule<GroupsOnlyLang>>) -> ParsingStateDelta<GroupsOnlyLang> =
        mint_verbatim_delta::<GroupsOnlyLang>;

    // The delta's scope-op builders require the scopes feature; a scopes-present
    // language (every `TrivialLang` declares all features) satisfies the bound.
    #[derive(Debug, Clone, Copy)]
    struct AllPresentLang;
    impl TrivialLang for AllPresentLang {}
    fn add_scope_op<L: LangHasScopes>(
        delta: ParsingStateDelta<L>,
        op: ScopeOp<L>,
    ) -> ParsingStateDelta<L> {
        delta.scope_op(op)
    }
    const _: fn(
        ParsingStateDelta<AllPresentLang>,
        ScopeOp<AllPresentLang>,
    ) -> ParsingStateDelta<AllPresentLang> = add_scope_op::<AllPresentLang>;

    // `ScopeStack::push` — direct stack mutation — requires the scopes feature the
    // same way; under the bound the stack's storage is transparent and the push is a
    // plain list push. (The negative half — pushing onto a scopes-absent language's
    // stack — does not compile, which is the fact itself.)
    fn push_onto_scope_stack<L: LangHasScopes>(
        mut stack: ScopeStack<L>,
        provider: Arc<dyn SpecsProvider<L>>,
    ) -> ScopeStack<L> {
        stack.push(provider);
        stack
    }
    const _: fn(
        ScopeStack<AllPresentLang>,
        Arc<dyn SpecsProvider<AllPresentLang>>,
    ) -> ScopeStack<AllPresentLang> = push_onto_scope_stack::<AllPresentLang>;

    type GroupsOnlyDecl = <GroupsOnlyLang as Lang>::Features;
    const _: () =
        assert!(<<GroupsOnlyDecl as LangFeatures>::Groups as FeaturePresence>::PRESENT);
    const _: () =
        assert!(!<<GroupsOnlyDecl as LangFeatures>::Whitespace as FeaturePresence>::PRESENT);
    const _: () =
        assert!(!<<GroupsOnlyDecl as LangFeatures>::Scopes as FeaturePresence>::PRESENT);

    type CommandsDecl = <CommandsWithoutScopesLang as Lang>::Features;
    const _: () =
        assert!(<<CommandsDecl as LangFeatures>::Commands as FeaturePresence>::PRESENT);
    const _: () =
        assert!(<<CommandsDecl as LangFeatures>::Whitespace as FeaturePresence>::PRESENT);
    // The lattice ruling, as a compile-time fact: callables do not imply scopes —
    // and this language needs no groups either (zero-argument callables).
    const _: () =
        assert!(!<<CommandsDecl as LangFeatures>::Scopes as FeaturePresence>::PRESENT);
    const _: () =
        assert!(!<<CommandsDecl as LangFeatures>::Groups as FeaturePresence>::PRESENT);
}

/// Static storage-collapse regression checks (all `const` asserts — verified when the
/// test profile compiles, no `#[test]` runs). Two directions:
///
/// - **Collapse**: for a language declaring every feature absent
///   ([`support::PlainCharsLang`], `NoLangFeatures`), every gated store is zero-sized,
///   so the rules, overrides, scope stack, state data, and frozen state (with its two
///   derived caches) occupy no storage at all. These checks are
///   platform-independent.
/// - **Transparency**: for an all-features-present language, the gated stores are the
///   payload types themselves, so the sizes are exactly what they were before storage
///   gating existed (measured on the tree before storage gating landed, 2026-08-06;
///   the asserts below are the durable record). Pointer-size dependent, so pinned for
///   64-bit targets only.
mod storage_collapse {
    use core::mem::size_of;

    use techy::core::specs::ScopeStack;
    use techy::core::{
        ParsingState, ParsingStateDelta, StateData, TokenRules, TokenRulesOverrides,
        TrivialLang,
    };

    use super::support::PlainCharsLang;

    /// All features present (`TrivialLang` declares `AllLangFeatures`).
    #[derive(Debug, Clone, Copy)]
    struct AllPresentLang;
    impl TrivialLang for AllPresentLang {}

    // Collapse: every feature absent — the gated storage vanishes entirely.
    const _: () = assert!(size_of::<TokenRules<PlainCharsLang>>() == 0);
    const _: () = assert!(size_of::<TokenRulesOverrides<PlainCharsLang>>() == 0);
    const _: () = assert!(size_of::<ScopeStack<PlainCharsLang>>() == 0);
    const _: () = assert!(size_of::<StateData<PlainCharsLang>>() == 0);
    const _: () = assert!(size_of::<ParsingState<PlainCharsLang>>() == 0);

    // The delta keeps only its ungated parts (`mode`/`ext` — here `Option<()>` each —
    // and the `events` list); its rules-override and scope-op storage is gone.
    #[cfg(target_pointer_width = "64")]
    const _: () = assert!(size_of::<ParsingStateDelta<PlainCharsLang>>() == 32);

    // Transparency: all features present — sizes identical to the pre-storage-gating
    // tree (the present store *is* the payload; gating adds nothing).
    #[cfg(target_pointer_width = "64")]
    mod all_present_sizes_unchanged {
        use super::*;

        const _: () = assert!(size_of::<TokenRules<AllPresentLang>>() == 176);
        const _: () = assert!(size_of::<TokenRulesOverrides<AllPresentLang>>() == 184);
        const _: () = assert!(size_of::<ParsingStateDelta<AllPresentLang>>() == 240);
        const _: () = assert!(size_of::<StateData<AllPresentLang>>() == 200);
        const _: () = assert!(size_of::<ParsingState<AllPresentLang>>() == 232);
        const _: () = assert!(size_of::<ScopeStack<AllPresentLang>>() == 24);
    }
}
