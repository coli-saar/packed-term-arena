# packed-term-arena

[![Crates.io](https://img.shields.io/crates/v/packed-term-arena.svg?cacheSeconds=300)](https://crates.io/crates/packed-term-arena)
[![Documentation](https://img.shields.io/docsrs/packed-term-arena/latest?cacheSeconds=300)](https://docs.rs/packed-term-arena/latest/packed_term_arena/)

`packed-term-arena` stores labeled trees and shared term DAGs in an
append-oriented arena. Nodes have small copyable handles, and every node's
ordered children are stored together in one flat buffer. A checkpoint can be
used to discard a recently appended suffix without releasing vector capacity;
`clear()` does the same for the whole arena.

The crate is designed for symbolic terms, syntax trees, and other workloads
that build structures bottom-up and then traverse, copy, or transform them. It
does not provide arbitrary deletion or variable-arity reparenting, but callers
may replace handles inside an existing fixed-length child slice.

## Installation

```toml
[dependencies]
packed-term-arena = "0.1.3"
```

The Rust crate name is `packed_term_arena`.

## Quick start

```rust
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();

let left = arena.add_leaf("left");
let right = arena.add_leaf("right");
let root = arena.add_binary("root", left, right);

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

### Append-oriented, bottom-up construction

A `TreeArena<E>` owns every label and child list. A `Tree` is only an opaque
integer handle into that arena.

Children must already exist when their parent is added, so structures are
naturally built bottom-up:

```text
add leaves → add their parents → add the root
```

After insertion, a retained node's label and child-list length never change.
Individual child handles can be replaced through `get_children_mut` without
moving any packed ranges. New trees can still be added to the same arena, and
multiple independent roots may coexist there. Rewinding to a checkpoint
removes only nodes added after that checkpoint.

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

`get_children_mut` returns the corresponding fixed-length mutable slice when
an application needs to update edges while preserving stable node handles.

There is no per-node child vector, per-access allocation, or sibling-link
traversal. Existing handles remain valid while more nodes are added. Rewinding
invalidates handles in the removed suffix; an invalidated index may later be
reused by a newly appended node.

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

### Allocation-free node construction

When the children are already available as a slice, `add_node_from_slice`
copies them directly into the arena's packed child storage and avoids creating
a temporary `Vec<Tree>`. Convenience methods cover the most common fixed
arities without a temporary child collection:

```rust
use packed_term_arena::tree::TreeArena;

let mut arena = TreeArena::new();
let left = arena.add_leaf("left");
let right = arena.add_leaf("right");
let unary = arena.add_unary("unary", left);
let binary = arena.add_binary("binary", unary, right);

let children = [left, binary, right];
let root = arena.add_node_from_slice("root", &children);

assert_eq!(arena.get_children(root), &children);
```

These methods avoid a separate heap allocation for a caller-owned child
vector. The arena's internal packed buffers may still grow and reallocate as
nodes and edges are appended. The original `add_node(label, Vec<Tree>)`
remains useful when the caller already owns a dynamically constructed vector.

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
| Replace a child handle | `O(1)` |
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
