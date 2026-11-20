set -euo pipefail
cd "$(dirname "$0")/.."

die() { printf '%s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "Install $1 and put it on PATH"; }
secret() { [[ -n ${!1:-} ]] || die "Missing $1; see PUBLISHING.md for where to set it"; }

action=${1:-}
target=${2:-}
case "$action" in setup|doctor|build|upload) ;; *) die 'Usage: bash .rouge/package.sh {setup|doctor|build|upload} {web|linux|macos|ios|android}' ;; esac
case "$target" in web|linux|macos|ios|android) ;; *) die 'Choose web, linux, macos, ios, or android' ;; esac
need cargo
need python3
metadata=$(python3 .rouge/metadata.py metadata "$target")
eval "$metadata"
export APP_ID APP_NAME APP_ICON APP_VERSION APP_TARGET
channel=${ROUGE_CHANNEL:-check}
case "$channel" in check|preview|production) ;; *) die 'Invalid ROUGE_CHANNEL' ;; esac
build_number=${ROUGE_BUILD_NUMBER:-${GITHUB_RUN_NUMBER:-1}}
[[ $build_number =~ ^[1-9][0-9]{0,8}$ ]] || die 'Build number must be a positive integer of at most 9 digits'
export ROUGE_BUILD_NUMBER="$build_number"
if [[ -n $APP_ICON ]]; then [[ -f $APP_ICON ]] || die "Icon does not exist: $APP_ICON"; fi
out="$PWD/.rouge/out/$target"
cargo_out="$PWD/.rouge/out/cargo"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$cargo_out}"

build_binary() {
    cargo build --locked --release --package "$APP_PACKAGE" --bin "$APP_TARGET" --target "$1"
}

build_web() {
    need rustup
    rustup target add wasm32-unknown-unknown
    version=$(python3 .rouge/metadata.py bindgen-version "$CARGO_LOCK")
    local bindgen=wasm-bindgen
    if [[ $(wasm-bindgen --version 2>/dev/null || true) != "wasm-bindgen $version" ]]; then
        local tools="$PWD/.rouge/out/tools/wasm-bindgen-$version"
        bindgen="$tools/bin/wasm-bindgen"
        [[ -x $bindgen ]] || cargo install wasm-bindgen-cli --version "=$version" --locked --root "$tools"
    fi
    build_binary wasm32-unknown-unknown
    "$bindgen" --target web --no-typescript --out-name app --out-dir "$out" \
        "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/$APP_TARGET.wasm"
    python3 .rouge/metadata.py web-page "$out/index.html"
    [[ -z $APP_ICON ]] || cp "$APP_ICON" "$out/icon.png"
}

build_linux() {
    [[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || die 'Use an x86-64 Linux host'
    local triple=x86_64-unknown-linux-gnu
    local stem="$APP_TARGET-$APP_VERSION-linux-x86_64"
    local staging="$out/$stem"
    build_binary "$triple"
    mkdir -p "$staging"
    install -m 755 "$CARGO_TARGET_DIR/$triple/release/$APP_TARGET" "$staging/$APP_TARGET"
    printf 'Launch ./%s. Requires a graphical Wayland or X11 session, a working GPU driver, and Fontconfig.\n' "$APP_TARGET" > "$staging/README.txt"
    tar -C "$out" -czf "$out/$stem.tar.gz" "$stem"
    rm -r "$staging"
}

case "$target" in
    macos|ios) [[ -f .rouge/apple.sh ]] || die "$target is not enabled"; source .rouge/apple.sh ;;
    android) [[ -f .rouge/android.sh ]] || die 'Android is not enabled'; source .rouge/android.sh ;;
esac

if [[ $action == setup ]]; then
    [[ $target == android ]] || die 'setup is only available for Android'
    install_android
    exit
fi

if [[ $action == upload ]]; then
    [[ $channel != check ]] || die 'Set ROUGE_CHANNEL to preview or production before uploading'
    case "$target" in ios) upload_ios ;; *) die 'This target is published by GitHub Actions' ;; esac
    exit
fi

case "$target" in
    macos|ios) doctor_apple ;;
    android) doctor_android ;;
esac
[[ $action != doctor ]] || exit 0
rm -rf "$out"
mkdir -p "$out"
case "$target" in
    web) build_web ;;
    linux) build_linux ;;
    macos) build_macos ;;
    ios) build_ios ;;
    android) build_android ;;
esac
printf 'Packages: %s\n' "$out"
