use std::collections::BTreeMap;
use std::fmt::Write;

const PRIMITIVES: [&str; 13] = [
    "bool", "char", "f32", "f64", "i32", "i64", "isize", "str", "u16", "u32", "u64", "u8", "usize",
];
const PLUMBING: [&str; 8] = [
    "core",
    "alloc",
    "std",
    "hashbrown",
    "backtrace",
    "__rustc",
    "FnOnce",
    "compiler_builtins",
];
const WHOLE: f64 = 0.97;
const CHAIN: usize = 4;

#[derive(Clone, Debug, Default)]
pub struct Stacks {
    stacks: BTreeMap<Vec<String>, f64>,
}

#[derive(Default)]
struct Node {
    weight: f64,
    children: BTreeMap<String, Node>,
}

impl Stacks {
    pub fn add(
        &mut self,
        outermost_first: impl IntoIterator<Item = String>,
        marker: &str,
        weight: f64,
    ) {
        let mut inside = false;
        let mut stack: Vec<String> = Vec::new();
        for name in outermost_first {
            if !inside {
                inside = name.contains(marker);
                continue;
            }
            let name = simplify(&name);
            let crate_name = name.split("::").next().unwrap_or_default();
            let plumbing = PLUMBING.contains(&crate_name) || PRIMITIVES.contains(&crate_name);
            if name.is_empty() || name.starts_with(['[', '(']) || plumbing {
                continue;
            }
            if let Some(index) = stack.iter().position(|known| *known == name) {
                stack.truncate(index + 1);
            } else {
                stack.push(name);
            }
        }
        if inside {
            *self.stacks.entry(stack).or_default() += weight;
        }
    }

    pub fn scaled(mut self, factor: f64) -> Self {
        for weight in self.stacks.values_mut() {
            *weight *= factor;
        }
        self
    }

    pub fn under(&self, part: &str) -> Stacks {
        let mut under = Stacks::default();
        for (stack, weight) in &self.stacks {
            if let Some(index) = stack.iter().position(|name| name.contains(part)) {
                *under.stacks.entry(stack[index..].to_vec()).or_default() += weight;
            }
        }
        under
    }

    pub fn total(&self) -> f64 {
        self.stacks.values().sum()
    }

    pub fn own(&self) -> Vec<(String, f64)> {
        let mut own: BTreeMap<&str, f64> = BTreeMap::new();
        for (stack, weight) in &self.stacks {
            let leaf = stack.last().map_or("(frame)", String::as_str);
            *own.entry(leaf).or_default() += weight;
        }
        let mut own: Vec<(String, f64)> = own
            .into_iter()
            .map(|(name, weight)| (name.to_owned(), weight))
            .collect();
        own.sort_by(|a, b| b.1.total_cmp(&a.1));
        own
    }

    pub fn tree(&self, least_share: f64, show: impl Fn(f64) -> String) -> String {
        let mut root = Node::default();
        for (stack, weight) in &self.stacks {
            let mut node = &mut root;
            node.weight += weight;
            for name in stack {
                node = node.children.entry(name.clone()).or_default();
                node.weight += weight;
            }
        }
        let mut out = String::new();
        let total = root.weight;
        if total > 0.0 {
            root.print(0, total * least_share, &show, &mut out);
        }
        out
    }

    pub fn folded(&self) -> String {
        let mut out = String::new();
        for (stack, weight) in &self.stacks {
            let path = if stack.is_empty() {
                "(frame)".to_owned()
            } else {
                stack.join(";").replace(' ', "_")
            };
            let _ = writeln!(out, "{path} {}", weight.round().max(1.0));
        }
        out
    }
}

impl Node {
    fn print(&self, indent: usize, least: f64, show: &impl Fn(f64) -> String, out: &mut String) {
        for (name, node) in self.largest(least) {
            let mut chain = vec![name];
            let mut end = node;
            while let [(name, only)] = end.largest(least)[..]
                && only.weight >= node.weight * WHOLE
            {
                chain.push(name);
                end = only;
            }
            if chain.len() > CHAIN {
                chain.splice(1..chain.len() - 2, ["..."]);
            }
            let pad = "  ".repeat(indent);
            let _ = writeln!(out, "{}  {pad}{}", show(node.weight), chain.join(" > "));
            end.print(indent + 1, least, show, out);
        }
    }

    fn largest(&self, least: f64) -> Vec<(&str, &Node)> {
        let children = self.children.iter();
        let mut children: Vec<(&str, &Node)> = children
            .filter(|(_, node)| node.weight >= least)
            .map(|(name, node)| (name.as_str(), node))
            .collect();
        children.sort_by(|a, b| b.1.weight.total_cmp(&a.1.weight));
        children
    }
}

pub fn simplify(name: &str) -> String {
    let name = name.trim();
    let (head, rest) = match name.strip_prefix('<').and_then(matched) {
        Some((inner, rest)) => (simplify(implementor(inner)), rest),
        None => (String::new(), name),
    };
    let mut plain = head;
    let mut depth = 0_usize;
    let mut previous = ' ';
    for c in rest.chars() {
        match c {
            '<' => depth += 1,
            '>' if previous != '-' => depth = depth.saturating_sub(1),
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
        previous = c;
    }
    let mut parts: Vec<&str> = plain
        .split("::")
        .filter(|part| !part.is_empty() && !part.starts_with('{'))
        .collect();
    if let Some(hash) = parts.last()
        && hash.len() == 17
        && hash.starts_with('h')
        && hash[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        parts.pop();
    }
    if parts.len() > 3 {
        parts.drain(1..parts.len() - 2);
    }
    parts.join("::")
}

fn implementor(inner: &str) -> &str {
    let mut depth = 0_usize;
    let mut end = inner.len();
    for (index, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 && inner[index..].starts_with(" as ") => {
                end = index;
                break;
            }
            _ => {}
        }
    }
    let mut plain = &inner[..end];
    for prefix in ["&", "mut ", "dyn ", "*const ", "*mut "] {
        plain = plain.strip_prefix(prefix).unwrap_or(plain);
    }
    plain
}

fn matched(rest: &str) -> Option<(&str, &str)> {
    let mut depth = 1_usize;
    let mut previous = ' ';
    for (index, c) in rest.char_indices() {
        match c {
            '<' => depth += 1,
            '>' if previous != '-' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some((&rest[..index], &rest[index + 1..]));
        }
        previous = c;
    }
    None
}
