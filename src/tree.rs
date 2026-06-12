use std::fmt::Display;
use std::ops::Range;

// This is the "data structure" that we use on the outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tree(usize);

impl Tree {
    pub fn index(self) -> usize {
        self.0
    }
}

// This is the data structure that actually holds the content.
// It is only used internally in this crate.
#[derive(Debug)]
struct Node<E> {
    pub children: Range<usize>,
    pub label: E,
}

#[derive(Debug)]
pub struct TreeArena<E> {
    nodes: Vec<Node<E>>,
    children: Vec<Tree>,
}

impl<E> TreeArena<E> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            children: Vec::new(),
        }
    }

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

    /// Given that Tree is opaque, in that the user can't create new Tree objects on the outside,
    /// let's assume that the index exists. The one situation where this would break is when a Tree
    /// from one arena is looked up in another arena. This could be fixed with arena IDs, but it
    /// seems like computational overkill (the nice slice construction of get_children would
    /// no longer work), so let's play it a little unsafe.
    fn get_node(&self, tree: Tree) -> &Node<E> {
        self.nodes.get(tree.index()).unwrap()
    }

    pub fn get_label(&self, tree: Tree) -> &E {
        &self.get_node(tree).label
    }

    pub fn get_children(&self, tree: Tree) -> &[Tree] {
        &self.children[self.get_node(tree).children.clone()]
    }

    pub fn map<Op, F>(
        &self,
        tree: Tree,
        f: impl Fn(&E) -> Op,
        alg: &mut impl MutAlgebra<Op, F>,
    ) -> F {
        self.map_int(tree, &f, alg)
    }

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
}

pub trait MutAlgebra<Op, E> {
    fn apply(&mut self, op: Op, children: Vec<E>) -> E;
}

impl<E> MutAlgebra<E, Tree> for TreeArena<E> {
    fn apply(&mut self, op: E, children: Vec<Tree>) -> Tree {
        self.add_node(op, children.clone())
    }
}

/// A struct that displays a tree node as a string.

pub struct TreeDisplay<'a, E: Display> {
    arena: &'a TreeArena<E>,
    tree: &'a Tree,
}

impl<E: Display> TreeDisplay<'_, E> {
    fn write_subtree(&self, id: &Tree, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        let node = self.arena.get_node(*id);

        write!(f, "{}", node.label)?;

        if !node.children.is_empty() {
            write!(f, "(")?;

            for child_id in self.arena.get_children(*id) {
                if first {
                    first = false;
                } else {
                    write!(f, ", ")?;
                }

                // let child = &self.arena.nodes[*child_id];
                self.write_subtree(child_id, f)?;
            }

            write!(f, ")")?;
        }
        Ok(())
    }
}

impl<'a, E: Display> Display for TreeDisplay<'a, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_subtree(self.tree, f)?;
        Ok(())
    }
}

impl Tree {
    pub fn display<'a, E: Display>(&'a self, arena: &'a TreeArena<E>) -> TreeDisplay<'a, E> {
        TreeDisplay {
            arena: arena,
            tree: self,
        }
    }
}

// impl Node {
//     pub fn display<'a, E: Display>(&'a self, arena: &'a TreeArena<E>) -> TreeDisplay<'a, E> {
//         TreeDisplay {
//             arena: arena,
//             node: self,
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

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
        let left = arena.add_node("left", vec![]);
        let right = arena.add_node("right", vec![]);
        let root = arena.add_node("root", vec![left, right]);

        assert_eq!(arena.get_label(left), &"left");
        assert_eq!(arena.get_label(right), &"right");
        assert_eq!(arena.get_label(root), &"root");
        assert_eq!(arena.get_children(left), &[][..]);
        assert_eq!(arena.get_children(root), &[left, right][..]);
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
        let missing = Tree(99);

        arena.get_label(missing);
    }

    // Verifies the display format for a leaf node.
    #[test]
    fn display_formats_leaf_nodes() {
        let mut arena = TreeArena::new();
        let leaf = arena.add_node("leaf", vec![]);

        assert_eq!(leaf.display(&arena).to_string(), "leaf");
    }

    // Verifies recursive display formatting and child ordering.
    #[test]
    fn display_formats_nested_trees_in_child_order() {
        let mut arena = TreeArena::new();
        let a = arena.add_node("a", vec![]);
        let b = arena.add_node("b", vec![]);
        let c = arena.add_node("c", vec![]);
        let f = arena.add_node("f", vec![a, b]);
        let root = arena.add_node("root", vec![f, c]);

        assert_eq!(root.display(&arena).to_string(), "root(f(a, b), c)");
    }

    // Verifies that repeated child references are displayed each time they appear.
    #[test]
    fn display_reuses_shared_subtrees_where_referenced() {
        let mut arena = TreeArena::new();
        let shared = arena.add_node("shared", vec![]);
        let root = arena.add_node("root", vec![shared, shared]);

        assert_eq!(root.display(&arena).to_string(), "root(shared, shared)");
    }

    // Verifies that map_into transforms a leaf into the target arena.
    #[test]
    fn map_into_maps_leaf_values_into_target_arena() {
        let mut source = TreeArena::new();
        let leaf = source.add_node(7, vec![]);
        let mut target = TreeArena::new();

        let mapped = source.map(leaf, |value| value * 2, &mut target);

        assert_eq!(mapped.index(), 0);
        assert_eq!(target.get_label(mapped), &14);
        assert_eq!(target.get_children(mapped), &[][..]);
    }

    // Verifies that map_into maps a nested tree while leaving the source unchanged.
    #[test]
    fn map_into_maps_nested_tree_without_mutating_source() {
        let mut source = TreeArena::new();
        let a = source.add_node("a".to_string(), vec![]);
        let b = source.add_node("b".to_string(), vec![]);
        let root = source.add_node("f".to_string(), vec![a, b]);
        let mut target = TreeArena::new();

        let mapped = source.map(root, |value| value.to_uppercase(), &mut target);

        assert_eq!(root.display(&source).to_string(), "f(a, b)");
        assert_eq!(mapped.display(&target).to_string(), "F(A, B)");
    }

    // Verifies that map_into adds mapped nodes after existing target nodes.
    #[test]
    fn map_into_appends_to_existing_target_arena() {
        let mut source = TreeArena::new();
        let child = source.add_node("child", vec![]);
        let root = source.add_node("root", vec![child]);
        let mut target = TreeArena::new();
        let existing = target.add_node("existing".to_string(), vec![]);

        let mapped = source.map(root, |value| value.to_string(), &mut target);

        assert_eq!(existing.index(), 0);
        assert_eq!(mapped.index(), 2);
        assert_eq!(existing.display(&target).to_string(), "existing");
        assert_eq!(mapped.display(&target).to_string(), "root(child)");
    }

    // Verifies that map_into_hom passes mapped child results to the homomorphism in order.
    #[test]
    fn map_into_hom_uses_child_results_in_order() {
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
        let a = arena.add_node("a", vec![]);
        let b = arena.add_node("b", vec![]);
        let c = arena.add_node("c", vec![]);
        let pair = arena.add_node("pair", vec![a, b]);
        let root = arena.add_node("root", vec![pair, c]);
        let mut hom = PrefixHom;

        let mapped = arena.map(root, |value| *value, &mut hom);

        assert_eq!(mapped, "root[pair[a|b]|c]");
    }

    // Verifies the current panic behavior when mapping from a missing tree ID.
    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn map_into_hom_panics_for_invalid_tree_id() {
        let arena = TreeArena::<&str>::new();
        let mut target = TreeArena::new();

        arena.map(Tree(0), |value| *value, &mut target);
    }
}
