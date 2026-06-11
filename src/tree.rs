use std::fmt::Display;

#[derive(Debug)]
pub struct Node {
    pub id: usize,
    pub children: Vec<usize>,
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

    pub fn add_node(&mut self, value: E, children: Vec<usize>) -> usize {
        let index = self.nodes.len();
        self.labels.push(value);
        self.nodes.push(Node {
            id: index,
            children: children,
        });
        index
    }

    pub fn get_node(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    fn map_into_rec<F>(&self, id: usize, f: &impl Fn(&E) -> F, target_arena: &mut TreeArena<F>) -> usize {
        let node = self.get_node(id).unwrap();
        let mapped_value = f(&self.labels[id]);

        let mut new_children_ids : Vec<usize> = vec![];

        for child_id in &node.children {
            let mapped_child_id = self.map_into_rec(*child_id, f, target_arena);
            new_children_ids.push(mapped_child_id);
        }

        target_arena.add_node(mapped_value, new_children_ids)
    }

    pub fn map_into<'a, F>(&self, node_id: usize, f: impl Fn(&E) -> F,target_arena: &mut TreeArena<F>) -> usize {
        self.map_into_rec(node_id, &f, target_arena)
    }
}






/// A struct that displays a tree node as a string.

pub struct TreeDisplay<'a, E: Display> {
    arena: &'a TreeArena<E>,
    node: &'a Node,
}

impl<E: Display> TreeDisplay<'_, E> {
    fn write_subtree(&self, node: &Node, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;

        write!(f, "{}", self.arena.labels[node.id])?;

        if !node.children.is_empty() {
            write!(f, "(")?;

            for child_id in &node.children {
                if first {
                    first = false;
                } else {
                    write!(f, ", ")?;
                }

                let child = &self.arena.nodes[*child_id];
                self.write_subtree(child, f)?;
            }

            write!(f, ")")?;
        }
        Ok(())
    }
}

impl<'a, E: Display> Display for TreeDisplay<'a, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_subtree(self.node, f)?;
        Ok(())
    }
}

impl Node {
    pub fn display<'a, E: Display>(&'a self, arena: &'a TreeArena<E>) -> TreeDisplay<'a, E> {
        TreeDisplay {
            arena: arena,
            node: self,
        }
    }
}
