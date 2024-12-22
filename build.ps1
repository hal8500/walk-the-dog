wasm-pack build --target web --out-dir web/pkg

# リリース用 panicフックを無効化
# wasm-pack build --no-default-features --target web --out-dir web/pkg