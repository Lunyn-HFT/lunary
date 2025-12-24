#!/bin/bash

set -e

echo "Running cargo check..."
cargo check --all-targets --all-features

echo "Running cargo fmt --check..."
cargo fmt --all --check

echo "Running cargo clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "Running cargo test..."
cargo test --all-features

echo "Running cargo test with --release..."
cargo test --all-features --release

echo "Running cargo doc..."
cargo doc --all-features --no-deps

echo "All checks passed!"