use position::*;
use r#move::*;
use types::*;

use numpy::PyArray1;
use pyo3::prelude::*;
use rand::distributions::Distribution;
use rand::Rng;

#[derive(Clone)]
pub struct Node {
    pub n: u32,
    pub v: f32,
    pub p: f32,
    pub w: f32,
    pub m: Move,
    pub parent: usize,
    pub children: std::vec::Vec<usize>,
    pub is_terminal: bool,
    pub virtual_loss: f32,
    pub is_used: bool,
}

impl Node {
    pub fn new(parent: usize, m: Move, policy: f32, is_used: bool) -> Node {
        Node {
            n: 0,
            v: 0.0,
            p: policy,
            w: 0.0,
            m: m,
            parent: parent,
            children: Vec::new(),
            is_terminal: false,
            virtual_loss: 0.0,
            is_used: is_used,
        }
    }

    pub fn clear(&mut self) {
        self.n = 0;
        self.v = 0.0;
        self.p = 0.0;
        self.w = 0.0;
        self.m = NULL_MOVE;
        self.parent = 0;
        self.children.clear();
        self.children.shrink_to_fit();
        self.is_terminal = false;
        self.virtual_loss = 0.0;
        self.is_used = false;
    }

    pub fn get_puct(&self, parent_n: f32, forced_playouts: bool) -> f32 {
        const C_BASE: f32 = 19652.0;
        const C_INIT: f32 = 1.25;

        if self.is_terminal {
            if self.v == 0.0 {
                return std::f32::MAX;
            } else if self.v == 1.0 {
                return -1.0;
            }
        }

        // KataGo approach (https://arxiv.org/abs/1902.10565)
        if forced_playouts {
            let n_forced: f32 = (2.0 * self.p * parent_n).sqrt();
            if (self.n as f32) < n_forced {
                return std::f32::MAX;
            }
        }

        let c: f32 = ((1.0 + (self.n as f32) + C_BASE) / C_BASE).log2() + C_INIT;
        let q: f32 = if self.n as f32 + self.virtual_loss == 0.0 {
            0.0
        } else {
            1.0 - (self.w + self.virtual_loss) / (self.n as f32 + self.virtual_loss)
        };
        let u: f32 = c * self.p * parent_n.sqrt() / (1.0 + (self.n as f32) + self.virtual_loss);

        return q + u;
    }

    pub fn expanded(&self) -> bool {
        return self.children.len() > 0;
    }
}

#[pyclass]
pub struct MCTS {
    pub size: usize,
    pub game_tree: std::vec::Vec<Node>,
    pub node_index: usize,
    pub node_used_count: usize,

    prev_root: usize,
}

#[pymethods]
impl MCTS {
    #[new]
    pub fn new(obj: &PyRawObject, memory: f32) {
        let num_node: usize =
            (memory * 1024.0 * 1024.0 * 1024.0 / std::mem::size_of::<MCTS>() as f32) as usize;

        obj.init(MCTS {
            size: num_node,
            game_tree: vec![Node::new(0, NULL_MOVE, 0.0, false); num_node],
            node_index: 0,
            node_used_count: 0,
            prev_root: 0,
        });
    }

    pub fn clear(&mut self) {
        if self.prev_root != 0 {
            self.eliminate_except(self.prev_root, 0);
        }

        self.node_index = 1;
        self.node_used_count = 1;
        self.prev_root = 0;
    }

    pub fn set_root(&mut self, position: &Position, reuse: bool) -> usize {
        if reuse && self.game_tree[self.prev_root].is_used && position.ply > 0 {
            let last_move = position.kif[position.ply as usize - 1];

            let mut next_root: usize = 0;

            for child in &self.game_tree[self.prev_root].children {
                if self.game_tree[*child].m == last_move {
                    next_root = *child;
                    break;
                }
            }

            if next_root != 0 {
                assert!(self.game_tree[next_root].is_used);
                self.eliminate_except(self.prev_root, next_root);
                self.prev_root = next_root;
                self.game_tree[next_root].parent = 0;

                return next_root;
            }
        }

        self.clear();

        self.game_tree[1].is_used = true;
        self.node_index = 2;
        self.node_used_count = 2;

        self.prev_root = 1;
        return 1;
    }

    pub fn best_move(&self, node: usize) -> Move {
        let best_child: usize = self.select_n_max_child(node);

        return self.game_tree[best_child].m;
    }

