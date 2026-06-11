mod tree;

fn main() {
    println!("Hello, world!");

    let mut arena = tree::TreeArena::<String>::new();
    let a = arena.add_node("a".to_string(), vec![]);
    let b = arena.add_node("b".to_string(), vec![]);
    let root = arena.add_node("f".to_string(), vec![a, b]);
    let t = arena.get_node(root).unwrap();

    // println!("{}", arena.get_node(root).unwrap());
    println!("{}", t.display(&arena));


    let mut arena2 = tree::TreeArena::<String>::new();
    let root2 = arena.map_into(root, |s| s.to_uppercase(), &mut arena2);
    println!("{}", arena2.get_node(root2).unwrap().display(&arena2));
}
