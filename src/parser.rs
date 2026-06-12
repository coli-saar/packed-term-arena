use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::tree::{Tree, TreeArena};

lalrpop_util::lalrpop_mod!(tree_parser);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeParseError {
    message: String,
}

impl TreeParseError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl Display for TreeParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for TreeParseError {}

pub fn parse_tree(arena: &mut TreeArena<String>, input: &str) -> Result<Tree, TreeParseError> {
    tree_parser::TreeParser::new()
        .parse(arena, input)
        .map_err(|error| TreeParseError::new(error.to_string()))
}

pub(crate) fn decode_quoted_symbol(input: &str) -> String {
    let content = &input[1..input.len() - 1];
    let mut output = String::with_capacity(content.len());
    let mut chars = content.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let escaped = chars
            .next()
            .expect("quoted symbol regex prevents trailing backslashes");
        match escaped {
            '\\' => output.push('\\'),
            '\'' => output.push('\''),
            '"' => output.push('"'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            _ => unreachable!("quoted symbol regex accepts only supported escapes"),
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> (TreeArena<String>, Tree) {
        let mut arena = TreeArena::new();
        let tree = parse_tree(&mut arena, input).unwrap();
        (arena, tree)
    }

    #[test]
    fn parses_nested_tree() {
        let (arena, tree) = parse("f(a, g(c))");

        assert_eq!(tree.display(&arena).to_string(), "f(a, g(c))");
    }

    #[test]
    fn ignores_whitespace_between_tokens() {
        let (arena, tree) = parse("  f (  a ,\n\tg ( c ) )  ");

        assert_eq!(tree.display(&arena).to_string(), "f(a, g(c))");
    }

    #[test]
    fn parses_single_and_double_quoted_symbols() {
        let (arena, tree) = parse(r#"'root node'("left leaf", 'right leaf')"#);

        assert_eq!(
            tree.display(&arena).to_string(),
            "root node(left leaf, right leaf)"
        );
    }

    #[test]
    fn parses_quoted_punctuation_and_escapes() {
        let (arena, tree) = parse(r#""f(a, b)"('line\none', "tab\tquote\"slash\\")"#);
        let children = arena.get_children(tree);

        assert_eq!(arena.get_label(tree), "f(a, b)");
        assert_eq!(arena.get_label(children[0]), "line\none");
        assert_eq!(arena.get_label(children[1]), "tab\tquote\"slash\\");
    }

    #[test]
    fn allows_empty_quoted_symbols() {
        let (arena, tree) = parse(r#""""#);

        assert_eq!(arena.get_label(tree), "");
    }

    #[test]
    fn appends_to_existing_arena() {
        let mut arena = TreeArena::new();
        let existing = arena.add_node("existing".to_string(), vec![]);

        let parsed = parse_tree(&mut arena, "f(a)").unwrap();

        assert_eq!(existing.index(), 0);
        assert_eq!(parsed.index(), 2);
        assert_eq!(existing.display(&arena).to_string(), "existing");
        assert_eq!(parsed.display(&arena).to_string(), "f(a)");
    }

    #[test]
    fn rejects_invalid_inputs() {
        for input in ["f(a", "f(a b)", "'unterminated", "f(a) trailing", "f()"] {
            assert!(
                parse_tree(&mut TreeArena::new(), input).is_err(),
                "{input:?} should not parse"
            );
        }
    }
}
