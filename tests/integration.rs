//! Integration tests for techy.

use techy::{Parser, Node};

#[test]
fn test_parse_empty() {
    let walker = Parser::new("".to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    let ast = result.unwrap();
    assert!(ast.is_empty());
}

#[test]
fn test_parse_plain_text() {
    let walker = Parser::new("Hello world".to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    assert_eq!(ast.len(), 1);
    
    match &ast.nodes[0] {
        Node::Chars(n) => assert_eq!(n.chars, "Hello"),
        _ => panic!("Expected Chars node"),
    }
}

#[test]
fn test_parse_macro() {
    let walker = Parser::new(r"\textbf".to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    match &ast.nodes[0] {
        Node::Macro(n) => {
            assert_eq!(n.name, "textbf");
            assert!(n.spec.is_some()); // Should find it in default context
        }
        _ => panic!("Expected Macro node"),
    }
}

#[test]
fn test_parse_group() {
    let walker = Parser::new("{hello}".to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    match &ast.nodes[0] {
        Node::Group(g) => {
            assert_eq!(g.nodelist.len(), 1);
        }
        _ => panic!("Expected Group node"),
    }
}

#[test]
fn test_parse_comment() {
    let walker = Parser::new("% comment\n".to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    match &ast.nodes[0] {
        Node::Comment(c) => {
            assert_eq!(c.comment.trim(), "comment");
        }
        _ => panic!("Expected Comment node"),
    }
}

#[test]
fn test_parse_mixed_content() {
    let source = r#"
Hello \textbf{world}!
% A comment
{grouped text}
"#;
    
    let walker = Parser::new(source.trim().to_string());
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    assert!(ast.len() > 0);
}

#[test]
fn test_parse_unmatched_brace() {
    let walker = Parser::new("{unclosed".to_string());
    let result = walker.parse();
    assert!(result.is_err());
}

#[test]
fn test_custom_macro() {
    use techy::{ContextDb, MacroSpec};
    
    let mut context = ContextDb::new();
    context.add_macro(MacroSpec::simple("mycmd", "{"));
    
    let walker = Parser::with_context(r"\mycmd".to_string(), context);
    let result = walker.parse();
    assert!(result.is_ok());
    
    let ast = result.unwrap();
    match &ast.nodes[0] {
        Node::Macro(n) => {
            assert_eq!(n.name, "mycmd");
            assert!(n.spec.is_some());
        }
        _ => panic!("Expected Macro node"),
    }
}
