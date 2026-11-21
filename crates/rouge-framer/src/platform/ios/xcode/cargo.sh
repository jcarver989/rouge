set -eu
PATH="$HOME/.cargo/bin:$PATH"
case "$PLATFORM_NAME" in
    iphonesimulator) triple=aarch64-apple-ios-sim ;;
    *) triple=aarch64-apple-ios ;;
esac
if [ "$CONFIGURATION" = Release ]; then
    profile=release
    set -- --release
else
    profile=debug
    set --
fi
cd "$SRCROOT/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.rouge/out/cargo}"
export CARGO_PROFILE_RELEASE_DEBUG=line-tables-only
export CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO=packed
export CARGO_PROFILE_RELEASE_STRIP=none
cargo build --locked "$@"@@PACKAGE@@ --bin "$EXECUTABLE_NAME" --target "$triple"
install -m 755 "$CARGO_TARGET_DIR/$triple/$profile/$EXECUTABLE_NAME" "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
cp "@@ICON@@" "$SRCROOT/Assets.xcassets/AppIcon.appiconset/icon.png"
