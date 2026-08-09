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
//!   everything else absent (the PLAN's lattice ruling: callables do **not** imply
//!   scopes). The seed populates exactly those two blocks. Commands resolve through
//!   a fixed table on the driver — no scope stack — and a state change carrying
//!   *override* data for an absent feature (a scope op, a comments override, the
//!   verbatim recipe's group data — the override structs are not storage-gated)
//!   errors through the standard recovery funnel as an implementation error, never
//!   a panic (out of parse: a `DeriveError` carrying
//!   [`AbsentFeatureOverrideError`]).
//!
//! Composition note for the commands language: whitespace is present because command
//! tokenization consumes post-space through `skip_whitespace`; groups stay absent
//! because zero-argument callables need no group machinery (the argument parsers
//! that mint temporary group rules are exactly the ones that do — they gain a
//! `LangHasGroups` bound at M3).
//!
//! [`Lang`]: techy::core::Lang
//! [`AbsentFeatureOverrideError`]: techy::core::AbsentFeatureOverrideError

mod support {
    use std::sync::Arc;

    use techy::core::constructs::{
        ConstructParser, ConstructParserResult, FromInvocation, Invocation, ParseContext,
        StdInvocationParser,
    };
    use techy::core::node::{validate_tree, BuildId, NodeKind, NodeRef, StagedChildren};
    use techy::core::specs::{
        CallableSpec, CommandResolution, Package, ResolvedCallable, ScopeStack,
        StdCallableSpec,
    };
    use techy::core::{
        CommandResolver, CommandRule, CommandRules, CommentOverrides, CommentRule,
        FeatureAbsent, FeaturePresent, GroupRule, GroupRules, Lang, LangFeatures,
        Language, NoLangFeatures, ParseResult, ParsingState, ParsingStateDelta,
        SpecialsMatch, StateData, StdParseDriver, Token, TokenKind, TokenResult,
        TokenRules, TokenRulesOverrides, TriggerChars, WhitespaceRules,
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
    /// scope stack consulted. `\def`, `\raw`, and `\verb` deliberately return
    /// after-effect deltas that carry *override* data for absent features (the
    /// override structs are not storage-gated), so tests can drive those violations
    /// through the in-parse recovery funnel.
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
                // After-effect: a scope op under a language without the scope stack.
                "def" => Arc::new(AfterEffectSpec {
                    delta: ParsingStateDelta::new()
                        .push_provider(Arc::new(Package::new("defs"))),
                }),
                // After-effect: comment rules data under absent comments.
                "raw" => Arc::new(AfterEffectSpec {
                    delta: ParsingStateDelta::new().rules(TokenRulesOverrides {
                        comments: CommentOverrides {
                            enabled: None,
                            rules: Some(vec![Arc::new(CommentRule { start: "#".into() })]),
                        },
                        ..TokenRulesOverrides::default()
                    }),
                }),
                // After-effect: the verbatim reading recipe, whose expected-close
                // terminator is groups data — absent here.
                "verb" => Arc::new(AfterEffectSpec {
                    delta: techy::core::constructs::verbatim_state_delta(Arc::new(
                        GroupRule { group_type: 0, open: "|".into(), close: "|".into() },
                    )),
                }),
                _ => return CommandResolution::Unresolved { detail: None },
            };
            CommandResolution::Resolved(ResolvedCallable {
                callable_type: CT_COMMAND,
                spec,
            })
        }
    }

    /// A zero-argument callable that stages the standard invocation node and then
    /// returns `delta` as its after-effect for subsequent siblings (the
    /// `\newcommand` shape) — the channel that routes a delta through the in-parse
    /// derivation funnel.
    #[derive(Debug)]
    pub struct AfterEffectSpec {
        pub delta: ParsingStateDelta<CommandsWithoutScopesLang>,
    }

    impl CallableSpec<CommandsWithoutScopesLang> for AfterEffectSpec {
        fn make_invocation_parser<'a, 's>(
            &'a self,
            invocation: Invocation<'a, 's, CommandsWithoutScopesLang>,
        ) -> Box<dyn ConstructParser<CommandsWithoutScopesLang, Output = BuildId> + 'a>
        where
            's: 'a,
        {
            Box::new(AfterEffectParser {
                inner: StdInvocationParser::new(invocation),
                delta: self.delta.clone(),
            })
        }
    }

    struct AfterEffectParser<'a, 's> {
        inner: StdInvocationParser<'a, 's, CommandsWithoutScopesLang>,
        delta: ParsingStateDelta<CommandsWithoutScopesLang>,
    }

    impl ConstructParser<CommandsWithoutScopesLang> for AfterEffectParser<'_, '_> {
        type Output = BuildId;
        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, CommandsWithoutScopesLang>,
        ) -> ConstructParserResult<
            CommandsWithoutScopesLang,
            (BuildId, Option<ParsingStateDelta<CommandsWithoutScopesLang>>),
        > {
            let (id, _) = self.inner.parse(cx)?;
            Ok((id, Some(self.delta.clone())))
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
    use techy::core::specs::{Package, ScopeOp};
    use techy::core::{
        CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
        Language, ParagraphOverrides, ParsingState, ParsingStateDelta, SpecialsOverrides,
        StdParseDriver, TokenRulesOverrides, WhitespaceOverrides,
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

    // A delta carrying data for every feature block plus a scope op reports all
    // eight features, in declaration order, and applies none of it.
    #[test]
    fn override_data_for_every_feature_is_reported_in_declaration_order() {
        let delta = ParsingStateDelta::<PlainCharsLang>::new()
            .rules(TokenRulesOverrides {
                whitespace: WhitespaceOverrides { enabled: Some(false), chars: None },
                paragraphs: ParagraphOverrides::disable(),
                groups: GroupOverrides { enabled: Some(false), ..GroupOverrides::default() },
                commands: CommandOverrides::disable(),
                comments: CommentOverrides::disable(),
                specials: SpecialsOverrides::disable(),
                forbidden_chars: ForbiddenCharsOverrides { chars: Some("!".into()) },
            })
            .scope_op(ScopeOp::Push(Arc::new(Package::new("pkg"))));

        let err = ParsingState::<PlainCharsLang>::lang_initial().derived(&delta).unwrap_err();
        let report = err.absent_overrides.as_ref().expect("absent-feature report");
        assert_eq!(
            report.features(),
            [
                "whitespace",
                "paragraphs",
                "groups",
                "commands",
                "comments",
                "specials",
                "forbidden_chars",
                "scopes",
            ]
        );
        // The scope op was never attempted (it is the "scopes" violation, not an op
        // failure), and nothing of the delta was applied to the recovered state:
        // with every field the zero-sized store there is nowhere for override data
        // to land, and every accessor keeps the absent feature's neutral answer.
        assert!(err.failures.is_empty());
        assert!(err.finalize_error.is_none());
        assert!(!err.recovered.rules().commands_enabled());
        assert_eq!(err.recovered.rules().forbidden_chars(), "");
        assert!(err.recovered.scopes().providers().is_empty());
    }

    // All-`None` blocks carry nothing: the empty delta derives cleanly even with
    // every feature absent, and the accessors keep answering neutrally.
    #[test]
    fn an_empty_delta_derives_cleanly() {
        let derived = ParsingState::<PlainCharsLang>::lang_initial()
            .derived(&ParsingStateDelta::new())
            .unwrap();
        assert_eq!(derived.rules().whitespace_chars(), "");
    }

    // Ruled 2026-08-10: `disable_all()` is the scoped off for every feature the
    // language *has* — it consults the compile-time declarations. With every feature
    // absent there is nothing for it to mention: the value is the all-`None` default,
    // and deriving with it succeeds like the empty delta does.
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
    fn braces_parse_as_group_nodes_while_other_rules_data_is_inert() {
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
    // here just groups — and leaves absent features' blocks all-`None`, so applying
    // it cannot report an absent-feature violation.
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
    use techy::core::constructs::{verbatim_state_delta, ImplementationError};
    use techy::core::specs::{Package, ScopeOp};
    use techy::core::{
        CommentOverrides, GroupRule, Language, ParsingState, ParsingStateDelta,
        StdParseDriver, TokenRulesOverrides, WhitespaceOverrides,
    };
    use techy::error::{DiagnosticInfo, Recovery};

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

    // A spec's after-effect delta pushes a provider — a scope op under a language
    // that declares the scope stack absent. The violation surfaces through the
    // in-parse derivation funnel as an implementation error, under *both* recovery
    // policies, never a panic.
    #[test]
    fn an_in_parse_scope_op_aborts_as_an_implementation_error_not_a_panic() {
        for recovery in [Recovery::Strict, Recovery::Tolerant] {
            let err = language(recovery).parse(r"\def x").unwrap_err();
            assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
            assert!(err.to_string().contains("scopes"), "{err}");
        }
    }

    // The same funnel for a rules override: an after-effect delta carrying comment
    // rules data under absent comments.
    #[test]
    fn an_in_parse_override_carrying_comments_data_aborts_the_same_way() {
        for recovery in [Recovery::Strict, Recovery::Tolerant] {
            let err = language(recovery).parse(r"\raw x").unwrap_err();
            assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
            assert!(err.to_string().contains("comments"), "{err}");
        }
    }

    // Out of parse, the same violation is a `DeriveError`: the scope op is reported
    // as the "scopes" feature violation (not an op failure), it never applies, and
    // the delta's present-feature parts still apply to the recovered state.
    #[test]
    fn scope_ops_error_out_of_parse_and_the_rest_of_the_delta_still_applies() {
        let delta = ParsingStateDelta::<CommandsWithoutScopesLang>::new()
            .rules(TokenRulesOverrides {
                whitespace: WhitespaceOverrides { enabled: None, chars: Some("XY".into()) },
                ..TokenRulesOverrides::default()
            })
            .scope_op(ScopeOp::Push(Arc::new(Package::new("late"))));

        let err =
            ParsingState::<CommandsWithoutScopesLang>::lang_initial().derived(&delta).unwrap_err();
        assert_eq!(err.absent_overrides.as_ref().unwrap().features(), ["scopes"]);
        assert!(err.failures.is_empty());
        assert_eq!(err.recovered.rules().whitespace_chars(), "XY");
        assert!(err.recovered.scopes().providers().is_empty());
    }

    // The gated-overrides contract, block by block: explicitly authored data on an
    // absent feature's block errors; an all-`None` block for an absent feature stays
    // silent.
    #[test]
    fn explicit_data_for_an_absent_feature_errors_while_an_all_none_block_stays_silent() {
        let seed = ParsingState::<CommandsWithoutScopesLang>::lang_initial();

        let err = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                comments: CommentOverrides::disable(),
                ..TokenRulesOverrides::default()
            }))
            .unwrap_err();
        assert_eq!(err.absent_overrides.unwrap().features(), ["comments"]);

        // Same delta shape minus the comments data: the absent blocks are all-`None`
        // and stay silent; the whitespace override (present feature) applies.
        let derived = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                whitespace: WhitespaceOverrides { enabled: None, chars: Some("Z".into()) },
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        assert_eq!(derived.rules().whitespace_chars(), "Z");
    }

    // Ruled 2026-08-10: under this two-present-features language, `disable_all()`
    // names exactly whitespace and commands — the absent features' blocks stay
    // all-`None` — and the delta it seeds applies without any absent-feature report.
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

    // M2-transitional behavior (user-accepted 2026-08-10): `verbatim_state_delta`
    // installs its terminator as `expecting_close` — groups data — so under a
    // groups-absent language it errors at application time; M3 replaces this with a
    // `LangHasGroups` compile bound on the verbatim family. Only membership of
    // "groups" is asserted — written before the feature-aware `disable_all()` rework
    // (ruled 2026-08-10, since landed): the report is now exactly ["groups"], the
    // delta's explicitly authored groups literal.
    #[test]
    fn verbatim_state_delta_errors_at_application_under_absent_groups() {
        let terminator = || {
            Arc::new(GroupRule::<CommandsWithoutScopesLang> {
                group_type: 0,
                open: "|".into(),
                close: "|".into(),
            })
        };

        // Out of parse: application via `derived()`.
        let err = ParsingState::<CommandsWithoutScopesLang>::lang_initial()
            .derived(&verbatim_state_delta(terminator()))
            .unwrap_err();
        let report = err.absent_overrides.expect("absent-feature report");
        assert!(report.features().contains(&"groups"), "{:?}", report.features());

        // In parse: `\verb`'s after-effect carries the same delta through the
        // recovery funnel — an implementation error under both policies, no panic.
        for recovery in [Recovery::Strict, Recovery::Tolerant] {
            let err = language(recovery).parse(r"\verb|x|").unwrap_err();
            assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
            assert!(err.to_string().contains("groups"), "{err}");
        }
    }
}

// -------------------------------------------------------------------------------------
// The compositions, pinned at the type level
// -------------------------------------------------------------------------------------

mod feature_composition {
    use super::support::{CommandsWithoutScopesLang, GroupsOnlyLang};
    use techy::core::{
        FeaturePresence, Lang, LangFeatures, LangHasCommands, LangHasGroups,
        LangHasWhitespace,
    };

    fn requires_groups<L: LangHasGroups>() {}
    fn requires_commands<L: LangHasCommands>() {}
    fn requires_whitespace<L: LangHasWhitespace>() {}

    // Positive bound checks: taking the function pointers forces them when the test
    // profile compiles.
    const _: fn() = requires_groups::<GroupsOnlyLang>;
    const _: fn() = requires_commands::<CommandsWithoutScopesLang>;
    const _: fn() = requires_whitespace::<CommandsWithoutScopesLang>;

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
