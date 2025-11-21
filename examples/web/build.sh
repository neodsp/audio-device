#!/bin/bash
set -e

# Build the WASM package
wasm-pack build --target web --out-dir pkg

echo ""
echo "Build complete! To run:"
echo "  cd $(dirname $0)"
echo "  python3 -m http.server 8080"
echo "  # Then open http://localhost:8080"