    pub fn softmax_sample(&self, node: usize, temperature: f32) -> Move {
        let mut visit_max: i32 = 0;

        for child in &self.game_tree[node].children {
            if self.game_tree[*child].n as i32 > visit_max {
                visit_max = self.game_tree[*child].n as i32;
            }
        }

        let mut sum: f32 = 0.0;

        for child in &self.game_tree[node].children {
            sum += ((self.game_tree[*child].n as i32 - visit_max) as f32 / temperature).exp();
        }

        let mut rng = rand::thread_rng();
        let r: f32 = rng.gen();

        let mut cum: f32 = 0.0;

        for child in &self.game_tree[node].children {
            cum += ((self.game_tree[*child].n as i32 - visit_max) as f32 / temperature).exp() / sum;
            if r < cum {
                return self.game_tree[*child].m;
            }
        }

        return self.game_tree[self.game_tree[node].children[0]].m;
    }

    pub fn print(&self, root: usize) {
        println!(
            "usage: {:.3}% ({}/{})",
            self.node_used_count as f32 / self.size as f32 * 100.0,
            self.node_used_count,
            self.size
        );
        println!("playout: {}", self.game_tree[root].n);

        let best_child: usize = self.select_n_max_child(root);

        println!("N(s, a): {}", self.game_tree[best_child].n);
        println!("P(s, a): {}", self.game_tree[best_child].p);
        println!("V(s, a): {}", self.game_tree[best_child].v);
        println!(
            "Q(s, a): {}",
            if self.game_tree[best_child].n == 0 {
                0.0
            } else {
                self.game_tree[best_child].w / self.game_tree[best_child].n as f32
            }
        );
    }

    pub fn get_usage(&self) -> f32 {
        return self.node_used_count as f32 / self.size as f32;
    }

    pub fn get_nodes(&self) -> usize {
        return self.node_used_count;
    }

    pub fn select_leaf(
        &mut self,
        root_node: usize,
        position: &mut Position,
        forced_playouts: bool,
    ) -> usize {
        let mut node = root_node;

        loop {
            self.game_tree[node].virtual_loss += 1.0;

            if self.game_tree[node].is_terminal || !self.game_tree[node].expanded() {
                break;
            }

            node = self.select_puct_max_child(node, forced_playouts);

            assert!(node > 0);
            position.do_move(&self.game_tree[node].m);
        }

        return node;
    }

    pub fn evaluate(
        &mut self,
        node: usize,
        position: &Position,
        np_policy: &PyArray1<f32>,
        mut value: f32,
        dirichlet_noise: bool,
    ) -> f32 {
        if !dirichlet_noise && self.game_tree[node].n > 0 {
            return self.game_tree[node].v;
        }

        let policy = np_policy.as_array();
        let mut legal_policy_sum: f32 = 0.0;
        let mut policy_max: f32 = std::f32::MIN;
        let moves = position.generate_moves();

        for m in &moves {
            if policy[m.to_policy_index()] > policy_max {
                policy_max = policy[m.to_policy_index()];
            }
        }

        for m in &moves {
            legal_policy_sum += (policy[m.to_policy_index()] - policy_max).exp();
        }

        let (is_repetition, is_check_repetition) = position.is_repetition();

        if is_repetition || moves.len() == 0 || position.ply == MAX_PLY as u16 {
            self.game_tree[node].is_terminal = true;
        }

        // win or lose is determined by the game rule
        if self.game_tree[node].is_terminal {
            if is_check_repetition {
                value = 1.0;
            } else if is_repetition {
                value = if position.side_to_move == Color::WHITE { 0.0 } else { 1.0 }
            } else if position.ply == MAX_PLY as u16 {
                value = 0.5;
            } else {
                value = if position.kif[position.ply as usize - 1].piece.get_piece_type()
                    == PieceType::PAWN
                {
                    // 打ち歩詰め
                    1.0
                } else {
                    // 詰み
                    0.0
                };
            }
        }

        let mut noise: std::vec::Vec<f64> = vec![0.0; moves.len()];

        if dirichlet_noise {
            let mut noise_sum = 0.0;
            let gamma = rand::distributions::Gamma::new(0.34, 1.0);

            for (i, _) in moves.iter().enumerate() {
                let v = gamma.sample(&mut rand::thread_rng());

                noise[i] = v;
                noise_sum += v;
            }

            for v in &mut noise {
                *v /= noise_sum;
            }
        }

        // set policy and vaue
        if !self.game_tree[node].is_terminal {
            if self.game_tree[node].children.len() == 0 {
                for (i, m) in moves.iter().enumerate() {
                    let policy_index = m.to_policy_index();

                    let mut index = self.node_index;
                    loop {
                        if index == 0 {
                            index = 1;
                        }

                        if !self.game_tree[index].is_used {
                            let p = if dirichlet_noise {
                                ((policy[policy_index] - policy_max).exp() / legal_policy_sum)
                                    * 0.75
                                    + (noise[i] as f32) * 0.25
                            } else {
                                (policy[policy_index] - policy_max).exp() / legal_policy_sum
                            };

                            self.game_tree[index] = Node::new(node, *m, p, true);
                            self.game_tree[node].children.push(index);
                            self.node_index = (index + 1) % self.size;
                            self.node_used_count += 1;

                            break;
                        }
                        index = (index + 1) % self.size;
                    }
                }
            } else if dirichlet_noise {
                let children = self.game_tree[node].children.clone();

                for (i, child) in children.iter().enumerate() {
                    let policy_index = self.game_tree[*child].m.to_policy_index();

                    self.game_tree[*child].p = if dirichlet_noise {
                        ((policy[policy_index] - policy_max).exp() / legal_policy_sum) * 0.75
                            + (noise[i] as f32) * 0.25
                    } else {
                        (policy[policy_index] - policy_max).exp() / legal_policy_sum
                    };
                }
            }
        }

        self.game_tree[node].v = value;

        return value;
    }

