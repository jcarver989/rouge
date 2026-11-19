set shell := ["bash", "-euo", "pipefail", "-c"]
set working-directory := ".."

# Type-check the browser target
check:
    cargo check --locked --target wasm32-unknown-unknown -p rouge --tests --all-features
    cargo check --locked --target wasm32-unknown-unknown -p rouge --example component_gallery

# Run Clippy on the browser target, failing on any warning
lint:
    cargo clippy --locked --target wasm32-unknown-unknown --workspace --lib --all-features -- -D warnings
    cargo clippy --locked --target wasm32-unknown-unknown -p rouge --tests --all-features -- -D warnings
    cargo clippy --locked --target wasm32-unknown-unknown -p rouge --example component_gallery -- -D warnings

# Run the browser tests in headless Chrome (needs wasm-bindgen-test-runner matching Cargo.lock and a matching chromedriver)
test:
    CHROMEDRIVER="$(command -v chromedriver)" CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --locked --target wasm32-unknown-unknown -p rouge --test platform_web_test --test platform_web_host_test --test platform_web_host_webgl_test

# Build an example for the browser into target/web/<example> (needs wasm-bindgen; uses wasm-opt when installed)
build example="component_gallery" package="rouge":
    cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p {{package}} --example {{example}}
    wasm-bindgen --target web --no-typescript --out-dir target/web/{{example}} target/wasm32-unknown-unknown/wasm-release/examples/{{example}}.wasm
    if command -v wasm-opt >/dev/null; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue target/web/{{example}}/{{example}}_bg.wasm -o target/web/{{example}}/{{example}}_bg.wasm; fi
    sed 's/MODULE/{{example}}/g' web/index.html > target/web/{{example}}/index.html

# Serve an example on http://localhost:8000; add ?rouge_bench=scroll:4:300 or ?rouge_profile to profile it
[private]
run example="component_gallery" package="rouge": (build example package)
    python3 -m http.server 8000 --bind 127.0.0.1 -d target/web/{{example}}
