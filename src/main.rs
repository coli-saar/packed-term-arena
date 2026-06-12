mod tree;

fn main() {
    println!("Hello, world!");

    let mut arena = tree::TreeArena::<String>::new();
    let a = arena.add_node("a".to_string(), vec![]);
    let b = arena.add_node("b".to_string(), vec![]);
    let root = arena.add_node("f".to_string(), vec![a, b]);

    // println!("{}", arena.get_node(root).unwrap());
    println!("{}", root.display(&arena));


    let mut arena2 = tree::TreeArena::<String>::new();
    let t2 = arena.map(root, |s| s.to_uppercase(), &mut arena2);
    println!("{}", t2.display(&arena2));
}
