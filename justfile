test:
    cargo clippy
    cargo test

build:
    cargo build --release

fmt:
    cargo fmt
    dprint fmt

# The feature sets CI builds one at a time (each replaces the default set)
feature_sets := '"" zstd watch hex alt_flags extra_id'

# Lint every feature set the way CI does
allclippy:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo clippy --all-targets --all-features -- -D warnings
    for f in {{feature_sets}}; do
        cargo clippy --all-targets --no-default-features --features "$f" -- -D warnings
    done

# Everything CI checks: formatting, lints and tests for every feature set, plus the tests in
# release mode the way the release workflow runs them (debug! logging is compiled out there)
ci: allclippy
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --all -- --check
    dprint check
    cargo test --all-features
    for f in {{feature_sets}}; do
        cargo test --no-default-features --features "$f"
    done
    cargo test --release
