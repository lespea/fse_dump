test:
    cargo clippy
    cargo test

build:
    cargo build --release

fmt:
    cargo fmt
    dprint fmt

allclippy:
    cargo clippy --no-default-features --features=default
    cargo clippy --no-default-features --features=zstd
    cargo clippy --no-default-features --features=watch
    cargo clippy --no-default-features --features=hex
    cargo clippy --no-default-features --features=alt_flags
    cargo clippy --no-default-features --features=extra_id
