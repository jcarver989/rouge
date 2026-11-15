# framer:begin rust-monorepo
set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Type-check every crate and target
check:
    cargo check --workspace --all-targets --all-features

# Run tests with nextest, then doctests, which nextest doesn't run
test profile="default":
    cargo nextest run --profile {{profile}} --workspace --all-features --no-tests pass
    cargo test --doc --workspace --all-features

# Run tests with nextest's CI profile, which writes JUnit results
test-ci: (test "ci")

# Run Clippy, failing on any warning
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Build docs, failing on any warning
doc-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items

# Regenerate files from .framer/
framer:
    cargo run --quiet --manifest-path .framer/Cargo.toml

# Check generated files are up to date
framer-check:
    cargo run --quiet --locked --manifest-path .framer/Cargo.toml -- --check
# framer:end rust-monorepo

# Type-check the browser target
web-check:
    cargo check --locked --target wasm32-unknown-unknown -p rouge --tests --all-features
    cargo check --locked --target wasm32-unknown-unknown -p rouge-widgets --example component_gallery

# Run the browser tests in headless Chrome (needs wasm-bindgen-test-runner matching Cargo.lock and a matching chromedriver)
web-test:
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --locked --target wasm32-unknown-unknown -p rouge --test platform_web_test --test platform_web_runtime_test --test platform_web_host_test

# Build an example for the browser into target/web/<example> (needs wasm-bindgen; uses wasm-opt when installed)
web-build example="component_gallery" package="rouge-widgets":
    cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p {{package}} --example {{example}}
    wasm-bindgen --target web --no-typescript --out-dir target/web/{{example}} target/wasm32-unknown-unknown/wasm-release/examples/{{example}}.wasm
    if command -v wasm-opt >/dev/null; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue target/web/{{example}}/{{example}}_bg.wasm -o target/web/{{example}}/{{example}}_bg.wasm; fi
    sed 's/MODULE/{{example}}/g' web/index.html > target/web/{{example}}/index.html

# Serve an example on http://localhost:8000; add ?rouge_bench=scroll:4:300 or ?rouge_profile to profile it
web example="component_gallery" package="rouge-widgets": (web-build example package)
    python3 -m http.server 8000 --bind 127.0.0.1 -d target/web/{{example}}

# Keep widget spacing on the 4px grid and radii on the token scale
style-check:
    scripts/style-check
