set shell := ["bash", "-euo", "pipefail", "-c"]
set working-directory := ".."

# Type-check the iOS targets (needs full Xcode selected with xcode-select, plus the aarch64-apple-ios and aarch64-apple-ios-sim targets)
check:
    cargo check --locked --target aarch64-apple-ios -p rouge --all-features
    cargo check --locked --target aarch64-apple-ios-sim -p rouge --tests --all-features
    cargo check --locked --target aarch64-apple-ios-sim -p rouge --example component_gallery

# Bundle an example into target/ios/<example>.app for the iOS simulator
build example="component_gallery" package="rouge":
    cargo build --locked --target aarch64-apple-ios-sim -p {{package}} --example {{example}}
    rm -rf target/ios/{{example}}.app
    mkdir -p target/ios/{{example}}.app
    cp target/aarch64-apple-ios-sim/debug/examples/{{example}} target/ios/{{example}}.app/
    sed -e 's/EXECUTABLE/{{example}}/g' -e "s/BUNDLE_ID/dev.rouge.$(echo {{example}} | tr _ -)/g" ios/Info.plist > target/ios/{{example}}.app/Info.plist

# Run an example in the iOS simulator and stream its output
[private]
run example="component_gallery" package="rouge" $device=env("ROUGE_IOS_DEVICE", "iPhone 17 Pro"): (build example package)
    xcrun simctl boot "$device" 2>/dev/null || true
    open -a Simulator
    xcrun simctl install "$device" target/ios/{{example}}.app
    xcrun simctl launch --console-pty --terminate-running-process "$device" "dev.rouge.$(echo {{example}} | tr _ -)"
