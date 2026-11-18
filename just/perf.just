set shell := ["bash", "-euo", "pipefail", "-c"]
set working-directory := ".."

# Measure the bench scenarios on a headless GPU and print what changed since `just perf::bench --save`; arguments are name fragments, --save or --runs <n>
[positional-arguments]
bench *args:
    cargo run --release -p rouge-bench -- run "$@"

# Sample one bench scenario and print its hottest functions (macOS: sample; Linux: perf)
profile scenario seconds="10":
    scripts/profile {{scenario}} {{seconds}}

# Write a dhat heap profile of one bench scenario to target/bench/dhat-heap.json
heap scenario:
    cargo run --release -p rouge-bench --features dhat -- run {{scenario}} --runs 0
