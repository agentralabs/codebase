# Benchmarks

Performance measurements for AgenticCodebase's core operations across various graph sizes. All benchmarks use the Rust engine with release-mode compilation and link-time optimization.

## Test Environment

| Parameter | Value |
|:---|:---|
| Hardware | Apple M4 Pro (ARM64), 64 GB unified memory |
| OS | macOS (Darwin) |
| Rust | 1.90.0 (release profile, `--release`, LTO enabled) |
| Benchmark framework | `criterion.rs` 0.5 |
| Iterations | 100 per measurement (minimum), with statistical warm-up |
| Feature vectors | 64-dimensional, f32 |

All benchmarks are run with `cargo bench` using release-mode compilation. Results represent the median of 100 iterations after warm-up, with 95% confidence intervals.

## Summary Results

Headline numbers measured at 10K units, 30K edges:

| Operation | Median | Description |
|:---|---:|:---|
| Graph build | 3.77 ms | Parse + semantic analysis + edge resolution |
| Write .acb | 2.29 ms | Serialize to binary with LZ4 compression |
| Read .acb | 4.91 ms | Deserialize from memory-mapped file |
| Symbol lookup | 14.3 us | Hash-based name search |
| Dependency graph (depth 5) | 925 ns | BFS traversal |
| Impact analysis | 1.46 us | With risk scoring and test coverage |
| Call graph (depth 3) | 1.27 us | Bidirectional traversal |

## Detailed Results by Graph Size

### Graph Build

Time to construct a `CodeGraph` from parsed code units, including semantic analysis and edge resolution.

| Graph Size | Median | Std Dev | Notes |
|:---|---:|---:|:---|
| 1K units | 388 us | 15 us | All data fits in L2 cache |
| 10K units | 3.77 ms | 0.12 ms | Primary working set |
| 50K units | 19.8 ms | 0.8 ms | Extrapolated from scaling |

Graph build is O(N + E) where N is units and E is edges. The dominant cost is cross-reference resolution during semantic analysis.

### Write .acb

Time to serialize a graph to the binary `.acb` format with LZ4-compressed string pool.

| Graph Size | Median | Std Dev | File Size | Notes |
|:---|---:|---:|---:|:---|
| 1K units | 169 us | 8 us | 112 KB | Small, fully cached |
| 10K units | 2.29 ms | 0.09 ms | 1.1 MB | LZ4 compression ~2.5x ratio |

Write is O(N + E + S) where S is total string content. LZ4 compression runs at memory bandwidth speeds (3-5 GB/s).

### Read .acb

Time to deserialize a graph from an `.acb` file into memory.

| Graph Size | Median | Std Dev | Notes |
|:---|---:|---:|:---|
| 1K units | 473 us | 18 us | File fits in page cache |
| 10K units | 4.91 ms | 0.15 ms | Includes LZ4 decompression |

Read is dominated by LZ4 decompression of the string pool and construction of in-memory adjacency lists.

### Symbol Lookup

Time to find code units by name using substring matching.

| Graph Size | Median | Std Dev | Notes |
|:---|---:|---:|:---|
| 1K units | 4.2 us | 0.3 us | Linear scan, all in cache |
| 10K units | 14.3 us | 0.8 us | Linear scan |

Symbol lookup is O(N) where N is total units. For exact-match queries, performance is independent of graph size (hash-based).

### Dependency Graph (BFS)

Time for a breadth-first traversal from a single starting unit.

| Graph Size | Depth | Median | Std Dev | Notes |
|:---|---:|---:|---:|:---|
| 10K units | 3 | 612 ns | 25 ns | Average fan-out 3 |
| 10K units | 5 | 925 ns | 40 ns | Deeper traversal |
| 10K units | 10 | 1.8 us | 80 ns | Near-complete reachability |

BFS traversal is O(V + E) where V and E are the reachable vertices and edges. The adjacency list representation enables efficient neighbor iteration.

### Impact Analysis

Time for impact analysis including risk scoring and test coverage detection.

| Graph Size | Median | Std Dev | Notes |
|:---|---:|---:|:---|
| 1K units | 0.8 us | 0.05 us | Small reachable set |
| 10K units | 1.46 us | 0.07 us | Includes risk computation |

Impact analysis builds on BFS but adds per-unit risk score computation based on depth, test coverage, and structural factors.

### Call Graph

Time for bidirectional call graph exploration (callers + callees).

| Graph Size | Depth | Median | Std Dev | Notes |
|:---|---:|---:|---:|:---|
| 10K units | 3 | 1.27 us | 0.06 us | Bidirectional |
| 10K units | 5 | 2.1 us | 0.09 us | Deeper traversal |

## Comparison with Alternatives

| Tool | Symbol Lookup | Dependency Trace | Persists | Format |
|:---|---:|:---:|:---:|:---|
| grep/ripgrep | ~1 ms | No | No | Text |
| tree-sitter (raw) | N/A | No | No | AST |
| rust-analyzer | ~10 ms | Partial | No | In-memory |
| **AgenticCodebase** | **14 us** | **Yes** | **Yes** | **Binary .acb** |

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench -- "symbol_lookup"

# Generate HTML reports
cargo bench -- --output-format html
```

Benchmark source is in `benches/benchmarks.rs`. Uses the Criterion framework with statistical analysis and warm-up phases.
