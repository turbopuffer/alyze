#!/usr/bin/env bash
# Builds the alyze WebAssembly package (web target).
#
# Output lands in `wasm/pkg/`. The turbopuffer docs site vendors a copy of these
# artifacts; after building, sync them into the site (see the site's
# `src/wasm/alyze/` directory).
#
# Requirements:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-pack   (or: brew install wasm-pack)
set -euo pipefail

cd "$(dirname "$0")"

wasm-pack build \
  --target web \
  --release \
  --out-name alyze \
  --out-dir pkg

echo "Built wasm package in $(pwd)/pkg"
