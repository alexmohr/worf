#!/usr/bin/env bash
set -euo pipefail

# Publish worf as both library and binary
cd worf

echo "Publishing worf (lib + bin) ..."
cargo publish "$@"
cd ..

# Publish each example as binary only
for crate in examples/worf-hyprswitch examples/worf-hyprspace examples/worf-warden; do
  echo "Publishing $crate (bin only) ..."
  cd "$crate"
  cargo publish "$@" 
  cd - > /dev/null
  sleep 10 # Give crates.io time to update index
  echo
  echo "---"
done

echo "All crates published!"
