//! AST node types for representing parsed LaTeX.

use crate::source::Span;
use crate::spec::{EnvironmentSpec, MacroSpec, SpecialsSpec};
use std::sync::Arc;

/// Base trait for all LaTeX nodes.
pub trait LatexNodeTrait {
    /// Get the span this node covers in the source.
    fn span(&self) -> Span;

    /// Get the LaTeX source text for this node.
    fn latex_verbatim<'a>(&self, source: &'a str) -> &'a str {
        self.span().text(source)
    }
}

/// A LaTeX node in the AST.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Plain text characters.
    Chars(CharsNode),
    /// A braced group `{...}`.
    Group(GroupNode),
    /// A macro/command `\cmd`.
    Macro(MacroNode),
    /// An environment `\begin{env}...\end{env}`.
    Environment(EnvironmentNode),
    /// A comment `% ...`.
    Comment(CommentNode),
    /// A math block `$...$` or `$$...$$`.
    Math(MathNode),
    /// Special characters like `&`, `~`.
    Specials(SpecialsNode),
}

impl LatexNodeTrait for Node {
    fn span(&self) -> Span {
        match self {
            Node::Chars(n) => n.span,
            Node::Group(n) => n.span,
            Node::Macro(n) => n.span,
            Node::Environment(n) => n.span,
            Node::Comment(n) => n.span,
            Node::Math(n) => n.span,
            Node::Specials(n) => n.span,
        }
    }
}

/// A list of nodes with span information.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeList {
    /// The nodes in this list.
    pub nodes: Vec<Node>,
    /// The span covering all nodes.
    pub span: Span,
}

impl NodeList {
    /// Create a new empty node list.
    pub fn new(span: Span) -> Self {
        Self {
            nodes: Vec::new(),
            span,
        }
    }

    /// Create a node list with nodes.
    pub fn with_nodes(nodes: Vec<Node>, span: Span) -> Self {
        Self { nodes, span }
    }

    /// Get the number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Add a node to the list.
    pub fn push(&mut self, node: Node) {
        self.nodes.push(node);
    }
}

/// Plain text characters node.
#[derive(Debug, Clone, PartialEq)]
pub struct CharsNode {
    pub span: Span,
    pub chars: String,
}

/// A braced group `{...}`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupNode {
    pub span: Span,
    pub nodelist: NodeList,
}

/// A macro/command node.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroNode {
    pub span: Span,
    /// The macro name (without backslash).
    pub name: String,
    /// The specification for this macro, if known.
    pub spec: Option<Arc<MacroSpec>>,
    /// Parsed arguments.
    pub args: Arguments,
    /// Whitespace after the macro.
    pub post_space: String,
}

/// An environment node.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentNode {
    pub span: Span,
    /// The environment name.
    pub name: String,
    /// The specification for this environment, if known.
    pub spec: Option<Arc<EnvironmentSpec>>,
    /// Arguments to \begin{env}.
    pub args: Arguments,
    /// The body content between \begin and \end.
    pub body: NodeList,
}

/// A comment node.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentNode {
    pub span: Span,
    /// The comment text (without the % and newline).
    pub comment: String,
    /// Whitespace after the comment.
    pub post_space: String,
}

/// A math mode node.
#[derive(Debug, Clone, PartialEq)]
pub struct MathNode {
    pub span: Span,
    /// Display type: `inline` or `display`.
    pub display_type: MathDisplayType,
    /// The content inside math mode.
    pub nodelist: NodeList,
}

/// Math display type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathDisplayType {
    /// Inline math: `$...$` or `\(...\)`.
    Inline,
    /// Display math: `$$...$$` or `\[...\]`.
    Display,
}

/// A specials node (special characters).
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialsNode {
    pub span: Span,
    /// The special characters.
    pub chars: String,
    /// The specification for this special, if known.
    pub spec: Option<Arc<SpecialsSpec>>,
    /// Parsed arguments (if the special takes arguments).
    pub args: Option<Arguments>,
}

/// Parsed arguments for a macro, environment, or special.
#[derive(Debug, Clone, PartialEq)]
pub struct Arguments {
    /// Named arguments with their values.
    pub args: Vec<(String, ArgumentValue)>,
    /// Span covering all arguments.
    pub span: Span,
}

impl Arguments {
    /// Create empty parsed arguments.
    pub fn empty(span: Span) -> Self {
        Self {
            args: Vec::new(),
            span,
        }
    }

    /// Get an argument by name.
    pub fn get(&self, name: &str) -> Option<&ArgumentValue> {
        self.args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Get the number of arguments.
    pub fn len(&self) -> usize {
        self.args.len()
    }

    /// Check if there are no arguments.
    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }
}

/// Value of an argument.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentValue {
    /// A single node.
    Node(Box<Node>),
    /// A list of nodes.
    NodeList(NodeList),
    /// Verbatim text (for verbatim arguments).
    Verbatim(String),
    /// No value (for optional arguments that weren't provided).
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_list() {
        let mut list = NodeList::new(Span::new(0, 10));
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        list.push(Node::Chars(CharsNode {
            span: Span::new(0, 5),
            chars: "hello".to_string(),
        }));

        assert!(!list.is_empty());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_parsed_arguments() {
        let args = Arguments::empty(Span::new(0, 0));
        assert!(args.is_empty());
        assert_eq!(args.len(), 0);
    }
}
