use std::fmt::Display;
use std::ops::Range;

/// A lightweight handle to a node inside a [`TreeArena`].
///
/// `Tree` is an opaque index — it is cheap to copy, compare, and hash, but
/// carries no data on its own.  All information (label, children) is stored in
/// the arena that created the handle.
///
/// Because `Tree` handles are created only by [`TreeArena::add_node`], a handle
/// can only refer to a node that has already been inserted.  Passing a handle
/// to the wrong arena is not prevented at compile time; doing so will either
/// panic (if the index is out of range) or silently return the wrong node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tree(usize);

impl Tree {
    /// Return the zero-based index of this node within its arena.
    ///
    /// Indices are assigned in insertion order and never change.  They can be
    /// used to size side tables and index into them in O(1).
    pub fn index(self) -> usize {
        self.0
    }
}

/// Internal storage for a single node.  Not exposed publicly.
#[derive(Debug)]
struct Node<E> {
    /// Slice of [`TreeArena::children`] that lists this node's children.
    pub children: Range<usize>,
    pub label: E,
}

/// An arena that owns a forest of labeled trees.
///
/// # Layout
///
/// Nodes are stored in a flat `Vec` in insertion order.  Children are stored
/// in a separate flat `Vec` of [`Tree`] handles, with each node keeping a
/// [`Range`] into that slice.  This means:
///
/// - `get_children` is a zero-copy slice reference — no allocation.
/// - Both vecs are append-only, so every [`Tree`] handle and every `&[Tree]`
///   child slice stays valid for the lifetime of the arena, regardless of
///   how many more nodes are added later.
///
/// # Node identity and sharing
///
/// A [`Tree`] handle is just an index.  The same handle can appear as a child
/// of multiple nodes, making the arena a DAG rather than a strict tree if you
/// choose to do so.  Methods like [`map`](Self::map) and [`post_order`](Self::post_order)
/// follow structural edges and will therefore visit a shared node once per
/// reference.
///
/// # Multiple trees in one arena
///
/// Nothing prevents storing several independent trees in the same arena.
/// Handles from one logical tree can coexist with handles from another; the
/// arena doesn't track roots.
#[derive(Debug)]
pub struct TreeArena<E> {
    nodes: Vec<Node<E>>,
    /// Flat storage of all child lists, sliced by each node's `children` range.
    children: Vec<Tree>,
}

impl<E> TreeArena<E> {
    /// Create an empty arena.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Insert a new node with the given label and ordered child list.
    ///
    /// Returns a [`Tree`] handle whose index equals the number of nodes that
    /// existed before this call.  Children must already live in this arena;
    /// passing handles from a different arena will produce incorrect results.
    ///
    /// The children are appended to the shared child list in the order given,
    /// preserving left-to-right semantics.
    pub fn add_node(&mut self, label: E, children: Vec<Tree>) -> Tree {
        let index = self.nodes.len();

        let children_start = self.children.len();
        self.children.extend_from_slice(&children);
        let children_end = self.children.len();

        self.nodes.push(Node {
            children: children_start..children_end,
            label,
        });

        Tree(index)
    }

    /// Return a reference to the label of `tree`.
    ///
    /// Panics if `tree` was not created by this arena.
    pub fn get_label(&self, tree: Tree) -> &E {
        &self.get_node(tree).label
    }

    /// Return the ordered child list of `tree` as a slice of handles.
    ///
    /// The slice is a zero-copy view into the arena's internal child storage.
    /// It remains valid as long as the arena is alive, even if more nodes are
    /// added later.
    ///
    /// Panics if `tree` was not created by this arena.
    pub fn get_children(&self, tree: Tree) -> &[Tree] {
        &self.children[self.get_node(tree).children.clone()]
    }

