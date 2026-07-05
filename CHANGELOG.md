# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-07-05

### Added
- **MSRV Update**: Constraint of `1.87.0` specified in `Cargo.toml` for compatibility.

### Changed
- **CLI Display**: Improved block device path printing to show full/canonical paths (e.g. `/dev/mapper/vg0-lv0`) instead of assuming all devices reside directly under `/dev/`.

### Removed
- **Dependency Clean**: Unused `nix` dependency removed from `Cargo.toml` to reduce compilation time.

## [0.1.0] - 2026-07-01

### Added
- **x86_64 Assembly Accel**: High-performance streaming copies using Non-Temporal (NT) stores (`movntdq`, `sfence`) and ERMS (`rep movsb`).
- **Direct I/O Support**: Page-aligned allocator enforcing `4096`-byte boundary conditions for safe `O_DIRECT` execution.
- **Cache Engine**: High-performance LBA lookup table with dirty tracking, eviction policy (clean/dirty priority), and concurrent background flush threads.
- **CLI Commands**:
  - `cache` to mount the cache engine onto raw block devices.
  - `bench` to run synthetic benchmark suites (sequential/random read/write).
  - `info` to introspect block device hardware details (size, sectors, LBA).
- **Diagnostics Emitter**: `rustc` compiler-like terminal layout mapping specific error codes (`E0001` - `E0012`) with warning and info gutters.
- **CLI Styling**: Custom `clap` v4 styles rendering headers in bold green and commands/placeholders in bold cyan, matching standard Cargo output formats.
