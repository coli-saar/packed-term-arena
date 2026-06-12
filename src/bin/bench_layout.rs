use std::hint::black_box;
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Node {
    label: u64,
    children: Range<usize>, // range into Arena::children
}

struct Arena {
    nodes: Vec<Node>,
    children: Vec<usize>,
    root: usize,
}

impl Arena {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            children: Vec::new(),
            root: 0,
        }
    }

    fn push_placeholder(&mut self, label: u64) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node {
            label,
            children: 0..0,
        });
        index
    }

    fn finish_node(&mut self, index: usize, children: &[usize]) {
        let start = self.children.len();
        self.children.extend_from_slice(children);
        let end = self.children.len();
        self.nodes[index].children = start..end;
    }

    fn add_node_postorder(&mut self, label: u64, children: &[usize]) -> usize {
        let index = self.nodes.len();
        let start = self.children.len();
        self.children.extend_from_slice(children);
        let end = self.children.len();

        self.nodes.push(Node {
            label,
            children: start..end,
        });

        index
    }

    fn traverse_sum(&self) -> u64 {
        let mut sum = 0u64;
        let mut stack = vec![self.root];

        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            sum = sum.wrapping_add(node.label);
            sum = sum.wrapping_add(index as u64);

            // Reverse push preserves left-to-right DFS visit order.
            for child in self.children[node.children.clone()].iter().rev() {
                stack.push(*child);
            }
        }

        sum
    }
}

fn balanced_preorder(depth: usize) -> Arena {
    fn build(arena: &mut Arena, depth: usize, next_label: &mut u64) -> usize {
        let label = *next_label;
        *next_label += 1;

        let index = arena.push_placeholder(label);

        if depth == 0 {
            arena.finish_node(index, &[]);
        } else {
            let left = build(arena, depth - 1, next_label);
            let right = build(arena, depth - 1, next_label);
            arena.finish_node(index, &[left, right]);
        }

        index
    }

    let mut arena = Arena::new();
    let mut next_label = 0;
    arena.root = build(&mut arena, depth, &mut next_label);
    arena
}

fn balanced_postorder(depth: usize) -> Arena {
    fn build(arena: &mut Arena, depth: usize, next_label: &mut u64) -> usize {
        if depth == 0 {
            let label = *next_label;
            *next_label += 1;
            arena.add_node_postorder(label, &[])
        } else {
            let left = build(arena, depth - 1, next_label);
            let right = build(arena, depth - 1, next_label);

            let label = *next_label;
            *next_label += 1;
            arena.add_node_postorder(label, &[left, right])
        }
    }

    let mut arena = Arena::new();
    let mut next_label = 0;
    arena.root = build(&mut arena, depth, &mut next_label);
    arena
}

fn chain_preorder(len: usize) -> Arena {
    let mut arena = Arena::new();

    for i in 0..len {
        arena.push_placeholder(i as u64);
    }

    for i in 0..len {
        if i + 1 < len {
            arena.finish_node(i, &[i + 1]);
        } else {
            arena.finish_node(i, &[]);
        }
    }

    arena.root = 0;
    arena
}

fn chain_postorder(len: usize) -> Arena {
    let mut arena = Arena::new();

    let mut child = arena.add_node_postorder(0, &[]);
    for i in 1..len {
        child = arena.add_node_postorder(i as u64, &[child]);
    }

    arena.root = child;
    arena
}

fn wide_preorder(width: usize) -> Arena {
    let mut arena = Arena::new();

    let root = arena.push_placeholder(0);
    let mut children = Vec::with_capacity(width);

    for i in 0..width {
        let child = arena.push_placeholder((i + 1) as u64);
        arena.finish_node(child, &[]);
        children.push(child);
    }

    arena.finish_node(root, &children);
    arena.root = root;
    arena
}

fn wide_postorder(width: usize) -> Arena {
    let mut arena = Arena::new();
    let mut children = Vec::with_capacity(width);

    for i in 0..width {
        children.push(arena.add_node_postorder((i + 1) as u64, &[]));
    }

    arena.root = arena.add_node_postorder(0, &children);
    arena
}

#[derive(Debug, Clone, Copy)]
struct Stats {
    min: f64,
    median: f64,
    max: f64,
}

impl Stats {
    fn from_samples(samples: &[f64]) -> Self {
        assert!(!samples.is_empty());

        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));

        Self {
            min: sorted[0],
            median: sorted[sorted.len() / 2],
            max: sorted[sorted.len() - 1],
        }
    }
}

fn bench(arena: &Arena, iterations: usize) -> Duration {
    let start = Instant::now();
    let mut checksum = 0u64;

    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(arena).traverse_sum());
    }

    black_box(checksum);
    start.elapsed()
}

fn ns_per_node(duration: Duration, iterations: usize, nodes: usize) -> f64 {
    duration.as_nanos() as f64 / iterations as f64 / nodes as f64
}

fn run_pair(name: &str, preorder: Arena, postorder: Arena, iterations: usize, trials: usize) {
    assert_eq!(preorder.nodes.len(), postorder.nodes.len());
    assert!(trials > 0);

    let nodes = preorder.nodes.len();
    let mut preorder_samples = Vec::with_capacity(trials);
    let mut postorder_samples = Vec::with_capacity(trials);
    let mut ratio_samples = Vec::with_capacity(trials);

    // Warm up both layouts before collecting samples.
    black_box(preorder.traverse_sum());
    black_box(postorder.traverse_sum());

    for trial in 0..trials {
        // Alternate measurement order to reduce bias from cache warmth, CPU frequency,
        // and branch predictor state.
        let (pre, post) = if trial % 2 == 0 {
            let pre = bench(&preorder, iterations);
            let post = bench(&postorder, iterations);
            (pre, post)
        } else {
            let post = bench(&postorder, iterations);
            let pre = bench(&preorder, iterations);
            (pre, post)
        };

        let pre_ns = ns_per_node(pre, iterations, nodes);
        let post_ns = ns_per_node(post, iterations, nodes);

        preorder_samples.push(pre_ns);
        postorder_samples.push(post_ns);
        ratio_samples.push(post_ns / pre_ns);
    }

    let preorder_stats = Stats::from_samples(&preorder_samples);
    let postorder_stats = Stats::from_samples(&postorder_samples);
    let ratio_stats = Stats::from_samples(&ratio_samples);

    println!("{name}");
    println!("  nodes:      {nodes}");
    println!("  iterations: {iterations}");
    println!("  trials:     {trials}");
    println!(
        "  preorder:   min {:>8.3}, med {:>8.3}, max {:>8.3} ns/node",
        preorder_stats.min, preorder_stats.median, preorder_stats.max
    );
    println!(
        "  postorder:  min {:>8.3}, med {:>8.3}, max {:>8.3} ns/node",
        postorder_stats.min, postorder_stats.median, postorder_stats.max
    );
    println!(
        "  ratio:      min {:>8.3}, med {:>8.3}, max {:>8.3}x post/pre",
        ratio_stats.min, ratio_stats.median, ratio_stats.max
    );
    println!();
}

fn main() {
    let iterations = 100;
    let trials = 11;

    run_pair(
        "balanced binary, depth 18",
        balanced_preorder(18),
        balanced_postorder(18),
        iterations,
        trials,
    );

    run_pair(
        "long chain, 500k nodes",
        chain_preorder(500_000),
        chain_postorder(500_000),
        iterations,
        trials,
    );

    run_pair(
        "wide root, 500k leaves",
        wide_preorder(500_000),
        wide_postorder(500_000),
        iterations,
        trials,
    );
}