    pub fn backpropagate(&mut self, leaf_node: usize, value: f32) {
        let mut node = leaf_node;
        let mut flip = false;

        while node != 0 {
            self.game_tree[node].w += if !flip { value } else { 1.0 - value };
            self.game_tree[node].n += 1;
            self.game_tree[node].virtual_loss -= 1.0;
            node = self.game_tree[node].parent;
            flip = !flip;
        }
    }

    /// dot言語で探索木を書き出す
    pub fn visualize(&self, node: usize, node_num: usize) -> String {
        let mut dot = String::new();

        dot.push_str("digraph game_tree {\n");

        let mut nodes: std::vec::Vec<usize> = Vec::new();

        let mut counter: usize = 0;
        nodes.push(node);

        while counter < node_num && nodes.len() > 0 {
            let mut n_max: i32 = -1;
            let mut n_max_node = 0;
            let mut index = 0;

            for (i, n) in nodes.iter().enumerate() {
                if self.game_tree[*n].n as i32 > n_max {
                    n_max = self.game_tree[*n].n as i32;
                    n_max_node = *n;
                    index = i;
                }
            }

            nodes.swap_remove(index);

            dot.push_str(
                &format!(
                    "  {} [label=\"N:{}\\nP:{:.3}\\nV:{:.3}\\nQ:{:.3}\"];\n",
                    n_max_node,
                    self.game_tree[n_max_node].n,
                    self.game_tree[n_max_node].p,
                    self.game_tree[n_max_node].v,
                    if self.game_tree[n_max_node].n == 0 {
                        0.0
                    } else {
                        self.game_tree[n_max_node].w / self.game_tree[n_max_node].n as f32
                    }
                )
                .to_string(),
            );
            if n_max_node != node {
                dot.push_str(
                    &format!(
                        "  {} -> {} [label=\"{}\"];\n",
                        self.game_tree[n_max_node].parent,
                        n_max_node,
                        self.game_tree[n_max_node].m.sfen()
                    )
                    .to_string(),
                );
            }

            counter += 1;
            for child in &self.game_tree[n_max_node].children {
                assert!(*child != 0);
                nodes.push(*child);
            }
        }

        dot.push_str("}");

        return dot;
    }

