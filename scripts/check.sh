#!/usr/bin/env bash
# Full gate: formatting, lints and the complete test suite.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets "$@"
