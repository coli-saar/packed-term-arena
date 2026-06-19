use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

use super::{Nonterminal, PcfgArena, Production};

lalrpop_util::lalrpop_mod!(parser, "/pcfg/parser.rs");

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParsedProduction {
    pub(crate) production: Production,
    pub(crate) explicit_probability: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcfgParseError {
    message: String,
}

impl PcfgParseError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl Display for PcfgParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for PcfgParseError {}

#[derive(Debug)]
pub enum PcfgLoadError {
    Io(std::io::Error),
    Parse(PcfgParseError),
}

impl Display for PcfgLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PcfgLoadError::Io(error) => write!(f, "failed to read PCFG file: {error}"),
            PcfgLoadError::Parse(error) => write!(f, "failed to parse PCFG file: {error}"),
        }
    }
}

impl Error for PcfgLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PcfgLoadError::Io(error) => Some(error),
            PcfgLoadError::Parse(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for PcfgLoadError {
    fn from(error: std::io::Error) -> Self {
        PcfgLoadError::Io(error)
    }
}

pub fn parse_pcfg(arena: &mut PcfgArena, input: &str) -> Result<Vec<Production>, PcfgParseError> {
    let parsed = parser::GrammarParser::new()
        .parse(arena, input)
        .map_err(|error| PcfgParseError::new(error.to_string()))?;

    fill_implicit_probabilities(arena, &parsed);

    Ok(parsed
        .into_iter()
        .map(|parsed_production| parsed_production.production)
        .collect())
}

pub fn parse_pcfg_file(
    arena: &mut PcfgArena,
    path: impl AsRef<Path>,
) -> Result<Vec<Production>, PcfgLoadError> {
    let input = std::fs::read_to_string(path)?;
    parse_pcfg(arena, &input).map_err(PcfgLoadError::Parse)
}

fn fill_implicit_probabilities(arena: &mut PcfgArena, parsed: &[ParsedProduction]) {
    let mut implicit_by_lhs: HashMap<Nonterminal, Vec<Production>> = HashMap::new();

    for parsed_production in parsed {
        if parsed_production.explicit_probability.is_none() {
            let lhs = arena.get_lhs(parsed_production.production);
            implicit_by_lhs
                .entry(lhs)
                .or_default()
                .push(parsed_production.production);
        }
    }

    for (lhs, implicit_productions) in implicit_by_lhs {
        let implicit_set: HashSet<Production> = implicit_productions.iter().copied().collect();
        let explicit_mass: f64 = arena
            .get_productions(lhs)
            .iter()
            .filter(|production| !implicit_set.contains(production))
            .map(|production| arena.get_probability(*production))
            .sum();
        let probability = (1.0 - explicit_mass) / implicit_productions.len() as f64;

        for production in implicit_productions {
            arena.set_probability(production, probability);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcfg::{Symbol, Terminal};

    fn parse(input: &str) -> (PcfgArena, Vec<Production>) {
        let mut arena = PcfgArena::new();
        let productions = parse_pcfg(&mut arena, input).unwrap();
        (arena, productions)
    }

    #[test]
    fn parses_multiple_newline_delimited_rules() {
        let (arena, productions) =
            parse("S -> NP VP [1.0]\nNP -> 'dog' [0.4]\nNP -> 'cat' [0.6]\nVP -> [0.1]\n");

        assert_eq!(productions.len(), 4);
        assert_eq!(
            arena.get_nonterminal_name(arena.get_lhs(productions[0])),
            "S"
        );
        assert_eq!(arena.get_probability(productions[1]), 0.4);
        assert!(arena.get_rhs(productions[3]).is_empty());
    }

    #[test]
    fn parses_quoted_terminals_and_escapes() {
        let (arena, productions) = parse(r#"NP -> 'line\none' "tab\tquote\"slash\\" [1.0]"#);

        assert_eq!(
            arena.get_rhs(productions[0]),
            &[Symbol::Terminal(Terminal(0)), Symbol::Terminal(Terminal(1))]
        );
        assert_eq!(arena.get_terminal_value(Terminal(0)), "line\none");
        assert_eq!(arena.get_terminal_value(Terminal(1)), "tab\tquote\"slash\\");
    }

    #[test]
    fn parses_scientific_notation_probabilities() {
        let (arena, productions) = parse("S -> NP [1e-3]\nNP -> [2.5E+1]");

        assert_eq!(arena.get_probability(productions[0]), 1e-3);
        assert_eq!(arena.get_probability(productions[1]), 2.5E+1);
    }

    #[test]
    fn splits_probability_evenly_when_all_rules_for_lhs_are_implicit() {
        let (arena, productions) = parse("NP -> 'dog'\nNP -> 'cat'");

        assert_eq!(arena.get_probability(productions[0]), 0.5);
        assert_eq!(arena.get_probability(productions[1]), 0.5);
    }

    #[test]
    fn splits_remaining_probability_across_implicit_rules_for_lhs() {
        let (arena, productions) = parse("NP -> 'dog' [0.4]\nNP -> 'cat'\nNP -> 'mouse'");

        assert_eq!(arena.get_probability(productions[0]), 0.4);
        assert_eq!(arena.get_probability(productions[1]), 0.3);
        assert_eq!(arena.get_probability(productions[2]), 0.3);
    }

    #[test]
    fn splits_implicit_probability_separately_per_lhs() {
        let (arena, productions) = parse("S -> NP\nNP -> 'dog' [0.25]\nNP -> 'cat'");

        assert_eq!(arena.get_probability(productions[0]), 1.0);
        assert_eq!(arena.get_probability(productions[1]), 0.25);
        assert_eq!(arena.get_probability(productions[2]), 0.75);
    }

    #[test]
    fn implicit_append_accounts_for_existing_productions() {
        let mut arena = PcfgArena::new();
        let np = arena.intern_nonterminal("NP");
        let dog = arena.intern_terminal("dog");
        arena.add_production(np, vec![Symbol::Terminal(dog)], 0.25);

        let parsed = parse_pcfg(&mut arena, "NP -> 'cat'\nNP -> 'mouse'").unwrap();

        assert_eq!(arena.get_probability(parsed[0]), 0.375);
        assert_eq!(arena.get_probability(parsed[1]), 0.375);
    }

    #[test]
    fn appends_to_existing_arena() {
        let mut arena = PcfgArena::new();
        let existing_lhs = arena.intern_nonterminal("Existing");
        let existing = arena.add_production(existing_lhs, vec![], 0.25);

        let parsed = parse_pcfg(&mut arena, "S -> 'dog' [0.75]").unwrap();

        assert_eq!(existing.index(), 0);
        assert_eq!(parsed, vec![Production(1)]);
        assert_eq!(arena.get_start_symbol(), Some(existing_lhs));
        assert_eq!(arena.get_probability(parsed[0]), 0.75);
    }

    #[test]
    fn parses_pcfg_from_file() {
        let path =
            std::env::temp_dir().join(format!("packed_term_pcfg_test_{}.pcfg", std::process::id()));
        std::fs::write(&path, "NP -> 'dog'\nNP -> 'cat'").unwrap();

        let mut arena = PcfgArena::new();
        let parsed = parse_pcfg_file(&mut arena, &path).unwrap();

        std::fs::remove_file(&path).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(arena.get_probability(parsed[0]), 0.5);
        assert_eq!(arena.get_probability(parsed[1]), 0.5);
    }

    #[test]
    fn rejects_invalid_inputs() {
        for input in [
            "'S' -> NP [1.0]",
            "S NP [1.0]",
            "S -> 'unterminated [1.0]",
            "S -> NP [1.0];",
            "S -> NP [1.0] trailing",
        ] {
            assert!(
                parse_pcfg(&mut PcfgArena::new(), input).is_err(),
                "{input:?} should not parse"
            );
        }
    }
}