    /// Return the total number of nodes in this arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Return `true` if the arena contains no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterate over the subtree rooted at `root` in post-order (children
    /// before their parent, left subtrees before right subtrees).
    ///
    /// The iterator is lazy: it uses an explicit stack and emits one node per
    /// call to `next`, without collecting the traversal upfront.  The stack is
    /// pre-allocated to 16 entries to avoid reallocations for typical tree
    /// depths.
    ///
    /// **Sharing**: if the same [`Tree`] handle appears as a child of multiple
    /// nodes, it is visited once per occurrence in the child list — exactly as
    /// many times as the structural traversal would reach it.
    ///
    /// Panics if `root` or any reachable descendant was not created by this
    /// arena.
    pub fn post_order(&self, root: Tree) -> impl Iterator<Item = Tree> + '_ {
        let mut stack: Vec<(Tree, bool)> = Vec::with_capacity(16);
        stack.push((root, false));
        std::iter::from_fn(move || loop {
            let &(node, expanded) = stack.last()?;
            if expanded {
                stack.pop();
                return Some(node);
            }
            // Mark this node as expanded and push its children right-to-left
            // so the leftmost child is on top of the stack.
            stack.last_mut().unwrap().1 = true;
            for &child in self.get_children(node).iter().rev() {
                stack.push((child, false));
            }
        })
    }

    /// Copy the subtree rooted at `root` into `target`, returning the new root.
    ///
    /// New nodes are appended to `target` after any nodes already there.
    /// Labels are cloned; child structure is followed edge by edge.
    ///
    /// **Sharing is unfolded**: if the same `Tree` handle appears as a child of
    /// multiple nodes, each structural reference is copied independently.  The
    /// copy may therefore contain more nodes than there are distinct nodes in
    /// the source subtree.  Use [`dup_subtree`](Self::dup_subtree) when the
    /// copy must preserve sharing.
    ///
    /// This is equivalent to calling [`map`](Self::map) with an identity label
    /// transform and `target` as the algebra.
    pub fn copy_into(&self, root: Tree, target: &mut TreeArena<E>) -> Tree
    where
        E: Clone,
    {
        self.map(root, |label| label.clone(), target)
    }

    /// Duplicate the subtree rooted at `root` within this arena.
    ///
    /// Fresh nodes for every *distinct* node in the subtree are appended to
    /// the arena, and the root of the copy is returned.  The original nodes
    /// are unmodified.  Labels are cloned.
    ///
    /// **Sharing is preserved**: if the same `Tree` handle appears as a child
    /// of multiple nodes in the subtree, it maps to a single new node in the
    /// copy, and all child slots that referenced the original point to that
    /// same new node.  As a result, `arena.len()` grows by exactly the number
    /// of *distinct* nodes reachable from `root`.  This is in contrast to
    /// [`copy_into`](Self::copy_into), which follows every structural edge
    /// independently and unfolds sharing into separate copies.
    ///
    /// Because Rust does not allow simultaneous mutable and immutable borrows
    /// of the same arena, the implementation uses two phases: a read pass to
    /// collect the traversal order (which releases the immutable borrow), then
    /// a write pass that rebuilds nodes one by one.  This is safe because the
    /// arena is append-only — existing [`Tree`] handles and child slices remain
    /// valid while new nodes are pushed.
    pub fn dup_subtree(&mut self, root: Tree) -> Tree
    where
        E: Clone,
    {
        let order: Vec<Tree> = self.post_order(root).collect();
        let mut remap = std::collections::HashMap::with_capacity(order.len());
        for node in order {
            // Skip nodes already mapped — post_order visits shared nodes once
            // per reference, but we only want one new copy per distinct node.
            if remap.contains_key(&node.index()) {
                continue;
            }
            let new_children: Vec<Tree> = self
                .get_children(node)
                .iter()
                .map(|c| remap[&c.index()])
                .collect();
            let label = self.get_label(node).clone();
            let new_node = self.add_node(label, new_children);
            remap.insert(node.index(), new_node);
        }
        remap[&root.index()]
    }

    /// Apply a tree homomorphism to the subtree rooted at `tree`.
    ///
    /// The traversal is top-down but results are assembled bottom-up:
    ///
    /// 1. `f` maps each node's label from `E` to `Op`.
    /// 2. Children are recursed first, producing results of type `F`.
    /// 3. `alg.apply(op, child_results)` combines the mapped label with the
    ///    children's results to produce this node's result of type `F`.
    ///
    /// This is a catamorphism (fold) over the tree.  The algebra `alg` is
    /// mutable so it can accumulate state — for example, when the algebra is
    /// another [`TreeArena<Op>`], `apply` inserts each node as it is processed,
    /// and the return value is the new root handle.
    ///
    /// See [`MutAlgebra`] for implementing custom algebras, and
    /// [`copy_into`](Self::copy_into) for the common copy-with-label-transform
    /// use case.
    pub fn map<Op, F>(
        &self,
        tree: Tree,
        f: impl Fn(&E) -> Op,
        alg: &mut impl MutAlgebra<Op, F>,
    ) -> F {
        self.map_int(tree, &f, alg)
    }

    /// Internal recursive implementation of [`map`](Self::map).
    ///
    /// Separated from the public method so `f` can be passed by shared
    /// reference without being re-borrowed on each recursive call.
    fn map_int<Op, F>(
        &self,
        tree: Tree,
        f: &impl Fn(&E) -> Op,
        alg: &mut impl MutAlgebra<Op, F>,
    ) -> F {
        let node = self.get_node(tree);
        let op = f(&node.label);

        let new_children: Vec<F> = self
            .get_children(tree)
            .iter()
            .map(|child_id| self.map_int(*child_id, f, alg))
            .collect();

        alg.apply(op, new_children)
    }

    /// Look up a node by handle, panicking if the index is out of range.
    ///
    /// Using a [`Tree`] handle from a different arena will either panic here
    /// or silently return the wrong node — there is no arena-ID check.  This
    /// is a deliberate tradeoff: arena IDs would require storing them in every
    /// handle and would make the zero-copy `get_children` slice impossible.
    fn get_node(&self, tree: Tree) -> &Node<E> {
        self.nodes.get(tree.index()).unwrap()
    }
}

