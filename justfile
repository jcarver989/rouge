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

# Run Clippy, failing on any rustc, Clippy or Cargo warning
lint:
    CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --all-features

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
    cargo check --locked --target wasm32-unknown-unknown -p rouge --example component_gallery

# Run Clippy on the browser target, failing on any warning
web-lint:
    cargo clippy --locked --target wasm32-unknown-unknown --workspace --lib --all-features -- -D warnings
    cargo clippy --locked --target wasm32-unknown-unknown -p rouge --tests --all-features -- -D warnings
    cargo clippy --locked --target wasm32-unknown-unknown -p rouge --example component_gallery -- -D warnings

# Run the browser tests in headless Chrome (needs wasm-bindgen-test-runner matching Cargo.lock and a matching chromedriver)
web-test:
    CHROMEDRIVER="$(command -v chromedriver)" CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --locked --target wasm32-unknown-unknown -p rouge --test platform_web_test --test platform_web_host_test

# Build an example for the browser into target/web/<example> (needs wasm-bindgen; uses wasm-opt when installed)
web-build example="component_gallery" package="rouge":
    cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p {{package}} --example {{example}}
    wasm-bindgen --target web --no-typescript --out-dir target/web/{{example}} target/wasm32-unknown-unknown/wasm-release/examples/{{example}}.wasm
    if command -v wasm-opt >/dev/null; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue target/web/{{example}}/{{example}}_bg.wasm -o target/web/{{example}}/{{example}}_bg.wasm; fi
    sed 's/MODULE/{{example}}/g' web/index.html > target/web/{{example}}/index.html

# Serve an example on http://localhost:8000; add ?rouge_bench=scroll:4:300 or ?rouge_profile to profile it
web example="component_gallery" package="rouge": (web-build example package)
    python3 -m http.server 8000 --bind 127.0.0.1 -d target/web/{{example}}

# Measure the bench scenarios on a headless GPU and print what changed since `just bench --save`; arguments are name fragments, --save or --runs <n>
bench *args:
    cargo run --release -p rouge-bench -- run {{args}}

# Sample one bench scenario and print its hottest functions (macOS: sample; Linux: perf)
profile scenario seconds="10":
    scripts/profile {{scenario}} {{seconds}}

# Write a dhat heap profile of one bench scenario to target/bench/dhat-heap.json
bench-heap scenario:
    cargo run --release -p rouge-bench --features dhat -- run {{scenario}} --runs 0

# Keep widget spacing on the 4px grid and radii on the token scale
style-check:
    scripts/style-check

# Type-check the iOS targets (needs full Xcode selected with xcode-select, plus the aarch64-apple-ios and aarch64-apple-ios-sim targets)
ios-check:
    cargo check --locked --target aarch64-apple-ios -p rouge --all-features
    cargo check --locked --target aarch64-apple-ios-sim -p rouge --tests --all-features
    cargo check --locked --target aarch64-apple-ios-sim -p rouge --example component_gallery

# Bundle an example into target/ios/<example>.app for the iOS simulator
ios-build example="component_gallery" package="rouge":
    cargo build --locked --target aarch64-apple-ios-sim -p {{package}} --example {{example}}
    rm -rf target/ios/{{example}}.app
    mkdir -p target/ios/{{example}}.app
    cp target/aarch64-apple-ios-sim/debug/examples/{{example}} target/ios/{{example}}.app/
    sed -e 's/EXECUTABLE/{{example}}/g' -e "s/BUNDLE_ID/dev.rouge.$(echo {{example}} | tr _ -)/g" ios/Info.plist > target/ios/{{example}}.app/Info.plist

# Run an example in the iOS simulator and stream its output
ios example="component_gallery" package="rouge" device="iPhone 17 Pro": (ios-build example package)
    xcrun simctl boot "{{device}}" 2>/dev/null || true
    open -a Simulator
    xcrun simctl install "{{device}}" target/ios/{{example}}.app
    xcrun simctl launch --console-pty --terminate-running-process "{{device}}" "dev.rouge.$(echo {{example}} | tr _ -)"

# Type-check the Android target (needs the aarch64-linux-android target, the Android NDK and cargo-ndk)
android-check:
    cargo ndk -t arm64-v8a check --locked -p rouge --all-features
    cargo ndk -t arm64-v8a check --locked -p rouge --tests --all-features
    cargo ndk -t arm64-v8a check --locked -p rouge --example android_todomvc

# Package the Android example into target/android/<example>/<example>.apk (needs ANDROID_HOME with build-tools and a platform, and a Java runtime on PATH for keytool and apksigner)
android-build example="android_todomvc" package="rouge":
    cargo ndk -t arm64-v8a build --locked -p {{package}} --example {{example}}
    scripts/android-apk {{example}} "dev.rouge.$(echo {{example}} | tr - _)"

# Install the Android example on the connected device or emulator, start it and stream its log
android example="android_todomvc" package="rouge": (android-build example package)
    adb install -r target/android/{{example}}/{{example}}.apk
    adb shell am start -n "dev.rouge.$(echo {{example}} | tr - _)/android.app.NativeActivity"
    adb logcat -v color RustStdoutStderr:V AndroidRuntime:E '*:S'

