# Implementation Tracker for mbus-rs Industrial-Grade Upgrade

This tracker outlines the phased plan to transform mbus-rs into an industrial-grade crate. Use checkboxes to mark completion. Estimated timeline: 3-6 months. Start with Phase 1 immediate steps.

## Phase 1: Foundation Building (1-2 weeks)
Focus: Complete core functionality, eliminate gaps, ensure basic quality.

### Tasks
- [ ] **Complete Parsing Logic**
  - [ ] Implement full multi-record handling in variable parsing (e.g., loop until end in `parse_records`)
  - [ ] Port complete VIFE/FD/FB tables from libmbus to `vif_maps.rs`
  - [ ] Test VIFE parsing with edge cases (e.g., extensions beyond 0xFF)
- [ ] **Enhance Error Handling**
  - [ ] Add specific error variants (e.g., `InvalidTimestamp`, `Overflow`) in `error.rs`
  - [ ] Ensure all parsing functions return detailed errors with context
  - [ ] Update error strings for clarity (e.g., include field names)
- [ ] **Documentation**
  - [ ] Add doc comments to all public APIs (functions, structs, enums)
  - [ ] Expand README.md with architecture diagram, spec references (EN 13757), and advanced examples
  - [ ] Generate and review `cargo doc` output; fix inconsistencies

### Metrics
- [ ] 100% compilation without warnings (run `cargo check`)
- [ ] All non-hardware tests passing (run `cargo test --lib`)

### Immediate Next Steps (1-3 days)
- [x] Port full VIFE maps to `vif_maps.rs` (copy from libmbus C code)
- [x] Implement complete variable record loop in `record.rs` (handle multiple records per frame)
- [x] Run `cargo doc` and review; add missing docs to public items

## Phase 2: Testing and Validation (2-4 weeks)
Focus: Achieve high coverage and verify against real-world data.

### Tasks
- [ ] **Unit/Integration Tests**
  - [ ] Add golden frame tests (integrate 20+ .hex files from libmbus/test-frames into `golden_frames.rs`)
  - [ ] Enable and expand error frame tests (from libmbus/test/error-frames)
  - [ ] Add fuzzing for parsers using `cargo-fuzz` (target `parse_vib`, `parse_records`)
- [ ] **Coverage Tools**
  - [ ] Install/use cargo-llvm-cov or tarpaulin
  - [ ] Run coverage analysis; identify and test low-coverage areas
  - [ ] Achieve 80%+ line/function coverage
- [ ] **Benchmarking**
  - [ ] Add benchmarks using `criterion` (e.g., for parsing/serial I/O)
  - [ ] Profile performance with `cargo flamegraph`; optimize hotspots
  - [ ] Ensure <10ms for typical frame parsing
- [ ] **Hardware Validation**
  - [ ] Enable ignored tests with mock serial ports (e.g., via `tokio-serial` mocks)
  - [ ] Add integration tests for real devices (if hardware available)
  - [ ] Test on multiple platforms (Linux, macOS, Windows)

### Metrics
- [ ] 80% test coverage
- [ ] All tests passing
- [ ] Benchmarks show <10ms for typical operations

### Immediate Next Steps (continued from Phase 1)
- [x] Integrate golden frames (e.g., parse ACW_Itron-BM-plus-m.hex and compare to .norm.xml)
- [ ] Run coverage: `cargo llvm-cov --lcov --output-path lcov.info` and review report

## Phase 3: Enhancement and Features (3-6 weeks)
Focus: Add advanced features for usability and performance.

### Tasks
- [ ] **Async Improvements**
  - [ ] Ensure all I/O is non-blocking (review tokio usage)
  - [ ] Add support for TCP M-Bus (implement `mbus-tcp` module)
  - [ ] Handle timeouts and retries gracefully
- [ ] **Security**
  - [ ] Audit for buffer overflows and injection vulnerabilities
  - [ ] Add input validation (e.g., max frame size limits)
  - [ ] Support encrypted modes if in M-Bus spec
- [ ] **Features**
  - [ ] Add WMBus support (wireless M-Bus) in `wmbus/` module
  - [ ] Add configurable options (e.g., baud rate, timeouts) via builder pattern
  - [ ] Create a CLI tool for testing (e.g., `mbus-cli` binary)
- [ ] **Performance**
  - [ ] Profile with flamegraph and optimize (e.g., avoid allocations in hot paths)
  - [ ] Support for large datasets (e.g., streaming parsing)
  - [ ] Memory safety checks (run `cargo miri` if possible)

### Metrics
- [ ] No critical vulnerabilities (run `cargo-audit`)
- [ ] New features documented with examples
- [ ] Performance benchmarks pass thresholds

## Phase 4: Quality Assurance and Release (2-4 weeks)
Focus: Polish for production and community.

### Tasks
- [ ] **Audits/Reviews**
  - [ ] Run `cargo-audit`, `clippy`, `rustfmt`
  - [ ] Perform external code review or security audit
  - [ ] Fix all lints and format issues
- [ ] **CI/CD**
  - [ ] Set up GitHub Actions for tests, coverage, releases
  - [ ] Add matrix for OS/architecture testing
  - [ ] Automate publishing on tag
- [ ] **Publishing**
  - [ ] Bump version to 1.0.0
  - [ ] Publish to crates.io
  - [ ] Create examples repository or demo app
- [ ] **Maintenance**
  - [ ] Add CONTRIBUTING.md and issue templates
  - [ ] Set up code of conduct and license checks
  - [ ] Monitor for bugs via GitHub issues

### Metrics
- [ ] Crate published on crates.io
- [ ] CI passing on all targets
- [ ] >90% coverage

## Overall Notes
- **Dependencies**: Update crates (nom, tokio) regularly; test compatibility.
- **Risks**: Hardware access limitations; resolve spec ambiguities by referencing EN 13757.
- **Tracking**: Update checkboxes as tasks complete. Reassess timelines based on progress.
- **Resources**: Use libmbus C code as reference; leverage tools like `cargo-fuzz`, `criterion`.