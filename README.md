# asm-cache-inject

`asm-cache-inject` is a low-level I/O RAM cache engine for raw block devices (HDDs, SSDs, etc.) featuring custom **x86_64 assembly-accelerated memory copying**. It bypasses the kernel page cache using `O_DIRECT` and manages its own high-performance caching layer in user-space.

The CLI has been meticulously styled to look and feel like the Rust compiler (`rustc`/`cargo`), providing clear diagnostic gutters, note/help annotations, and specific error codes (`E0001` to `E0012`).

---

## Features

- **x86_64 Assembly Copying**: Bypasses standard `memcpy` with highly-optimized streaming routines:
  - **ERMS** (`rep movsb` detection and usage for massive memory transfers).
  - **Non-Temporal Stores** (`movntdq`, `sfence` to stream cache-bypassing writes directly to main memory, keeping CPU caches clean).
- **Direct I/O (`O_DIRECT`)**: Complete kernel bypass. Enforces page-aligned (`4096`-byte boundary) memory buffers to satisfy hardware DMA alignment constraints.
- **Eviction & Writeback Caching**:
  - Thread-safe LBA (Logical Block Address) lookup table.
  - Prioritized page eviction (clean pages evicted before dirty ones).
  - Asynchronous background flush thread matching configured watermarks and dirty limits.
- **Built-in Benchmark Suite**: Run direct vs. cached I/O operations (sequential & random read/write patterns) to measure speedup factors.
- **Rustc-Style Diagnostics**: Real-time visual feedback using compiler-style gutters, colored notes, warning frames, and structured errors.

---

## Project Structure

```text
src/
├── asm/             # x86_64 assembly implementations (ERMS & non-temporal)
├── bench/           # Benchmark runners and compiler-style comparison reports
├── cli/             # CLI argument definitions, command handlers, and atomic printer
├── engine/          # Cache orchestrator, configuration, and worker threads
├── io/              # Raw O_DIRECT block device interactions and metadata queries
├── memory/          # Page-aligned allocators and reusable block buffer pools
├── error.rs         # Global Error definitions and diagnostic mapping
├── lib.rs           # Library entry point
└── main.rs          # CLI binary entry point
```

---

## Prerequisites

- **OS**: Linux (uses Linux-specific system calls like `O_DIRECT`, `ioctl`, and device sysfs queries).
- **CPU**: x86_64 architecture (with SSE/AVX capabilities).
- **Permissions**: Root/superuser privileges (e.g. `sudo`) are required to read/write directly to raw block devices.

---

## Getting Started

### 1. Build the Project

Build an optimized production release binary:
```bash
cargo build --release
```
The compiled binary will be located at `./target/release/asm-cache-inject`.

### 2. Verify Compilation & Tests
Run unit tests to ensure page alignment and assembly copy functions work correctly on your hardware:
```bash
cargo test
```

---

## Command Usage

```bash
asm-cache-inject [OPTIONS] <COMMAND>
```

### 1. Introspect a Block Device (`info`)
View detailed hardware metadata, sector sizes, sector counts, and calculated maximum LBAs:
```bash
sudo ./target/release/asm-cache-inject info --device /dev/sdb
```

### 2. Benchmark I/O Performance (`bench`)
Compare direct block device access speeds against cached accesses to see speedup multipliers:
```bash
# Read-only sequential benchmark
sudo ./target/release/asm-cache-inject bench --device /dev/sdb --blocks 10000

# Destructive read-write benchmark (Warning: Will overwrite data)
sudo ./target/release/asm-cache-inject bench --device /dev/sdb --blocks 10000 --write
```

### 3. Launch Cache Engine (`cache`)
Mount the RAM cache engine on top of the block device to start acceleration:
```bash
sudo ./target/release/asm-cache-inject cache --device /dev/sdb --size 512
```

---

## Diagnostics Style Example

All CLI output uses a thread-safe atomic printer designed around the `rustc` compiler diagnostic layout.

### Success note:
```text
   Injecting cache-engine onto `/dev/sdb`
  --> /dev/sdb
   |
  1 | CacheConfig {
  2 |   cache_size:      512 MiB
  3 |   block_size:      4096 B
  4 |   flush_interval:  5 s
  5 |   dirty_watermark: 75%
  6 |   read_only:       false
  7 | }
   |
   = note: cache engine successfully running
   = help: press `Ctrl+C` to gracefully shut down and commit all dirty blocks.
```

### Warnings:
```text
warning: destructive write benchmarks enabled
  --> /dev/sdb
   |
   = caution: data on the device WILL be overwritten!
   = help: ensure you have backed up any important data before proceeding.
```

### Error codes (`E0001` - `E0012`):
```text
error[E0012]: permission denied: open (read-only): Permission denied
  --> device
   |
   = note: reading/writing to raw block devices requires root privileges.
   = help: try running with `sudo` or check if the user has `CAP_SYS_RAWIO` capability.
```

---

## License

Copyright (c) 2026 v1lnv. Licensed under the [MIT License](LICENSE).
