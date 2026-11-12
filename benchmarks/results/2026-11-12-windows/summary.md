# Benchmark results 2026-11-12-windows

Written by `benchmarks/run.py` in default mode at commit 8a174ef.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.13.15, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 12.0 ms | 569 µs | 11.3 ms | 208.8 MB/s |
| Lark grammar load | 6.04 ms | 122 µs | 5.93 ms |  |
| Lark LALR, tree | 1.48 s | 17.2 ms | 1.45 s | 1.7 MB/s |
| Lark Transformer over the tree | 451 ms | 23.7 ms | 427 ms | 5.6 MB/s |
| Lark LALR with inlined Transformer, Python objects | 1.20 s | 39.0 ms | 1.15 s | 2.1 MB/s |
| pe machine grammar load | 3.04 ms | 267 µs | 2.85 ms |  |
| pe machine, match only | 150 ms | 7.18 ms | 140 ms | 16.7 MB/s |
| pe machine, Python objects | 209 ms | 15.6 ms | 199 ms | 12.0 MB/s |
| pe packrat grammar load | 3.19 ms | 105 µs | 3.04 ms |  |
| pe packrat, match only | 538 ms | 60.2 ms | 504 ms | 4.7 MB/s |
| pe packrat, Python objects | 553 ms | 48.2 ms | 522 ms | 4.5 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.72 ms | 159 µs | 7.44 ms | 255,408 lines/s |
| CPython parser and symbol table, no AST objects | 5.49 ms | 154 µs | 5.24 ms | 359,080 lines/s |
| compile to bytecode | 8.74 ms | 448 µs | 8.29 ms | 225,402 lines/s |
| tree-sitter parse, native tree | 6.72 ms | 242 µs | 6.54 ms | 293,284 lines/s |
| tree-sitter cursor walk over every node | 1.64 ms | 28.3 µs | 1.60 ms |  |
| tree-sitter walk, a Python Node per node | 2.41 ms | 64.5 µs | 2.31 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.57 ms | 87.5 µs | 4.40 ms |  |
| LibCST parse_module | 94.5 ms | 3.01 ms | 90.5 ms | 20,866 lines/s |
| Lark Python grammar load | 307 ms | 7.07 ms | 298 ms |  |
| Lark LALR, bundled Python grammar, tree | 100 ms | 2.24 ms | 97.6 ms | 19,616 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 679 ms | 10.9 ms | 668 ms |  |
| grammar load with a warm cache | 36.0 ms | 2.58 ms | 34.1 ms |  |
| LALR parse, tree | 181 ms | 11.9 ms | 172 ms | 10,899 lines/s |
| Transformer with the real callbacks | 21.9 ms | 1.73 ms | 20.7 ms |  |
| Transformer, trivial method on every rule | 10.6 ms | 384 µs | 10.2 ms |  |
| end to end: parse, transform, fix locations | 208 ms | 5.22 ms | 199 ms | 9,493 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json/pest generated, token queue | 60.5 ms | 771 µs | 59.7 ms | 41.4 MB/s |
| json/pest_vm interpreter, token queue | 312 ms | 2.64 ms | 309 ms | 8.0 MB/s |
| json/serde_json to Value | 9.54 ms | 234 µs | 9.15 ms | 262.7 MB/s |
| pest_vm grammar load | 640 µs | 33.9 µs | 580 µs |  |
