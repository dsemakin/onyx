# One entry point. If `just ci` passes, the change is ready to review.
# Contributors and coding agents should not need to know any other command.

default: ci

# Everything the hermetic CI jobs run, in the same order. No network, no clock.
ci: fmt-check lint test links vendored reference-reader package

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Includes the corpus runner, which lives in crates/onyx-core/tests/corpus.rs.
test:
    cargo test --workspace --all-features

# Every copy cargo forces us to keep — the manifests and the licence texts — still
# matches its original.
vendored:
    node scripts/vendor.mjs --check

# Every relative link in the Markdown resolves. Catches a moved file before a reader does.
links:
    node scripts/check-links.mjs

# Just the corpus, when that is all you changed.
corpus:
    cargo test --package onyx-core --test corpus

# Checks the corpus against the JSON Schema. Kept out of `ci` because it installs Ajv
# from the network; the project itself has no npm dependencies.
schema:
    npm install --no-save --no-package-lock --silent --no-audit --no-fund ajv@8
    node scripts/check-schema.mjs

# Runs the conformance corpus against the standard-library Python reader, which shares
# no code with the engine. Needs python3; no packages.
reference-reader:
    python3 examples/reader-py/run_corpus.py

# What a published crate would contain, and whether it compiles standing alone. Catches a
# crate reaching outside its own directory, which works in a checkout and fails once
# published.
package:
    cargo package --locked --package onyx-core

# The fuzz targets compile against the current engine API. They live outside the
# workspace, so `just ci` cannot reach them; this is what stops them bit-rotting.
fuzz-check:
    rustup toolchain install nightly --profile minimal
    cargo +nightly check --manifest-path fuzz/Cargo.toml

# Build the wasm module consumed by packages/npm. Plain cargo: no wasm-pack, no
# wasm-bindgen, nothing to keep in version step with anything else.
wasm:
    rustup target add wasm32-unknown-unknown
    cargo build --locked --release --package onyx-wasm --target wasm32-unknown-unknown
    mkdir -p packages/npm/pkg
    cp target/wasm32-unknown-unknown/release/onyx_wasm.wasm packages/npm/pkg/onyx.wasm

# Runs the corpus through the wasm binding, proving the marshalling works and not only
# the engine. Builds the wasm first.
npm-test: wasm
    cd packages/npm && npm test

audit:
    cargo audit
