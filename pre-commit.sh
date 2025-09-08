# Pre-commit Hooks

To ensure code quality, run these before committing:

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test