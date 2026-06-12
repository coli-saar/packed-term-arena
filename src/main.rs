use rusty_tree::parser::parse_tree;
use rusty_tree::pcfg::{PcfgArena, parse_pcfg_file};
use rusty_tree::tree::TreeArena;

fn main() {
    println!("Hello, world!");

    let mut arena = TreeArena::<String>::new();
    let a = arena.add_node("a".to_string(), vec![]);
    let b = arena.add_node("b".to_string(), vec![]);
    let root = arena.add_node("f".to_string(), vec![a, b]);

    // println!("{}", arena.get_node(root).unwrap());
    println!("{}", root.display(&arena));

    let mut arena2 = TreeArena::<String>::new();
    let t2 = arena.map(root, |s| s.to_uppercase(), &mut arena2);
    println!("{}", t2.display(&arena2));

    let parsed = parse_tree(&mut arena, r#"f(a, "g(c)")"#).unwrap();
    println!("{}", parsed.display(&arena));

    let mut pcfg_arena = PcfgArena::new();
    let pcfg = parse_pcfg_file(&mut pcfg_arena, "examples/elephant.cfg").unwrap();
    println!("{}", pcfg_arena);
}
