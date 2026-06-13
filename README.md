# rusty-tree

An arena-based library for building, transforming, and traversing labeled trees.

## Core concepts

### `TreeArena<E>` and `Tree`

All nodes live inside a `TreeArena<E>`, where `E` is the label type.  A node is
referenced by a `Tree` handle — a cheap, copyable index.  The arena itself owns
all data; handles are meaningless outside the arena that created them.

The arena is **append-only**: once a node is inserted its index, label, and
children never change.  This means every `Tree` handle and every `&[Tree]`
child slice stays valid for the arena's entire lifetime, even as more nodes are
added.

Because children must exist before their parent (you need a `Tree` handle to
pass as a child, and handles are only minted by `add_node`), every tree is
naturally built bottom-up.

### Building trees

Use `add_node` directly, or the `tree!` macro for concise nested syntax:

```rust
use rusty_tree::tree::TreeArena;

let mut arena = TreeArena::new();

// bottom-up with add_node
let a    = arena.add_node("a", vec![]);
let b    = arena.add_node("b", vec![]);
let root = arena.add_node("f", vec![a, b]);

// same result with the macro
let mut arena2 = TreeArena::new();
let root2 = rusty_tree::tree!(arena2, ("f", "a", "b"));

assert_eq!(root.display(&arena).to_string(), "f(a, b)");
assert_eq!(root2.display(&arena2).to_string(), "f(a, b)");
```

The `tree!` macro accepts arbitrary Rust expressions as labels and supports
arbitrary nesting with an optional trailing comma:

```rust
let root = rusty_tree::tree!(arena2, (
    "root",
    ("left",  "ll", "lr"),
    ("right", "rl", "rr"),
));
```

### Accessing nodes

```rust
arena.get_label(node)       // &E
arena.get_children(node)    // &[Tree]  — zero-copy slice, no allocation
arena.len()                 // usize, total node count
arena.is_empty()            // bool
```

### Displaying trees

```rust
println!("{}", root.display(&arena));  // f(a, b)
```

Leaf nodes print as their label.  Internal nodes print as `label(c1, c2, ...)`.
The label type must implement `Display`.

### Parsing trees

```rust
use rusty_tree::parser::parse_tree;

let mut arena = rusty_tree::tree::TreeArena::new();
let root = parse_tree(&mut arena, r#"f(a, "g(c)")"#).unwrap();
```

The parser accepts the same `label(c1, c2, ...)` format that `display` produces.
Labels can be bare alphanumeric identifiers or single- or double-quoted strings
with standard backslash escapes (`\\`, `\'`, `\"`, `\n`, `\r`, `\t`).  Parsed
nodes are appended after any nodes already in the arena.

### Traversal

`post_order` returns a lazy iterator that visits each node after all its
descendants, left subtrees before right:

```rust
for node in arena.post_order(root) {
    println!("{}", arena.get_label(node));
}
```

The iterator uses an explicit stack pre-allocated to 16 entries and does not
collect the traversal upfront.

### Copying subtrees

**Cross-arena copy** — append a subtree from one arena into another:

```rust
let mut target = TreeArena::new();
let new_root = source.copy_into(root, &mut target);
```

**In-arena copy** — duplicate a subtree within the same arena:

```rust
let dup_root = arena.dup_subtree(root);
// original nodes unchanged; dup_root refers to fresh nodes appended at the end
```

Both operations clone labels and preserve child structure.  They only copy
nodes reachable from the given root.

### Transformations with `map` and `MutAlgebra`

`map` is a bottom-up fold (catamorphism) over a subtree.  It takes a label
mapping function and a mutable algebra, and returns the algebra's result for
the root:

```rust
// uppercase all labels into a new arena
let mut target = TreeArena::new();
let new_root = source.map(root, |label: &String| label.to_uppercase(), &mut target);
```

Implement `MutAlgebra<Op, F>` to produce any result type `F` from a tree:

```rust
use rusty_tree::tree::{TreeArena, MutAlgebra};

struct Depth;

impl MutAlgebra<(), usize> for Depth {
    fn apply(&mut self, _op: (), children: Vec<usize>) -> usize {
        1 + children.into_iter().max().unwrap_or(0)
    }
}

let mut arena = TreeArena::new();
let root = rusty_tree::tree!(arena, ("f", ("g", "a"), "b"));
let depth = arena.map(root, |_| (), &mut Depth);
assert_eq!(depth, 3);
```

`TreeArena<E>` itself implements `MutAlgebra<E, Tree>`, so it can serve as the
algebra for cross-arena copy-with-transform — which is exactly what `copy_into`
uses internally.

## PCFG support

The `pcfg` module provides `PcfgArena` for storing and loading probabilistic
context-free grammars.  Productions, nonterminals, and terminals use the same
arena-plus-opaque-handle pattern as trees.  Use `parse_pcfg` or
`parse_pcfg_file` to load a grammar from text; probabilities may be specified
explicitly or left implicit (in which case the remaining mass is shared equally
among implicit rules for the same left-hand side).

## Design notes

**Why separate `nodes` and `children` vecs?**  Storing all child lists in one
flat `Vec<Tree>` with per-node ranges means `get_children` returns a plain
`&[Tree]` with no allocation or indirection.  The slice is stable because the
vec is only ever appended to.

**Sharing**  The same `Tree` handle can appear as a child of multiple nodes,
making the arena a DAG rather than a strict tree.  `post_order` visits a shared
node once per reference (once per place it appears in a child list).  `map`
and `copy_into` follow the same structural semantics.  `dup_subtree` creates
independent copies of each node in the subtree, so shared structure is
unshared in the copy.

**Arena identity**  `Tree` handles carry no arena tag.  Using a handle in the
wrong arena panics if the index is out of range, or silently returns the wrong
node if it happens to be in range.  This is an intentional tradeoff: arena
tags would bloat every handle and make the zero-copy child slice impossible.
