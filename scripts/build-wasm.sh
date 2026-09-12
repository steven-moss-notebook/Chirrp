#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
binding_tool="${WASM_BINDGEN:-wasm-bindgen}"
expected="0.2.127"
if ! command -v "$binding_tool" >/dev/null || [[ "$("$binding_tool" --version)" != "wasm-bindgen $expected" ]]; then
  echo "Install matching bindings: cargo install wasm-bindgen-cli --version $expected --locked" >&2
  echo "Or set WASM_BINDGEN to the matching executable." >&2
  exit 1
fi
cargo build --locked --release --no-default-features --target wasm32-unknown-unknown --lib
"$binding_tool" --target web --out-dir wasm/pkg --out-name chirrp target/wasm32-unknown-unknown/release/chirrp.wasm
echo "Built wasm/pkg: JavaScript, TypeScript declarations, and the WASM library."
