//! [`display_tree`]: a human-oriented one-line-per-node subtree rendering.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::source::{LineIndex, Source, SourceOrigin, SourceProvenance, SourceSpan};
use crate::state::Lang;

use super::node_ref::NodeRef;

/// Render the subtree under `node` for human eyes: one line per node — box-drawing
/// guides, the node's [`summary`](NodeRef::summary), and its span as `line:column`
/// positions (computed through each source's lazy
/// [`LineIndex`](crate::source::LineIndex); spans in a source too large to index
/// fall back to raw byte offsets):
///
/// ```text
/// list(3) @ 1:1..1:20
/// ├── chars(x) @ 1:1..1:2
/// ├── Macro(frac) @ 1:2..1:13
/// │   ├── group(? { }) @ 1:7..1:10
/// │   └── group(? { }) @ 1:10..1:13
/// └── comment( note) @ 1:14..1:20
/// ```
///
/// On multi-source trees (attached `\input` content, cross-tree splices), a line
/// whose node lives in a **different source than the previous line's** carries that
/// source's name (`[source: …]` — the origin label when one is set, otherwise the
/// provenance's reference/description); the initial source is not named.
///
/// The output is human-oriented and **not a stability contract** (the same caveat as
/// [`summary`](NodeRef::summary)) — compare trees structurally where exactness
/// matters beyond a test's lifetime. Annotations are ignored.
///
/// A free function, deliberately not a `NodeRef`/`NodeTree` method: display sugar
/// stays out of the core read surface and is dead-code-eliminated when unused.
pub fn display_tree<L: Lang, A>(node: NodeRef<'_, L, A>) -> String {
    let mut renderer = Renderer {
        out: String::new(),
        indexes: Vec::new(),
        previous_source: None,
    };
    renderer.line(node, "");
    render_children(&mut renderer, node, "");
    renderer.out
}

fn render_children<'t, L: Lang, A>(
    renderer: &mut Renderer<'t, L::SourceOrigin>,
    node: NodeRef<'t, L, A>,
    prefix: &str,
) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else {
            // child_count() bounds the loop; unreachable.
            continue;
        };
        let last = i + 1 == count;
        renderer.line(child, &format!("{prefix}{}", if last { "└── " } else { "├── " }));
        render_children(
            renderer,
            child,
            &format!("{prefix}{}", if last { "    " } else { "│   " }),
        );
    }
}

struct Renderer<'t, O: SourceOrigin> {
    out: String,
    /// One lazy [`LineIndex`] per distinct `Source` Arc seen, keyed by Arc identity.
    indexes: Vec<(*const Source<O>, LineIndex<'t>)>,
    /// The source of the previously rendered line (Arc identity); `None` before the
    /// first line — the initial source is never named.
    previous_source: Option<*const Source<O>>,
}

impl<'t, O: SourceOrigin + 't> Renderer<'t, O> {
    fn line<L: Lang<SourceOrigin = O>, A>(&mut self, node: NodeRef<'t, L, A>, prefix: &str) {
        let span: &'t SourceSpan<O> = node.span();
        let ptr = Arc::as_ptr(span.source());

        self.out.push_str(prefix);
        self.out.push_str(&node.summary());

        let position = {
            let index = self.index_for(span.source());
            (index.line_col(span.start()), index.line_col(span.end()))
        };
        match position {
            (Some((l1, c1)), Some((l2, c2))) => {
                let _ = write!(self.out, " @ {l1}:{c1}..{l2}:{c2}");
            }
            _ => {
                let _ = write!(self.out, " @ bytes {}..{}", span.start(), span.end());
            }
        }

        if let Some(previous) = self.previous_source {
            if previous != ptr {
                let _ = write!(self.out, " [source: {}]", source_name(span.source()));
            }
        }
        self.previous_source = Some(ptr);
        self.out.push('\n');
    }

    fn index_for(&mut self, source: &'t Arc<Source<O>>) -> &mut LineIndex<'t> {
        let ptr = Arc::as_ptr(source);
        if let Some(i) = self.indexes.iter().position(|(p, _)| *p == ptr) {
            return &mut self.indexes[i].1;
        }
        self.indexes.push((ptr, source.line_index()));
        &mut self.indexes.last_mut().expect("just pushed").1
    }
}

