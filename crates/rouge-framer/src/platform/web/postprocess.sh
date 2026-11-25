set -eu
cd "$1"
wasm="${2}_bg.wasm"
if command -v wasm-opt >/dev/null; then
    wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue "$wasm" -o "$wasm"
fi
if command -v brotli >/dev/null; then
    brotli --best --keep --force "$wasm" "$2.js" index.html
else
    rm -f "$wasm.br" "$2.js.br" index.html.br
fi