/// An algebra for use with [`TreeArena::map`].
///
/// `Op` is the type produced by mapping a node's label (the "operation" at
/// that position in the tree), and `E` is the result type for the whole
/// subtree after the children have been processed.
///
/// The algebra is mutable so it can accumulate output — the canonical example
/// is [`TreeArena<Op>`] itself, where each `apply` call inserts a new node and
/// returns its handle.
///
/// # Example: counting nodes
///
/// ```rust
/// use packed_term_arena::tree::{TreeArena, MutAlgebra};
///
/// struct Counter;
///
/// impl MutAlgebra<(), usize> for Counter {
///     fn apply(&mut self, _op: (), children: Vec<usize>) -> usize {
///         1 + children.into_iter().sum::<usize>()
///     }
/// }
///
/// let mut arena = TreeArena::new();
/// let root = packed_term_arena::tree!(arena, ("root", ("f", "a", "b"), "c"));
/// let count = arena.map(root, |_| (), &mut Counter);
/// assert_eq!(count, 5);
/// ```
pub trait MutAlgebra<Op, E> {
    /// Combine the mapped label `op` with the already-processed child results
    /// to produce this node's result.
    fn apply(&mut self, op: Op, children: Vec<E>) -> E;
}

/// [`TreeArena<E>`] is its own algebra: `apply` inserts a node and returns the
/// handle.  This is what [`copy_into`](TreeArena::copy_into) uses under the hood.
impl<E> MutAlgebra<E, Tree> for TreeArena<E> {
    fn apply(&mut self, op: E, children: Vec<Tree>) -> Tree {
        self.add_node(op, children.clone())
    }
}

/// Helper returned by [`Tree::display`] that renders a subtree as a string.
///
/// Leaf nodes are printed as their label alone.  Internal nodes are printed
/// as `label(child1, child2, ...)`.  Labels are formatted with their [`Display`]
/// implementation.
pub struct TreeDisplay<'a, E: Display> {
    arena: &'a TreeArena<E>,
    tree: &'a Tree,
}

impl<E: Display> TreeDisplay<'_, E> {
    fn write_subtree(&self, id: &Tree, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let node = self.arena.get_node(*id);
        let mut first = true;

        write!(f, "{}", node.label)?;

        if !node.children.is_empty() {
            write!(f, "(")?;

            for child_id in self.arena.get_children(*id) {
                if first {
                    first = false;
                } else {
                    write!(f, ", ")?;
                }
                self.write_subtree(child_id, f)?;
            }

            write!(f, ")")?;
        }
        Ok(())
    }
}

