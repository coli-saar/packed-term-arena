# packed-term-arena

[![Crates.io](https://img.shields.io/crates/v/packed-term-arena.svg?cacheSeconds=300)](https://crates.io/crates/packed-term-arena)
[![Documentation](https://img.shields.io/docsrs/packed-term-arena/latest?cacheSeconds=300)](https://docs.rs/packed-term-arena/latest/packed_term_arena/)

`packed-term-arena` stores labeled trees and shared term DAGs in an append-only
arena. Nodes have small copyable handles, and every node's ordered children are
stored together in one flat buffer.

The crate is designed for symbolic terms, syntax trees, grammar tooling, and
other workloads that build structures bottom-up and then traverse, copy, or
transform them. It deliberately does not provide deletion, reparenting, or
in-place topology mutation.

## Installation

```toml
[dependencies]
packed-term-arena = "0.1"
```

The Rust crate name is `packed_term_arena`.

## Quick start

```rust
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();

let left = arena.add_node("left", vec![]);
let right = arena.add_node("right", vec![]);
let root = arena.add_node("root", vec![left, right]);

assert_eq!(arena.get_label(root), &"root");
assert_eq!(arena.get_children(root), &[left, right]);
assert_eq!(root.display(&arena).to_string(), "root(left, right)");
```

The `tree!` macro provides nested construction syntax:

```rust
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();
let root = packed_term_arena::tree!(
    arena,
    ("root", ("left", "a", "b"), ("right", "c"))
);

assert_eq!(
    root.display(&arena).to_string(),
    "root(left(a, b), right(c))"
);
```

## Design

### Append-only, bottom-up construction

A `TreeArena<E>` owns every label and child list. A `Tree` is only an opaque
integer handle into that arena.

Children must already exist when their parent is added, so structures are
naturally built bottom-up:

```text
add leaves → add their parents → add the root
```

After insertion, a node's label and children never move or change. New trees
can still be added to the same arena, and multiple independent roots may
coexist there.

This restricted model keeps the representation small and predictable. If an
application needs frequent deletion, reparenting, or parent/sibling navigation,
a mutable hierarchy crate such as `indextree` is a better fit.

### Packed child storage

“Packed” means that the tree topology is stored in two flat, growable arrays,
not as separately allocated node objects connected by pointers. For example,
`root(left(a, b), right(c))` is inserted bottom-up and receives these handles:

```text
Logical structure                 Packed arena memory

          root (T5)               nodes: Vec<Node<E>>
         /         \              ┌──────┬───────┬───────────┐
   left (T2)     right (T4)        │ index│ label │ children  │
    /    \           │             ├──────┼───────┼───────────┤
 a (T0) b (T1)     c (T3)          │ T0   │ a     │ 0..0      │
                                    │ T1   │ b     │ 0..0      │
                                    │ T2   │ left  │ 0..2      │
                                    │ T3   │ c     │ 2..2      │
                                    │ T4   │ right │ 2..3      │
                                    │ T5   │ root  │ 3..5      │
                                    └──────┴───────┴───────────┘

                                    children: Vec<Tree>
                                    index     0    1    2    3    4
                                    value   [ T0 | T1 | T3 | T2 | T4 ]
                                              └ left ┘  │    └ root ┘
                                                      right
```

A `Tree` is only a `usize` index into `nodes`. Each node descriptor contains
its label and a `Range<usize>` selecting one contiguous run in `children`.
Leaves use an empty range and require no child allocation.

This layout keeps node metadata and edge handles densely packed. Sequential
node processing walks adjacent descriptors, while iterating a node's children
walks adjacent, pointer-sized handles. Compared with a pointer-rich tree, this
usually means fewer allocations and indirections and gives the CPU cache and
hardware prefetcher a much simpler access pattern. The label type `E` can
still own heap data; it is specifically the arena's topology that is packed.

Consequently, `get_children` returns an ordinary contiguous `&[Tree]`:

```rust
let children: &[packed_term_arena::tree::Tree] = arena.get_children(root);
```

There is no per-node child vector, per-access allocation, or sibling-link
traversal. Because the arena is append-only, existing handles and child slices
remain valid while more nodes are added.

### Trees and shared DAGs

The same `Tree` handle may appear in more than one child list. This permits
structural sharing:

```rust
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();
let shared = arena.add_node("x", vec![]);
let left = arena.add_node("left", vec![shared]);
let right = arena.add_node("right", vec![shared]);
let root = arena.add_node("root", vec![left, right]);

assert_eq!(arena.get_children(left), &[shared]);
assert_eq!(arena.get_children(right), &[shared]);
```

The result is a DAG rather than a strict tree. Structural operations follow
child edges, so a shared node is normally visited once for each occurrence.

## Features

### Lazy post-order traversal

`post_order` visits children before their parent and preserves left-to-right
child order:

```rust
for node in arena.post_order(root) {
    println!("{}", arena.get_label(node));
}
```

The iterator uses an explicit stack and does not collect the full traversal
before yielding nodes.

### Bottom-up folds and transformations

`map` is a bottom-up fold over a term. A label-mapping function produces an
operation for each node, and a `MutAlgebra` combines that operation with the
already-computed child results.

```rust
use packed_term_arena::tree::{MutAlgebra, TreeArena};

struct Depth;

impl MutAlgebra<(), usize> for Depth {
    fn apply(&mut self, _label: (), children: Vec<usize>) -> usize {
        1 + children.into_iter().max().unwrap_or(0)
    }
}

let mut arena = TreeArena::new();
let root = packed_term_arena::tree!(arena, ("f", ("g", "a"), "b"));

assert_eq!(arena.map(root, |_| (), &mut Depth), 3);
```

A `TreeArena<E>` is itself an algebra, so the same mechanism can rebuild a term
in another arena while changing its labels:

```rust
let mut target = TreeArena::new();
let mapped = arena.map(root, |label| label.to_uppercase(), &mut target);

assert_eq!(mapped.display(&target).to_string(), "F(G(A), B)");
```

### Copying with explicit sharing semantics

Two copying operations serve different purposes:

- `copy_into` copies a rooted structure into another arena. It follows every
  structural occurrence, so shared nodes are unfolded.
- `dup_subtree` duplicates a rooted structure in the same arena. Each distinct
  source node is copied once, preserving sharing in the duplicate.

Both operations append new nodes and leave existing nodes untouched.

### Parsing and display

`parse_tree` reads a compact term notation:

```rust
use packed_term_arena::parser::parse_tree;
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();
let root = parse_tree(&mut arena, r#"f(a, "label with spaces")"#)?;

assert_eq!(arena.get_label(root), "f");
# Ok::<(), packed_term_arena::parser::TreeParseError>(())
```

Bare labels may contain letters, digits, `_`, and `-`. Single- and
double-quoted labels support common backslash escapes. Parsed nodes are
appended to the supplied arena.

`Tree::display` renders labels using their `Display` implementation and formats
children as `label(child1, child2, ...)`.


## Complexity and tradeoffs

| Operation | Cost |
|---|---:|
| Add a node | `O(number of children)` |
| Access a label | `O(1)` |
| Access a child slice | `O(1)` |
| Traverse or fold a rooted structure | `O(structural occurrences)` |
| Duplicate while preserving sharing | `O(distinct reachable nodes + edges)` |

The compact `Tree` handle does not contain an arena identity. Passing a handle
to the wrong arena can panic or, if the index exists there, address the wrong
node. This is an intentional space and layout tradeoff; callers must keep
handles associated with their originating arena.

The arena also does not track parent links or roots. Those omissions make
shared children and cheap contiguous child access possible, but mean ancestor
queries require application-maintained data.

## Minimum supported Rust version

The minimum supported Rust version is 1.85.

## License

Licensed under the [MIT License](LICENSE).
