# File Format Specification

The `.acb` binary format stores a complete code concept graph in a single file. This document describes the on-disk layout, section structure, and design rationale.

## Design Goals

1. **O(1) random access.** Look up any code unit by ID without scanning the file.
2. **Compact.** LZ4 compression for variable-length strings. Fixed-size records for units and edges.
3. **Memory-mappable.** The format supports `mmap()` for zero-copy access to unit and edge tables.
4. **Forward-compatible.** New fields are appended to the header. Older readers skip unknown sections.

## File Layout

```
Offset 0x00   ┌─────────────────────────────┐
              │         Header (128 B)       │
              ├─────────────────────────────┤
              │     Unit Table (96N bytes)    │  N = unit_count
              ├─────────────────────────────┤
              │     Edge Table (40M bytes)    │  M = edge_count
              ├─────────────────────────────┤
              │  String Pool (LZ4 compressed) │  Variable size
              ├─────────────────────────────┤
              │  Feature Vectors (f32 array)  │  N * dim * 4 bytes
              └─────────────────────────────┘
```

## Header (128 bytes)

| Offset | Size | Type | Field | Description |
|:---|:---|:---|:---|:---|
| 0x00 | 4 | `[u8; 4]` | magic | Magic bytes: `ACB\0` |
| 0x04 | 4 | `u32` | version | Format version (currently 1) |
| 0x08 | 8 | `u64` | unit_count | Number of code units |
| 0x10 | 8 | `u64` | edge_count | Number of edges |
| 0x18 | 8 | `u64` | string_pool_offset | Byte offset of string pool section |
| 0x20 | 8 | `u64` | string_pool_size | Compressed size of string pool |
| 0x28 | 8 | `u64` | feature_offset | Byte offset of feature vector section |
| 0x30 | 4 | `u32` | dimension | Feature vector dimensionality |
| 0x34 | 8 | `u64` | timestamp | Compilation timestamp (Unix epoch) |
| 0x3C | 52 | `[u8; 52]` | reserved | Reserved for future fields |

Total: 128 bytes (fixed).

## Unit Table

Starts immediately after the header at offset 128. Each unit record is 96 bytes.

| Offset | Size | Type | Field | Description |
|:---|:---|:---|:---|:---|
| 0x00 | 8 | `u64` | id | Unique unit identifier |
| 0x04 | 4 | `u32` | name_offset | Offset into decompressed string pool |
| 0x08 | 4 | `u32` | name_length | Length of name string |
| 0x0C | 4 | `u32` | qname_offset | Qualified name offset |
| 0x10 | 4 | `u32` | qname_length | Qualified name length |
| 0x14 | 1 | `u8` | unit_type | UnitType enum discriminant |
| 0x15 | 1 | `u8` | language | Language enum discriminant |
| 0x16 | 1 | `u8` | visibility | Visibility enum discriminant |
| 0x17 | 1 | `u8` | flags | Bit flags (is_async, is_generator, etc.) |
| 0x18 | 4 | `u32` | file_offset | File path offset in string pool |
| 0x1C | 4 | `u32` | file_length | File path length |
| 0x20 | 4 | `u32` | start_line | Span start line |
| 0x24 | 4 | `u32` | start_col | Span start column |
| 0x28 | 4 | `u32` | end_line | Span end line |
| 0x2C | 4 | `u32` | end_col | Span end column |
| 0x30 | 4 | `u32` | complexity | Cyclomatic complexity |
| 0x34 | 4 | `f32` | stability | Stability score (0.0 - 1.0) |
| 0x38 | 4 | `u32` | sig_offset | Signature string offset (0 if none) |
| 0x3C | 4 | `u32` | sig_length | Signature string length |
| 0x40 | 4 | `u32` | doc_offset | Doc summary offset (0 if none) |
| 0x44 | 4 | `u32` | doc_length | Doc summary length |
| 0x48 | 24 | `[u8; 24]` | reserved | Reserved for future fields |

Total: 96 bytes per unit.

## Edge Table

Starts after the unit table. Each edge record is 40 bytes.

| Offset | Size | Type | Field | Description |
|:---|:---|:---|:---|:---|
| 0x00 | 8 | `u64` | source_id | Source unit ID |
| 0x08 | 8 | `u64` | target_id | Target unit ID |
| 0x10 | 1 | `u8` | edge_type | EdgeType enum discriminant |
| 0x11 | 7 | `[u8; 7]` | padding | Alignment padding |
| 0x18 | 8 | `f64` | weight | Edge weight (0.0 - 1.0) |
| 0x20 | 8 | `[u8; 8]` | reserved | Reserved |

Total: 40 bytes per edge.

## String Pool

The string pool contains all variable-length text: unit names, qualified names, file paths, signatures, and documentation summaries. Stored as a single contiguous buffer, LZ4-compressed.

On read, the entire pool is decompressed into memory. String references in unit records use (offset, length) pairs into this decompressed buffer.

### Compression

LZ4 block compression is used for the string pool. Typical compression ratios on source code metadata:

- English identifiers: ~2.5x compression
- File paths with common prefixes: ~3-4x compression
- Documentation text: ~2-3x compression

LZ4 decompression runs at 3-5 GB/s on modern hardware, making the decompression cost negligible.

## Feature Vectors

Feature vectors are stored as a flat array of `f32` values, one vector per unit. The vector for unit N starts at offset `feature_offset + N * dimension * 4`.

Default dimension is 64, configurable at compile time. Vectors are not compressed since f32 values compress poorly.

## Versioning

The `version` field in the header enables forward compatibility:

- **Version 1** (current): Base format as described in this document.
- Future versions will maintain backward compatibility by appending new sections after existing ones and using reserved header fields.

Readers should check the version field and reject files with unsupported versions rather than attempting to parse unknown formats.

## Checksum

The current format does not include checksums. File integrity can be verified using external tools (e.g., `blake3sum`). A checksum field may be added in a future version using reserved header space.
