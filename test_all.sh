#!/bin/bash
set -e

cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D clippy::all -D clippy::default_numeric_fallback
cargo doc --no-deps

cargo test --workspace --all-targets --all-features
cargo test --workspace --all-features --doc

echo "### all tests passed! ###"