impl<'a, E: Display> Display for TreeDisplay<'a, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_subtree(self.tree, f)
    }
}

impl Tree {
    /// Return a [`Display`]-able view of the subtree rooted at this node.
    ///
    /// Requires the label type `E` to implement [`Display`].  The format is
    /// `label` for leaves and `label(c1, c2, ...)` for internal nodes,
    /// rendered recursively.
    ///
    /// # Example
    ///
    /// ```rust
    /// use packed_term_arena::tree::TreeArena;
    ///
    /// let mut arena = TreeArena::new();
    /// let root = packed_term_arena::tree!(arena, ("f", "a", "b"));
    /// assert_eq!(root.display(&arena).to_string(), "f(a, b)");
    /// ```
    pub fn display<'a, E: Display>(&'a self, arena: &'a TreeArena<E>) -> TreeDisplay<'a, E> {
        TreeDisplay { arena, tree: self }
    }
}

/// Build a tree in an existing arena using a concise nested-tuple syntax.
///
/// Each invocation adds nodes to `$arena` and returns the root [`Tree`] handle.
///
/// # Syntax
///
/// - **Leaf**: `tree!(arena, label)` — adds a single node with no children.
/// - **Internal node**: `tree!(arena, (label, child1, child2, ...))` — adds
///   the children recursively first (left to right), then adds the parent.
///   A trailing comma after the last child is accepted.
///
/// Labels are arbitrary Rust expressions evaluated in declaration order, so
/// they can be variables, function calls, or block expressions.
///
/// # Example
///
/// ```rust
/// use packed_term_arena::tree::TreeArena;
///
/// let mut arena = TreeArena::new();
/// let root = packed_term_arena::tree!(arena, ("f", ("g", "a", "b"), "c"));
/// assert_eq!(root.display(&arena).to_string(), "f(g(a, b), c)");
/// ```
#[macro_export]
macro_rules! tree {
    ($arena:expr, ($label:expr $(, $child:tt)* $(,)?)) => {{
        let children = vec![
            $(
                $crate::tree!($arena, $child)
            ),*
        ];

        $arena.add_node($label, children)
    }};

    ($arena:expr, $label:expr) => {{ $arena.add_node($label, vec![]) }};
}

