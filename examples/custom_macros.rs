//! Example showing how to define and use custom macros.

use techy::{ArgumentSpec, ArgumentsSpec, LatexContextDb, LatexWalker, MacroSpec};

fn main() {
    println!("Custom Macro Definition Example");
    println!("════════════════════════════════\n");

    // Create a custom context with additional macros
    let mut context = LatexContextDb::default();

    // Define a custom macro: \highlight[color]{text}
    context.add_macro(MacroSpec::new(
        "highlight",
        ArgumentsSpec::new(vec![
            ArgumentSpec::Optional,  // [color]
            ArgumentSpec::Mandatory, // {text}
        ]),
    ));

    // Define another custom macro: \todo*[priority]{description}
    context.add_macro(MacroSpec::new(
        "todo",
        ArgumentsSpec::new(vec![
            ArgumentSpec::Star,      // *
            ArgumentSpec::Optional,  // [priority]
            ArgumentSpec::Mandatory, // {description}
        ]),
    ));

    // Or use the simpler syntax
    context.add_macro(MacroSpec::simple("note", "[{"));

    let source = r#"
Here is some \highlight[yellow]{important text}.

\note[Important]{This is a note.}

\todo*[high]{Fix this bug}

Regular \textbf{bold} text still works!
"#;

    let walker = LatexWalker::with_context(source.trim().to_string(), context);

    match walker.parse() {
        Ok(ast) => {
            println!("✓ Parsed successfully!\n");
            println!("Found macros:");
            println!("─────────────");

            // Collect all macros
            collect_macros(&ast.nodes, 0);
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
        }
    }
}

fn collect_macros(nodes: &[techy::Node], indent: usize) {
    use techy::Node;

    for node in nodes {
        match node {
            Node::Macro(m) => {
                let prefix = "  ".repeat(indent);
                println!("{}\\{}", prefix, m.name);

                if m.spec.is_some() {
                    println!("{}  (defined in context)", prefix);
                }
            }
            Node::Group(g) => {
                collect_macros(&g.nodelist.nodes, indent + 1);
            }
            Node::Environment(e) => {
                collect_macros(&e.body.nodes, indent + 1);
            }
            _ => {}
        }
    }
}
