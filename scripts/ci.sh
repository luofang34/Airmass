#!/bin/sh
set -eu

./scripts/check-doc-boundary.sh
./scripts/check-domain-boundary.sh
./scripts/check-lint-policy.sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" \
    cargo doc --locked --workspace --all-features --no-deps
cargo build --locked --workspace --all-features --release
./scripts/check-no-std.sh
