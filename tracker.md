# Implementation Tracker for mbus-rs Industrial-Grade Upgrade

This tracker outlines the phased plan to transform mbus-rs into an industrial-grade crate. Use checkboxes to mark completion. Estimated timeline: 3-6 months. Start with Phase 1 immediate steps.

## Phase 1: Foundation Building (1-2 weeks)
Focus: Complete core functionality, eliminate gaps, ensure basic quality.

### Tasks
- [x] **Complete Parsing Logic**
  - [x] Implement full multi-record handling in variable parsing (e.g., loop until end in `parse_records`)
  - [x] Port complete VIFE/FD/FB tables from libmbus to `vif_maps.rs`
  - [x] Test VIFE parsing with edge cases (e.g., extensions beyond 0xFF)
  - [x] Add const generics for frame sizes in `frame.rs` to prevent runtime errors
  - [x] Use visitor pattern for VIF decoding in `vif.rs` if complex
- [x] **Enhance Error Handling**
  - [x] Add specific error variants (e.g., `InvalidTimestamp`, `Overflow`) in `error.rs`
  - [x] Ensure all parsing functions return detailed errors with context
  - [x] Update error strings for clarity (e.g., include field names)
  - [x] Add context to errors with `anyhow` chaining for user-facing messages
- [x] **Documentation**
  - [x] Add doc comments to all public APIs (functions, structs, enums)
  - [x] Expand README.md with architecture diagram, spec references (EN 13757), and advanced examples
  - [x] Generate and review `cargo doc` output; fix inconsistencies
  - [ ] Host generated docs on docs.rs
  - [x] Add comprehensive rustdoc examples for parsing/encoding

### Metrics
- [x] 100% compilation without warnings (run `cargo check`)
- [x] All non-hardware tests passing (run `cargo test --lib`)

### Immediate Next Steps (1-3 days)
- [x] Port full VIFE maps to `vif_maps.rs` (copy from libmbus C code)
- [x] Implement complete variable record loop in `record.rs` (handle multiple records per frame)
- [x] Run `cargo doc` and review; add missing docs to public items

## Phase 2: Testing and Validation (2-4 weeks)
Focus: Achieve high coverage and verify against real-world data.

### Tasks
- [x] **Unit/Integration Tests**
  - [x] Add golden frame tests (integrate 20+ .hex files from libmbus/test-frames into `golden_frames.rs`)
  - [x] Enable and expand error frame tests (from libmbus/test/error-frames)
  - [x] Add fuzzing for parsers using `cargo-fuzz` (target `parse_vib`, `parse_records`)
  - [x] Add property-based tests via `proptest` for VIF maps in `vif_tests.rs`
  - [x] Add tests for error paths and edge cases in encryption
- [x] **Coverage Tools**
  - [x] Install/use cargo-tarpaulin for coverage
  - [x] Run coverage analysis; identify and test low-coverage areas
  - [x] Achieve 80%+ line/function coverage
- [x] **Benchmarking**
  - [x] Add benchmarks using `criterion` (e.g., for parsing/serial I/O)
  - [x] Profile performance with `cargo flamegraph`; optimize hotspots
  - [x] Ensure <10ms for typical frame parsing
- [x] **Hardware Validation**
  - [x] Enable ignored tests with mock serial ports (e.g., via `tokio-serial` mocks)
  - [x] Add integration tests for real devices (if hardware available)
  - [x] Test on multiple platforms (Linux, macOS, Windows)
  - [x] Audit for panics in parsing and serial handling

### Metrics
- [x] 80% test coverage
- [x] All tests passing
- [x] Benchmarks show <10ms for typical operations

### Immediate Next Steps (continued from Phase 1)
- [x] Integrate golden frames (e.g., parse ACW_Itron-BM-plus-m.hex and compare to .norm.xml)
- [x] Run coverage: `cargo llvm-cov --lcov --output-path lcov.info` and review report

## Phase 3: Enhancement and Features (3-6 weeks)
Focus: Add advanced features for usability and performance.

## Phase 3: Enhancement and Features (3-6 weeks)
Focus: Add advanced features for usability and performance.

### Tasks
- [x] **Async Improvements**
  - [x] Ensure all I/O is non-blocking (review tokio usage)
  - [x] Add support for TCP M-Bus (implement `mbus-tcp` module)
  - [x] Handle timeouts and retries gracefully
  - [x] Migrate to async everywhere for scalability
- [x] **Security**
  - [x] Audit for buffer overflows and injection vulnerabilities
  - [x] Add input validation (e.g., max frame size limits)
  - [x] Support encrypted modes if in M-Bus spec
  - [x] Run `cargo-audit` for dependency vulnerabilities
  - [x] Ensure no logging of payloads with secrets
- [x] **Features**
  - [x] Add WMBus support (wireless M-Bus) in `wmbus/` module
  - [x] Add configurable options (e.g., baud rate, timeouts) via builder pattern
  - [x] Create a CLI tool for testing (e.g., `mbus-cli` binary) with `clap` for arg parsing
  - [x] Introduce protocol-agnostic interfaces for easier extension (e.g., traits in `wmbus_protocol.rs`)
- [x] **Performance**
  - [x] Profile with flamegraph and optimize (e.g., avoid allocations in hot paths)
  - [x] Support for large datasets (e.g., streaming parsing)
  - [x] Memory safety checks (run `cargo miri` if possible)
  - [x] Check for `bytes` crate usage in serialization if not already; add if beneficial

### Metrics
- [x] No critical vulnerabilities (run `cargo-audit`)
- [x] New features documented with examples
- [x] Performance benchmarks pass thresholds

## Phase 4: Quality Assurance and Release (2-4 weeks)
Focus: Polish for production and community.

### Tasks
- [ ] **Audits/Reviews**
  - [ ] Run `cargo-audit`, `clippy`, `rustfmt`
  - [ ] Perform external code review or security audit
  - [ ] Fix all lints and format issues
  - [ ] Set up pre-commit hooks for `cargo fmt --check` and `clippy`
  - [ ] Run `cargo udeps` for unused dependencies
  - [ ] Use selective `pub use` in `lib.rs` for cleaner API
- [ ] **CI/CD**
  - [ ] Set up GitHub Actions for tests, coverage, releases
  - [ ] Add matrix for OS/architecture testing
  - [ ] Automate publishing on tag
  - [ ] Integrate configurable logging levels in binary
- [ ] **Publishing**
  - [ ] Bump version to 1.0.0
  - [ ] Publish to crates.io
  - [ ] Create examples repository or demo app
  - [ ] Add an `examples/` crate for advanced usage demos
- [ ] **Maintenance**
  - [ ] Add CONTRIBUTING.md and issue templates
  - [ ] Set up code of conduct and license checks
  - [ ] Monitor for bugs via GitHub issues
  - [ ] Add protocol diagrams to `docs/design.md`

### Metrics
- [ ] Crate published on crates.io
- [ ] CI passing on all targets
- [ ] >90% coverage

## Overall Notes
- **Dependencies**: Update crates (nom, tokio) regularly; test compatibility. Pin versions to avoid breakage (e.g., `tokio = \"1.0\"`).
- **Risks**: Hardware access limitations; resolve spec ambiguities by referencing EN 13757.
- **Tracking**: Update checkboxes as tasks complete. Reassess timelines based on progress.
- **Resources**: Use libmbus C code as reference; leverage tools like `cargo-fuzz`, `criterion`.
- **Additional**: Ensure thread-safety in `mbus_device_manager.rs` with mutexes if needed. Add configurable timeouts/keys in `handle.rs`.