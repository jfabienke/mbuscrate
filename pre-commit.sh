#!/bin/bash

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test