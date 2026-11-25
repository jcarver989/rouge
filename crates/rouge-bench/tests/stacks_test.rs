#![cfg(not(target_arch = "wasm32"))]

use rouge_bench::stacks::{Stacks, simplify};

const MARKER: &str = "measured_frame";

#[test]
fn only_what_runs_inside_the_marker_counts() {
    let stacks = StacksBuilder::default()
        .stack("main;measured_frame;app::paint", 3.0)
        .stack("main;app::setup", 9.0)
        .build();
    assert_eq!(stacks.total(), 3.0);
    assert_eq!(stacks.own(), [("app::paint".to_owned(), 3.0)]);
}

#[test]
fn plumbing_counts_towards_its_caller() {
    let stacks = StacksBuilder::default()
        .stack("measured_frame;app::paint;core::ptr::write::<u8>", 1.0)
        .stack("measured_frame;app::paint;<[u8]>::len", 1.0)
        .stack("measured_frame;app::paint;malloc", 1.0)
        .build();
    let own = [("app::paint".to_owned(), 2.0), ("malloc".to_owned(), 1.0)];
    assert_eq!(stacks.own(), own);
}

#[test]
fn recursion_folds_into_its_first_call() {
    let stacks = StacksBuilder::default()
        .stack(
            "measured_frame;app::layout;app::flex;app::layout;app::text",
            2.0,
        )
        .build();
    assert_eq!(stacks.folded(), "app::layout;app::text 2\n");
}

#[test]
fn a_tree_joins_chains_and_hides_small_branches() {
    let stacks = StacksBuilder::default()
        .stack("measured_frame;app::step;app::paint;app::text", 60.0)
        .stack("measured_frame;app::step;app::paint;app::quad", 39.0)
        .stack("measured_frame;app::step;app::paint;app::rare", 1.0)
        .build();
    let tree = stacks.tree(0.02, |weight| format!("{weight:>3}"));
    assert_eq!(
        tree,
        "100  app::step > app::paint\n 60    app::text\n 39    app::quad\n"
    );
}

#[test]
fn under_starts_at_the_named_function() {
    let stacks = StacksBuilder::default()
        .stack("measured_frame;app::step;app::hit_test;app::rect", 2.0)
        .stack("measured_frame;app::step;app::paint", 5.0)
        .build();
    let under = stacks.under("hit_test");
    assert_eq!(under.folded(), "app::hit_test;app::rect 2\n");
}

#[test]
fn names_lose_generics_traits_hashes_and_closures() {
    let simple = [
        "<rouge_core::tree::Tree>::paint",
        "<rouge_core::ui::Ui as core::ops::Drop>::drop",
        "rouge_core::tree::paint::paint::<F>::{closure#0}",
        "rouge_core::paint::h0123456789abcdef",
        "app::apply::<fn(u8) -> u8, Vec<u8>>",
    ]
    .map(simplify);
    assert_eq!(
        simple,
        [
            "rouge_core::Tree::paint",
            "rouge_core::Ui::drop",
            "rouge_core::paint::paint",
            "rouge_core::paint",
            "app::apply",
        ]
    );
}

#[derive(Default)]
struct StacksBuilder {
    stacks: Stacks,
}

impl StacksBuilder {
    fn stack(mut self, path: &str, weight: f64) -> Self {
        let names = path.split(';').map(str::to_owned);
        self.stacks.add(names, MARKER, weight);
        self
    }

    fn build(self) -> Stacks {
        self.stacks
    }
}
