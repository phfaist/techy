//! Basic usage example for techy.

use techy::{LatexWalker, Node};

fn main() {
    // Parse a simple LaTeX document
    let source = r#"
\section{Introduction}

This is a \textbf{bold} statement with some \emph{emphasis}.

Here's a group: {content}.

% This is a comment
And some more text.
"#;

    println!("Parsing LaTeX source...\n");

    let walker = LatexWalker::new(source.trim().to_string());
    
    match walker.parse() {
        Ok(ast) => {
            println!("✓ Successfully parsed {} nodes\n", ast.nodes.len());
            
            // Traverse and display the AST
            println!("AST Structure:");
            println!("═════════════");
            
            for (i, node) in ast.nodes.iter().enumerate() {
                print_node(node, i, 0);
            }
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
            eprintln!("\n{}", e.format_with_source(walker.source()));
        }
    }
}

fn print_node(node: &Node, index: usize, indent: usize) {
    let prefix = "  ".repeat(indent);
    
    match node {
        Node::Chars(n) => {
            println!("{}[{}] Chars: {:?}", prefix, index, n.chars);
        }
        Node::Macro(n) => {
            println!("{}[{}] Macro: \\{}", prefix, index, n.name);
            if !n.args.is_empty() {
                println!("{}    args: {} argument(s)", prefix, n.args.len());
            }
        }
        Node::Group(n) => {
            println!("{}[{}] Group: {{ ... }} ({} nodes)", prefix, index, n.nodelist.len());
            for (i, child) in n.nodelist.nodes.iter().enumerate() {
                print_node(child, i, indent + 1);
            }
        }
        Node::Environment(n) => {
            println!("{}[{}] Environment: {}", prefix, index, n.name);
            println!("{}    body: {} nodes", prefix, n.body.len());
        }
        Node::Comment(n) => {
            println!("{}[{}] Comment: %{}", prefix, index, n.comment);
        }
        Node::Math(n) => {
            println!("{}[{}] Math: {:?} ({} nodes)", prefix, index, n.display_type, n.nodelist.len());
        }
        Node::Specials(n) => {
            println!("{}[{}] Specials: {}", prefix, index, n.chars);
        }
    }
}
