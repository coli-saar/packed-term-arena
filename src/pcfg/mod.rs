use std::collections::HashMap;
use std::fmt::Display;
use std::ops::Range;

mod load;

pub use load::{PcfgLoadError, PcfgParseError, parse_pcfg, parse_pcfg_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nonterminal(usize);

impl Nonterminal {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Terminal(usize);

impl Terminal {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Production(usize);

impl Production {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    Nonterminal(Nonterminal),
    Terminal(Terminal),
}

#[derive(Debug)]
struct ProductionData {
    lhs: Nonterminal,
    rhs: Range<usize>,
    probability: f64,
}

#[derive(Debug)]
pub struct PcfgArena {
    nonterminals: Vec<String>,
    nonterminal_indices: HashMap<String, Nonterminal>,
    terminals: Vec<String>,
    terminal_indices: HashMap<String, Terminal>,
    productions: Vec<ProductionData>,
    rhs_symbols: Vec<Symbol>,
    productions_by_lhs: Vec<Vec<Production>>,
    start_symbol: Option<Nonterminal>,
}

impl PcfgArena {
    pub fn new() -> Self {
        Self {
            nonterminals: Vec::new(),
            nonterminal_indices: HashMap::new(),
            terminals: Vec::new(),
            terminal_indices: HashMap::new(),
            productions: Vec::new(),
            rhs_symbols: Vec::new(),
            productions_by_lhs: Vec::new(),
            start_symbol: None,
        }
    }

    pub fn intern_nonterminal(&mut self, name: impl Into<String>) -> Nonterminal {
        let name = name.into();

        if let Some(nonterminal) = self.nonterminal_indices.get(&name) {
            return *nonterminal;
        }

        let nonterminal = Nonterminal(self.nonterminals.len());
        self.nonterminals.push(name.clone());
        self.nonterminal_indices.insert(name, nonterminal);
        self.productions_by_lhs.push(Vec::new());
        nonterminal
    }

    pub fn intern_terminal(&mut self, value: impl Into<String>) -> Terminal {
        let value = value.into();

        if let Some(terminal) = self.terminal_indices.get(&value) {
            return *terminal;
        }

        let terminal = Terminal(self.terminals.len());
        self.terminals.push(value.clone());
        self.terminal_indices.insert(value, terminal);
        terminal
    }

    pub fn add_production(
        &mut self,
        lhs: Nonterminal,
        rhs: Vec<Symbol>,
        probability: f64,
    ) -> Production {
        self.get_nonterminal_name(lhs);

        let production = Production(self.productions.len());
        let rhs_start = self.rhs_symbols.len();
        self.rhs_symbols.extend_from_slice(&rhs);
        let rhs_end = self.rhs_symbols.len();

        self.productions.push(ProductionData {
            lhs,
            rhs: rhs_start..rhs_end,
            probability,
        });
        self.productions_by_lhs[lhs.index()].push(production);

        if self.start_symbol.is_none() {
            self.start_symbol = Some(lhs);
        }

        production
    }

    fn get_production(&self, production: Production) -> &ProductionData {
        self.productions.get(production.index()).unwrap()
    }

    pub fn get_nonterminal_name(&self, nonterminal: Nonterminal) -> &str {
        self.nonterminals.get(nonterminal.index()).unwrap()
    }

    pub fn get_terminal_value(&self, terminal: Terminal) -> &str {
        self.terminals.get(terminal.index()).unwrap()
    }

    pub fn get_symbol_name(&self, symbol: Symbol) -> &str {
        match symbol {
            Symbol::Nonterminal(nonterminal) => self.get_nonterminal_name(nonterminal),
            Symbol::Terminal(terminal) => self.get_terminal_value(terminal),
        }
    }

    pub fn get_lhs(&self, production: Production) -> Nonterminal {
        self.get_production(production).lhs
    }

    pub fn get_rhs(&self, production: Production) -> &[Symbol] {
        &self.rhs_symbols[self.get_production(production).rhs.clone()]
    }

    pub fn get_probability(&self, production: Production) -> f64 {
        self.get_production(production).probability
    }

    pub(crate) fn set_probability(&mut self, production: Production, probability: f64) {
        self.productions
            .get_mut(production.index())
            .unwrap()
            .probability = probability;
    }

    pub fn get_productions(&self, lhs: Nonterminal) -> &[Production] {
        &self.productions_by_lhs[lhs.index()]
    }

    pub fn get_start_symbol(&self) -> Option<Nonterminal> {
        self.start_symbol
    }
}

impl Display for PcfgArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for production in &self.productions {
            write!(f, "{} ->", self.get_nonterminal_name(production.lhs))?;

            for rhs_index in production.rhs.clone() {
                let rhs_symbol = self.rhs_symbols[rhs_index];
                match rhs_symbol {
                    Symbol::Nonterminal(nonterminal) => {
                        write!(f, " {}", self.get_nonterminal_name(nonterminal))?;
                    }
                    Symbol::Terminal(terminal) => {
                        write!(f, " '{}'", self.get_terminal_value(terminal))?;
                    }
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_arena_starts_empty() {
        let arena = PcfgArena::new();

        assert!(arena.nonterminals.is_empty());
        assert!(arena.terminals.is_empty());
        assert!(arena.productions.is_empty());
        assert!(arena.rhs_symbols.is_empty());
        assert_eq!(arena.get_start_symbol(), None);
    }

    #[test]
    fn interns_nonterminals_and_terminals() {
        let mut arena = PcfgArena::new();

        let first_s = arena.intern_nonterminal("S");
        let second_s = arena.intern_nonterminal("S");
        let dog = arena.intern_terminal("dog");
        let second_dog = arena.intern_terminal("dog");

        assert_eq!(first_s, second_s);
        assert_eq!(dog, second_dog);
        assert_eq!(first_s.index(), 0);
        assert_eq!(dog.index(), 0);
    }

    #[test]
    fn add_production_returns_stable_sequential_ids() {
        let mut arena = PcfgArena::new();
        let s = arena.intern_nonterminal("S");
        let np = arena.intern_nonterminal("NP");

        let first = arena.add_production(s, vec![Symbol::Nonterminal(np)], 0.7);
        let second = arena.add_production(s, vec![], 0.3);

        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
    }

    #[test]
    fn rhs_slices_remain_stable_after_more_productions_are_added() {
        let mut arena = PcfgArena::new();
        let s = arena.intern_nonterminal("S");
        let np = arena.intern_nonterminal("NP");
        let vp = arena.intern_nonterminal("VP");
        let dog = arena.intern_terminal("dog");

        let first = arena.add_production(
            s,
            vec![Symbol::Nonterminal(np), Symbol::Nonterminal(vp)],
            1.0,
        );
        let second = arena.add_production(np, vec![Symbol::Terminal(dog)], 0.5);

        assert_eq!(
            arena.get_rhs(first),
            &[Symbol::Nonterminal(np), Symbol::Nonterminal(vp)]
        );
        assert_eq!(arena.get_rhs(second), &[Symbol::Terminal(dog)]);
    }

    #[test]
    fn productions_can_be_queried_by_lhs() {
        let mut arena = PcfgArena::new();
        let s = arena.intern_nonterminal("S");
        let np = arena.intern_nonterminal("NP");

        let first = arena.add_production(s, vec![Symbol::Nonterminal(np)], 0.7);
        let second = arena.add_production(s, vec![], 0.3);

        assert_eq!(arena.get_productions(s), &[first, second]);
        assert!(arena.get_productions(np).is_empty());
    }

    #[test]
    fn first_lhs_becomes_start_symbol() {
        let mut arena = PcfgArena::new();
        let s = arena.intern_nonterminal("S");
        let np = arena.intern_nonterminal("NP");

        arena.add_production(s, vec![Symbol::Nonterminal(np)], 1.0);
        arena.add_production(np, vec![], 1.0);

        assert_eq!(arena.get_start_symbol(), Some(s));
    }
}