///////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    // ── Arena basics ──────────────────────────────────────────────────────────

    // Verifies that a fresh arena has no nodes or child references.
    #[test]
    fn new_arena_starts_empty() {
        let arena = TreeArena::<&str>::new();

        assert!(arena.nodes.is_empty());
        assert!(arena.children.is_empty());
    }

    // Verifies that nodes receive stable IDs in insertion order.
    #[test]
    fn add_node_returns_stable_sequential_tree_ids() {
        let mut arena = TreeArena::new();

        let first = arena.add_node("first", vec![]);
        let second = arena.add_node("second", vec![]);
        let third = arena.add_node("third", vec![first, second]);

        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
        assert_eq!(third.index(), 2);
    }

    // Verifies that labels and child slices are stored for leaves and parents.
    #[test]
    fn stores_labels_and_children_for_leaf_and_parent_nodes() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("root", "left", "right"));
        let left = arena.get_children(root)[0];
        let right = arena.get_children(root)[1];

        assert_eq!(arena.get_label(left), &"left");
        assert_eq!(arena.get_label(right), &"right");
        assert_eq!(arena.get_label(root), &"root");
        assert_eq!(arena.get_children(left), &[][..]);
        assert_eq!(arena.get_children(root), &[left, right][..]);
    }

    // Verifies that len/is_empty track node count correctly.
    #[test]
    fn len_and_is_empty_reflect_node_count() {
        let mut arena = TreeArena::<&str>::new();
        assert_eq!(arena.len(), 0);
        assert!(arena.is_empty());

        arena.add_node("a", vec![]);
        assert_eq!(arena.len(), 1);
        assert!(!arena.is_empty());

        arena.add_node("b", vec![]);
        assert_eq!(arena.len(), 2);
    }

    // Verifies that earlier child ranges still point to the same children after later inserts.
    #[test]
    fn child_slices_remain_stable_after_more_nodes_are_added() {
        let mut arena = TreeArena::new();
        let a = arena.add_node("a", vec![]);
        let b = arena.add_node("b", vec![]);
        let first_parent = arena.add_node("first_parent", vec![a, b]);
        let c = arena.add_node("c", vec![]);
        let second_parent = arena.add_node("second_parent", vec![c, first_parent]);

        assert_eq!(arena.get_children(first_parent), &[a, b][..]);
        assert_eq!(arena.get_children(second_parent), &[c, first_parent][..]);
    }

    // Verifies that infallible accessors panic for a tree ID outside the arena.
    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn invalid_tree_id_panics_from_accessors() {
        let mut arena = TreeArena::new();
        arena.add_node("only", vec![]);

        arena.get_label(Tree(99));
    }

    // ── tree! macro ───────────────────────────────────────────────────────────

    // Verifies the bare-leaf arm: tree!(arena, expr) with no children.
    #[test]
    fn tree_macro_bare_leaf() {
        let mut arena = TreeArena::new();
        let leaf = tree!(arena, "leaf");

        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get_label(leaf), &"leaf");
        assert!(arena.get_children(leaf).is_empty());
    }

    // Verifies the tuple arm with exactly one child.
    #[test]
    fn tree_macro_single_child() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("f", "a"));

        assert_eq!(root.display(&arena).to_string(), "f(a)");
        assert_eq!(arena.get_children(root).len(), 1);
    }

    // Verifies nested multi-child construction and insertion into an existing arena.
    #[test]
    fn tree_macro_nested_multi_child() {
        let mut arena = TreeArena::new();
        let existing = arena.add_node("existing", vec![]);
        let root = tree!(arena, ("root", ("f", "a", "b"), "c"));

        assert_eq!(existing.index(), 0);
        assert_eq!(root.display(&arena).to_string(), "root(f(a, b), c)");
        assert_eq!(arena.get_label(root), &"root");
    }

    // Verifies that labels can be arbitrary Rust expressions and a trailing
    // comma is accepted after the last child.
    #[test]
    fn tree_macro_accepts_label_expressions_and_trailing_comma() {
        let mut arena = TreeArena::new();
        let prefix = "leaf";
        let root = tree!(
            arena,
            (format!("{}-root", prefix), { prefix.to_string() }, {
                String::from("right")
            },)
        );

        assert_eq!(root.display(&arena).to_string(), "leaf-root(leaf, right)");
    }

    // Verifies that tree! works with non-string label types.
    #[test]
    fn tree_macro_accepts_non_string_labels() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, (1, (2, 3, 4), 5));

        assert_eq!(arena.get_label(root), &1);
        assert_eq!(root.display(&arena).to_string(), "1(2(3, 4), 5)");
    }

    // ── Display ───────────────────────────────────────────────────────────────

    // Verifies the display format for a leaf node.
    #[test]
    fn display_formats_leaf_nodes() {
        let mut arena = TreeArena::new();
        let leaf = tree!(arena, "leaf");

        assert_eq!(leaf.display(&arena).to_string(), "leaf");
    }

    // Verifies recursive display formatting and child ordering.
    #[test]
    fn display_formats_nested_trees_in_child_order() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("root", ("f", "a", "b"), "c"));

        assert_eq!(root.display(&arena).to_string(), "root(f(a, b), c)");
    }

    // Verifies that repeated child references are rendered each time they appear.
    #[test]
    fn display_renders_shared_node_at_each_reference() {
        let mut arena = TreeArena::new();
        let shared = arena.add_node("shared", vec![]);
        let root = arena.add_node("root", vec![shared, shared]);

        assert_eq!(root.display(&arena).to_string(), "root(shared, shared)");
    }

    // ── map / MutAlgebra ─────────────────────────────────────────────────────

    // Verifies that map transforms a leaf and places it in the target arena.
    #[test]
    fn map_transforms_leaf_into_target_arena() {
        let mut source = TreeArena::new();
        let leaf = tree!(source, 7);
        let mut target = TreeArena::new();

        let mapped = source.map(leaf, |value| value * 2, &mut target);

        assert_eq!(mapped.index(), 0);
        assert_eq!(target.get_label(mapped), &14);
        assert_eq!(target.get_children(mapped), &[][..]);
    }

    // Verifies that map transforms a nested tree while leaving the source unchanged.
    #[test]
    fn map_transforms_nested_tree_without_mutating_source() {
        let mut source = TreeArena::new();
        let root = tree!(source, ("f", "a", "b"));
        let mut target: TreeArena<String> = TreeArena::new();

        let mapped = source.map(root, |value: &&str| value.to_uppercase(), &mut target);

        assert_eq!(root.display(&source).to_string(), "f(a, b)");
        assert_eq!(mapped.display(&target).to_string(), "F(A, B)");
    }

    // Verifies that map appends mapped nodes after any existing target nodes.
    #[test]
    fn map_appends_to_existing_target_arena() {
        let mut source = TreeArena::new();
        let root = tree!(source, ("root", "child"));
        let mut target = TreeArena::new();
        let existing = target.add_node("existing".to_string(), vec![]);

        let mapped = source.map(root, |value| value.to_string(), &mut target);

        assert_eq!(existing.index(), 0);
        assert_eq!(mapped.index(), 2);
        assert_eq!(existing.display(&target).to_string(), "existing");
        assert_eq!(mapped.display(&target).to_string(), "root(child)");
    }

    // Verifies that map passes child results to the algebra in left-to-right order.
    #[test]
    fn map_passes_child_results_to_algebra_in_order() {
        struct PrefixHom;
        impl MutAlgebra<&'static str, String> for PrefixHom {
            fn apply(&mut self, op: &'static str, children: Vec<String>) -> String {
                if children.is_empty() {
                    op.to_string()
                } else {
                    format!("{}[{}]", op, children.join("|"))
                }
            }
        }

        let mut arena = TreeArena::new();
        let root = tree!(arena, ("root", ("pair", "a", "b"), "c"));

        let mapped = arena.map(root, |value| *value, &mut PrefixHom);

        assert_eq!(mapped, "root[pair[a|b]|c]");
    }

    // Verifies that map panics when given a tree ID not in the arena.
    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn map_panics_for_invalid_tree_id() {
        let arena = TreeArena::<&str>::new();
        let mut target = TreeArena::new();

        arena.map(Tree(0), |value| *value, &mut target);
    }

    // Verifies that map follows every structural edge, creating an independent
    // copy for each reference to a shared node (sharing is unfolded).
    #[test]
    fn map_unfolds_shared_node_into_independent_copies() {
        let mut source = TreeArena::new();
        let shared = source.add_node("shared", vec![]);
        let root = source.add_node("root", vec![shared, shared]);
        let mut target = TreeArena::<&str>::new();

        let new_root = source.copy_into(root, &mut target);

        // Two independent copies of shared plus the root copy = 3 nodes.
        assert_eq!(target.len(), 3);
        let children = target.get_children(new_root);
        assert_ne!(children[0], children[1]);
        assert_eq!(target.get_label(children[0]), &"shared");
        assert_eq!(target.get_label(children[1]), &"shared");
    }

    // ── post_order ────────────────────────────────────────────────────────────

    // Verifies post_order on a single leaf.
    #[test]
    fn post_order_single_node() {
        let mut arena = TreeArena::new();
        let leaf = tree!(arena, "leaf");

        let order: Vec<Tree> = arena.post_order(leaf).collect();

        assert_eq!(order, vec![leaf]);
    }

    // Verifies that children appear before their parent, left subtrees before right.
    #[test]
    fn post_order_visits_children_before_parent() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("root", ("f", "a", "b"), "c"));

        let labels: Vec<&str> = arena.post_order(root).map(|n| *arena.get_label(n)).collect();

        assert_eq!(labels, vec!["a", "b", "f", "c", "root"]);
    }

    // Verifies correct post-order for a deep right-skewed chain.
    #[test]
    fn post_order_right_skewed_chain() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("a", ("b", ("c", "d"))));

        let labels: Vec<&str> = arena.post_order(root).map(|n| *arena.get_label(n)).collect();

        assert_eq!(labels, vec!["d", "c", "b", "a"]);
    }

    // Verifies that post_order follows structural edges, visiting a shared node
    // once per place it appears in a child list.
    #[test]
    fn post_order_visits_shared_node_per_reference() {
        let mut arena = TreeArena::new();
        let shared = arena.add_node("shared", vec![]);
        let root = arena.add_node("root", vec![shared, shared]);

        let order: Vec<Tree> = arena.post_order(root).collect();

        assert_eq!(order, vec![shared, shared, root]);
    }

    // ── copy_into ────────────────────────────────────────────────────────────

    // Verifies that copy_into produces an independent subtree in the target arena.
    #[test]
    fn copy_into_produces_independent_subtree() {
        let mut source = TreeArena::new();
        let root = tree!(source, ("root", "child"));
        let mut target = TreeArena::<&str>::new();

        let new_root = source.copy_into(root, &mut target);

        assert_eq!(target.get_label(new_root), &"root");
        assert_eq!(target.get_label(target.get_children(new_root)[0]), &"child");
        assert_eq!(target.len(), 2);
        assert_eq!(source.len(), 2); // source unchanged
    }

    // Verifies that copy_into appends nodes after any existing nodes in the target.
    #[test]
    fn copy_into_appends_after_existing_target_nodes() {
        let mut source = TreeArena::new();
        let root = tree!(source, ("root", "a"));
        let mut target = TreeArena::new();
        let existing = target.add_node("existing", vec![]);

        let new_root = source.copy_into(root, &mut target);

        assert_eq!(existing.index(), 0);
        assert_eq!(new_root.index(), 2);
        assert_eq!(target.get_label(new_root), &"root");
    }

    // ── dup_subtree ───────────────────────────────────────────────────────────

    // Verifies that dup_subtree appends a full copy and returns the new root.
    #[test]
    fn dup_subtree_appends_copy_to_same_arena() {
        let mut arena = TreeArena::new();
        let root = tree!(arena, ("root", "a", "b"));
        let a = arena.get_children(root)[0];
        let b = arena.get_children(root)[1];

        let dup = arena.dup_subtree(root);

        assert_eq!(arena.len(), 6); // original 3 + 3 new
        assert_eq!(arena.get_label(dup), &"root");
        let dup_children = arena.get_children(dup);
        assert_ne!(dup_children[0], a);
        assert_ne!(dup_children[1], b);
        assert_eq!(arena.get_label(dup_children[0]), &"a");
        assert_eq!(arena.get_label(dup_children[1]), &"b");
    }

    // Verifies that dup_subtree only copies nodes reachable from the given root.
    #[test]
    fn dup_subtree_only_copies_reachable_nodes() {
        let mut arena = TreeArena::new();
        let unrelated = arena.add_node("unrelated", vec![]);
        let root = tree!(arena, ("root", "a"));

        let dup = arena.dup_subtree(root);

        // unrelated + a + root + dup_a + dup_root = 5
        assert_eq!(arena.len(), 5);
        let _ = unrelated;
        assert_eq!(arena.get_label(dup), &"root");
    }

    // Verifies that dup_subtree works on a single leaf.
    #[test]
    fn dup_subtree_leaf() {
        let mut arena = TreeArena::new();
        let leaf = tree!(arena, "leaf");

        let dup = arena.dup_subtree(leaf);

        assert_eq!(arena.len(), 2);
        assert_ne!(dup, leaf);
        assert_eq!(arena.get_label(dup), &"leaf");
        assert!(arena.get_children(dup).is_empty());
    }

    // Verifies that dup_subtree creates exactly one new node per distinct source
    // node, preserving sharing without creating orphaned nodes.
    #[test]
    fn dup_subtree_preserves_sharing_without_orphans() {
        let mut arena = TreeArena::new();
        let shared = arena.add_node("shared", vec![]);
        let root = arena.add_node("root", vec![shared, shared]);

        let dup = arena.dup_subtree(root);

        // shared + root + dup_shared + dup_root = 4 (not 5)
        assert_eq!(arena.len(), 4);
        assert_eq!(arena.get_label(dup), &"root");
        let dup_children = arena.get_children(dup);
        assert_eq!(dup_children[0], dup_children[1]); // both point to one new node
        assert_ne!(dup_children[0], shared);
        assert_eq!(arena.get_label(dup_children[0]), &"shared");
    }
}