    /// プレイアウト回数，Q値, それぞれの手の訪問回数を出力する
    pub fn dump(
        &mut self,
        node: usize,
        target_pruning: bool,
        remove_zeros: bool,
    ) -> (u32, f32, std::vec::Vec<(String, u32)>) {
        let mut distribution: std::vec::Vec<(String, u32)> = std::vec::Vec::new();

        if target_pruning {
            let n_max_child = self.select_n_max_child(node);
            let children = self.game_tree[node].children.clone();

            let n_max_puct =
                self.game_tree[n_max_child].get_puct(self.game_tree[node].n as f32, false);

            for child in &children {
                if *child == n_max_child {
                    continue;
                }

                let n_forced: f32 =
                    (2.0 * self.game_tree[*child].p * self.game_tree[node].n as f32).sqrt();

                for remove in 1..n_forced as usize {
                    if self.game_tree[*child].n == 0 {
                        break;
                    }

                    self.game_tree[*child].n -= 1;
                    let puct = self.game_tree[*child]
                        .get_puct((self.game_tree[node].n - remove as u32) as f32, false);

                    if puct >= n_max_puct {
                        self.game_tree[*child].n += 1;
                        break;
                    }
                }
            }
        }

        let q: f32 = if self.game_tree[node].n == 0 {
            0.0
        } else {
            self.game_tree[node].w / self.game_tree[node].n as f32
        };

        let mut sum_n: u32 = 0;

        for child in &self.game_tree[node].children {
            if remove_zeros && self.game_tree[*child].n == 0 {
                continue;
            }

            distribution.push((self.game_tree[*child].m.sfen(), self.game_tree[*child].n));
            sum_n += self.game_tree[*child].n;
        }

        return (sum_n, q, distribution);
    }

    pub fn get_playouts(&self, node: usize, child_sum: bool) -> u32 {
        if child_sum {
            let mut sum: u32 = 0;

            for child in &self.game_tree[node].children {
                sum += self.game_tree[*child].n;
            }

            return sum;
        } else {
            return self.game_tree[node].n;
        }
    }

    /// nodeの子に関する情報を出力する
    pub fn debug(&self, node: usize) {
        for child in &self.game_tree[node].children {
            println!(
                "{}, p:{:.3}, v:{:.3}, w:{:.3}, n:{:.3}, puct:{:.3}, vloss: {:.3}, parentn: {}",
                self.game_tree[*child].m.sfen(),
                self.game_tree[*child].p,
                self.game_tree[*child].v,
                self.game_tree[*child].w,
                self.game_tree[*child].n,
                self.game_tree[*child].get_puct(self.game_tree[node].n as f32, false),
                self.game_tree[*child].virtual_loss,
                self.game_tree[node].n
            );
        }
    }

    pub fn info(&self, node: usize) -> (std::vec::Vec<Move>, f32) {
        let mut pv_moves: std::vec::Vec<Move> = std::vec::Vec::new();
        let mut q: f32 = 0.0;

        let mut pn: usize = node;
        let mut depth = 0;

        while self.game_tree[pn].expanded() {
            pn = self.select_n_max_child(pn);
            pv_moves.push(self.game_tree[pn].m);

            depth += 1;
            if depth == 1 {
                q = if self.game_tree[pn].n == 0 {
                    0.0
                } else {
                    1.0 - (self.game_tree[pn].w / self.game_tree[pn].n as f32)
                };
            }
        }

        (pv_moves, q)
    }
}

impl MCTS {
    fn eliminate_except(&mut self, root: usize, except_node: usize) {
        let mut nodes: std::vec::Vec<usize> = std::vec::Vec::new();

        nodes.push(root);

        while nodes.len() > 0 {
            let n = nodes.pop().unwrap();

            if n == except_node {
                continue;
            }

            for child in &self.game_tree[n].children {
                nodes.push(*child);
            }

            self.game_tree[n].clear();
            self.node_used_count -= 1;
        }
    }

    fn select_puct_max_child(&self, node: usize, forced_playouts: bool) -> usize {
        let mut puct_max: f32 = -1.0;
        let mut puct_max_child: usize = 0;

        for child in &self.game_tree[node].children {
            let puct = self.game_tree[*child].get_puct(
                self.game_tree[node].n as f32 + self.game_tree[node].virtual_loss,
                forced_playouts,
            );

            if puct_max_child == 0 || puct > puct_max {
                puct_max = puct;
                puct_max_child = *child;
            }
        }

        return puct_max_child;
    }

    fn select_n_max_child(&self, node: usize) -> usize {
        let mut n_max: u32 = 0;
        let mut n_max_child: usize = 0;

        for child in &self.game_tree[node].children {
            if n_max_child == 0 || self.game_tree[*child].n > n_max {
                n_max = self.game_tree[*child].n;
                n_max_child = *child;
            }
        }

        return n_max_child;
    }
}
