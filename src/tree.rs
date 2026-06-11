use std::fmt::Display;

// This is the "data structure" that we use on the outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tree(pub usize);

impl Tree {
    pub fn index(self) -> usize {
        self.0
    }
}




// This is the data structure that actually holds the content.
// It is only used internally in this crate.
#[derive(Debug)]
struct Node {
    // pub id: usize,
    pub children: Vec<Tree>,
}

#[derive(Debug)]
pub struct TreeArena<E> {
    nodes: Vec<Node>,
    labels: Vec<E>,
}

impl<E> TreeArena<E> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            labels: Vec::new(),
        }
    }

    pub fn add_node(&mut self, value: E, children: Vec<Tree>) -> Tree {
        let index = self.nodes.len();
        self.labels.push(value);
        self.nodes.push(Node {
            children: children,
        });

        Tree(index)
    }

    fn get_node(&self, tree: &Tree) -> Option<&Node> {
        self.nodes.get(tree.index())
    }

    fn map_into_rec<F>(&self, tree: Tree, f: &impl Fn(&E) -> F, target_arena: &mut TreeArena<F>) -> Tree {
        let node = self.get_node(&tree).unwrap();
        let mapped_value = f(&self.labels[tree.index()]);

        let mut new_children_ids : Vec<Tree> = vec![];

        for child_id in &node.children {
            let mapped_child_id = self.map_into_rec(*child_id, f, target_arena);
            new_children_ids.push(mapped_child_id);
        }

        target_arena.add_node(mapped_value, new_children_ids)
    }

    pub fn map_into<'a, F>(&self, tree: Tree, f: impl Fn(&E) -> F,target_arena: &mut TreeArena<F>) -> Tree {
        self.map_into_rec(tree, &f, target_arena)
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
        let node = self.arena.get_node(id).unwrap();

        write!(f, "{}", self.arena.labels[id.index()])?;

        if !node.children.is_empty() {
            write!(f, "(")?;

            for child_id in &node.children {
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