/// A display name for a source: the origin label when set, otherwise the
/// provenance's reference (resolved sources) or description (synthesized sources),
/// `<primary>` for an unlabeled primary source.
fn source_name<O: SourceOrigin>(source: &Source<O>) -> String {
    if let Some(label) = source.origin().label() {
        return label.into_owned();
    }
    match source.provenance() {
        SourceProvenance::Primary => "<primary>".to_string(),
        SourceProvenance::Resolved { reference, .. } => reference.clone(),
        SourceProvenance::Synthesized { description, .. } => {
            alloc::format!("<{description}>")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::builder::NodeTreeBuilder;
    use super::super::kind::{CallableData, GroupData, NodeKind};
    use super::super::tree::NodeTree;
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::source::Span;
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{ParsingState, StateData, TrivialLang};
    use crate::token::{
        CommandRules, CommentRules, ForbiddenCharsRules, GroupRules, ParagraphRules,
        SpecialsRules, TokenRules, WhitespaceRules,
    };
    use alloc::vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    fn state() -> Arc<ParsingState<PlainLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: TokenRules {
                whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
                paragraphs: ParagraphRules { enabled: true },
                groups: GroupRules {
                    enabled: true,
                    rules: Vec::new(),
                    temporary: Vec::new(),
                    expecting_close: None,
                },
                commands: CommandRules {
                    enabled: true,
                    rules: Vec::new(),
                },
                comments: CommentRules {
                    enabled: true,
                    rules: Vec::new(),
                },
                specials: SpecialsRules { enabled: true },
                forbidden_chars: ForbiddenCharsRules { chars: "".into() },
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// `"a\n{bc}"`: root List [chars(a), group [chars(bc)]] — the group on source
    /// line 2 (the newline byte is deliberately unrepresented; `finish()` does no
    /// byte accounting, and newline-free chars payloads keep each rendered line
    /// single).
    fn two_line_tree() -> NodeTree<PlainLang> {
        let source: Arc<Source> = Arc::new(Source::new("a\n{bc}"));
        let st = state();
        let mut b: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let a = b.add(
            NodeKind::chars(Span::new(0, 1)),
            SourceSpan::new(&source, 0..1),
            st.clone(),
            vec![], (), (),
        ).unwrap();
        let bc = b.add(
            NodeKind::chars(Span::new(3, 5)),
            SourceSpan::new(&source, 3..5),
            st.clone(),
            vec![], (), (),
        ).unwrap();
        let group = b.add(
            NodeKind::group(GroupData::new(0u32, Span::new(2, 3), Span::new(5, 6))),
            SourceSpan::new(&source, 2..6),
            st.clone(),
            vec![bc], (), (),
        ).unwrap();
        let root = b.add(
            NodeKind::list(),
            SourceSpan::entire(&source),
            st.clone(),
            vec![a, group], (), (),
        ).unwrap();
        b.finish(root).unwrap()
    }

    #[test]
    fn one_line_per_node_with_guides_and_line_col() {
        let tree = two_line_tree();
        let rendered = display_tree(tree.root());
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), tree.node_count(), "one line per node:\n{rendered}");

        // Root: summary + whole-span line/col, no guides.
        assert_eq!(lines[0], "list(2) @ 1:1..2:5");
        // Children carry guides; the group (last child) guides its own child deeper.
        assert_eq!(lines[1], "├── chars(a) @ 1:1..1:2");
        assert_eq!(lines[2], "└── group(0 { }) @ 2:1..2:5");
        assert_eq!(lines[3], "    └── chars(bc) @ 2:2..2:4");
    }

    #[test]
    fn multibyte_first_character_renders() {
        // Regression: the line index resumed mid-character, so any document whose
        // first character is multi-byte panicked the renderer. Same tree shape
        // over a 2-byte and a 4-byte first character (`é` and `😀`, each followed
        // by the 3-byte `—` and the ASCII `{x}`); byte offsets differ per source.
        // Columns count bytes, so the pinned root line ends at byte length + 1.
        for (content, expected_root_line) in [
            ("é—{x}", "list(2) @ 1:1..1:9"),   // 2 + 3 + 3 = 8 bytes
            ("😀—{x}", "list(2) @ 1:1..1:11"), // 4 + 3 + 3 = 10 bytes
        ] {
            let text_end = content.len() - 3; // the trailing ASCII "{x}"
            let source: Arc<Source> = Arc::new(Source::new(content));
            let st = state();
            let mut b: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
            let text = b.add(
                NodeKind::chars(Span::new(0, text_end)),
                SourceSpan::new(&source, 0..text_end),
                st.clone(),
                vec![], (), (),
            ).unwrap();
            let x = b.add(
                NodeKind::chars(Span::new(text_end + 1, text_end + 2)),
                SourceSpan::new(&source, text_end + 1..text_end + 2),
                st.clone(),
                vec![], (), (),
            ).unwrap();
            let group = b.add(
                NodeKind::group(GroupData::new(
                    0u32,
                    Span::new(text_end, text_end + 1),
                    Span::new(text_end + 2, text_end + 3),
                )),
                SourceSpan::new(&source, text_end..text_end + 3),
                st.clone(),
                vec![x], (), (),
            ).unwrap();
            let root = b.add(
                NodeKind::list(),
                SourceSpan::entire(&source),
                st.clone(),
                vec![text, group], (), (),
            ).unwrap();
            let tree = b.finish(root).unwrap();

            let rendered = display_tree(tree.root());
            let first_line = rendered.lines().next().unwrap_or_default();
            assert_eq!(
                first_line, expected_root_line,
                "root line of {content:?}:\n{rendered}"
            );
        }
    }

    #[test]
    fn source_name_printed_only_on_change() {
        // main (labeled) holds x·y; an attached resolved source "f" holds the
        // middle child. Expect: no name on the first lines (initial source),
        // "f" where the attached line starts, main's label where it returns.
        let main: Arc<Source> =
            Arc::new(Source::new("xy").with_origin(Some("main.tex".into())));
        let inc: Arc<Source> =
            Arc::new(Source::resolved("ab", "f", SourceSpan::new(&main, 0..1)));
        let st = state();
        let mut b: NodeTreeBuilder<PlainLang> = NodeTreeBuilder::new();
        let x = b.add(
            NodeKind::chars(Span::new(0, 1)),
            SourceSpan::new(&main, 0..1),
            st.clone(),
            vec![], (), (),
        ).unwrap();
        let ab = b.add(
            NodeKind::chars(Span::new(0, 2)),
            SourceSpan::entire(&inc),
            st.clone(),
            vec![], (), (),
        ).unwrap();
        let y = b.add(
            NodeKind::chars(Span::new(1, 2)),
            SourceSpan::new(&main, 1..2),
            st.clone(),
            vec![], (), (),
        ).unwrap();
        let root = b.add(
            NodeKind::list(),
            SourceSpan::entire(&main),
            st.clone(),
            vec![x, ab, y], (), (),
        ).unwrap();
        let tree = b.finish(root).unwrap();

        let rendered = display_tree(tree.root());
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(!lines[0].contains("[source:"), "initial source named: {:?}", lines[0]);
        assert!(!lines[1].contains("[source:"), "unchanged source named: {:?}", lines[1]);
        assert!(lines[2].ends_with("[source: f]"), "attached line: {:?}", lines[2]);
        assert!(
            lines[3].ends_with("[source: main.tex]"),
            "return-to-main line: {:?}",
            lines[3]
        );
    }

    #[test]
    fn kind_as_str_names_every_variant() {
        let chars: NodeKind<PlainLang> = NodeKind::chars(Span::new(0, 1));
        let group: NodeKind<PlainLang> =
            NodeKind::group(GroupData::new(0u32, Span::new(0, 1), Span::new(1, 2)));
        let callable: NodeKind<PlainLang> = NodeKind::callable(CallableData {
            callable_type: 0u32,
            name: "m".into(),
            spec: Arc::new(StdCallableSpec::default()) as Arc<dyn CallableSpec<PlainLang>>,
            arguments: super::super::arguments::ParsedArguments::empty(),
            slots: super::super::arguments::ParsedSlots::new(vec![]),
            invocation_syntax: (),
        });
        let comment: NodeKind<PlainLang> =
            NodeKind::comment(Span::new(0, 1), Span::new(1, 2), Span::new(2, 3));
        let list: NodeKind<PlainLang> = NodeKind::list();
        assert_eq!(chars.as_str(), "Chars");
        assert_eq!(group.as_str(), "Group");
        assert_eq!(callable.as_str(), "Callable");
        assert_eq!(comment.as_str(), "Comment");
        assert_eq!(list.as_str(), "List");
    }
}
