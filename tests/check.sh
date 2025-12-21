#!/bin/bash

set -e

echo "Running cargo check..."
cargo check

echo "Running cargo fmt --check..."
cargo fmt --check

echo "Running cargo clippy..."
cargo clippy -- -D warnings

echo "Running cargo test..."
cargo test

echo "Running cargo test with --release..."
cargo test --release

echo "All checks passed!"