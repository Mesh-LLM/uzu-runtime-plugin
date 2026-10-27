build:
    cargo build --release
verify:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test
clean:
    cargo clean
