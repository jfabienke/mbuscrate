# Repository Guidelines

## Project Structure & Module Organization
- `src/lib.rs`: Library API surface for M‑Bus and wM‑Bus.
- `src/main.rs`: Runnable example/binary entry point.
- Modules: `src/mbus/`, `src/wmbus/`, `src/payload/`, plus helpers in `src/logging.rs`, `src/error.rs`, `src/constants.rs`.
- `tests/`: Integration and unit tests (e.g., `frame_tests.rs`, `lib_tests.rs`).
- `src/AI/`: Design notes and prompts (non-code; do not ship).

## Build, Test, and Development Commands
- Build: `cargo build` (use `--release` for optimized builds).
- Run binary: `RUST_LOG=info cargo run` (set serial port like `/dev/ttyUSB0` in code/args).
- Tests: `cargo test` (use `-- --nocapture` to see log output).
- Format: `cargo fmt --all` (check only: `cargo fmt --all -- --check`).
- Lint: `cargo clippy --all-targets -- -D warnings`.

## Coding Style & Naming Conventions
- Rustfmt is the source of truth; 4‑space indentation, 100‑120 col soft wrap.
- Names: types/enums `CamelCase`, functions/modules/files `snake_case`, constants `SCREAMING_SNAKE_CASE`.
- Error handling uses `thiserror` in `src/error.rs`; prefer specific variants over `Other`.
- Logging via `log`/`env_logger`; use `log_debug/info/warn/error` helpers.

## Testing Guidelines
- Framework: `cargo test` with async tests on `tokio` where needed.
- Location: integration tests in `tests/*_tests.rs`; unit tests can live in `#[cfg(test)]` modules.
- Expectations: add tests for new parsing/encoding and device‑manager behaviors; keep them deterministic (no real serial I/O).

## Commit & Pull Request Guidelines
- Commits: concise, imperative subject (optionally Conventional Commits, e.g., `feat: add long frame packing`).
- PRs must include: clear description, rationale, linked issue (if any), tests for new/changed behavior, and docs updates (README or rustdoc) when APIs change.
- Add screenshots or logs only when debugging; avoid leaking device IDs/keys.

## Security & Configuration Tips
- Do not commit secrets or device credentials. Avoid hard‑coding `/dev/tty*`; make it configurable.
- Use `RUST_LOG=debug` locally for troubleshooting; default to `info` in docs/examples.
