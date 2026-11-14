# Benchmark results 2026-11-14-linux

Written by `benchmarks/run.py` in default mode at commit 91425fb.
Times are per call.

- Machine: Linux-6.18.33.2-microsoft-standard-WSL2-x86_64-with-glibc2.36, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.7 ms | 1.69 ms | 9.50 ms | 233.3 MB/s |
| fpeg grammar load | 268 µs | 56.8 µs | 234 µs |  |
| fpeg parse, tree | 68.2 ms | 11.0 ms | 61.0 ms | 36.8 MB/s |
| fpeg parse and grammar builders, Python objects | 87.2 ms | 15.7 ms | 75.6 ms | 28.7 MB/s |
| fpeg parse and Transformer methods, Python objects | 120 ms | 28.1 ms | 104 ms | 20.9 MB/s |
| Lark grammar load | 4.55 ms | 204 µs | 4.35 ms |  |
| Lark LALR, tree | 1.15 s | 44.4 ms | 1.08 s | 2.2 MB/s |
| Lark Transformer over the tree | 332 ms | 30.1 ms | 304 ms | 7.6 MB/s |
| Lark LALR with inlined Transformer, Python objects | 923 ms | 44.8 ms | 858 ms | 2.7 MB/s |
| pe machine grammar load | 2.11 ms | 82.7 µs | 2.02 ms |  |
| pe machine, match only | 114 ms | 16.2 ms | 106 ms | 21.9 MB/s |
| pe machine, Python objects | 161 ms | 19.3 ms | 147 ms | 15.6 MB/s |
| pe packrat grammar load | 2.37 ms | 215 µs | 2.26 ms |  |
| pe packrat, match only | 436 ms | 34.1 ms | 394 ms | 5.7 MB/s |
| pe packrat, Python objects | 451 ms | 30.4 ms | 407 ms | 5.6 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 138 µs | 29.5 µs | 118 µs |  |
| transform, a method on every node, list | 1.91 ms | 311 µs | 1.73 ms |  |
| transform, a method on every node, *children | 1.81 ms | 213 µs | 1.65 ms |  |
| transform, the same with a member by field | 1.84 ms | 270 µs | 1.66 ms |  |
| transform, a method on every token, taking nothing | 2.36 ms | 247 µs | 2.19 ms |  |
| transform, a method on every token, taking it | 3.55 ms | 281 µs | 3.34 ms |  |
| interpreter, no method | 9.21 ms | 934 µs | 8.58 ms |  |
| interpreter, a method on every node | 10.5 ms | 1.35 ms | 9.40 ms |  |
| visitor, no method | 2.11 ms | 381 µs | 1.85 ms |  |
| visitor, a method on every node | 3.34 ms | 361 µs | 3.08 ms |  |
| parse | 6.21 ms | 825 µs | 5.59 ms |  |
| parse, then every view and token | 11.1 ms | 1.76 ms | 9.85 ms |  |
| 2000 small CSV parses with a transformer | 5.21 ms | 886 µs | 4.54 ms |  |
| 2000 small CSV parses with csv.reader | 1.38 ms | 169 µs | 1.24 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 14.9 ms | 2.23 ms | 13.5 ms | 6.3 MB/s |
| fpeg parse, tree | 15.8 ms | 2.52 ms | 14.6 ms | 5.9 MB/s |
| fpeg parse and Transformer, numbers | 17.8 ms | 1.40 ms | 16.7 ms | 5.3 MB/s |
| Lark LALR, tree | 93.1 ms | 9.89 ms | 86.9 ms | 1.0 MB/s |
| Lark LALR with inlined Transformer, numbers | 88.6 ms | 14.6 ms | 80.6 ms | 1.1 MB/s |
| pe machine, numbers | 38.3 ms | 4.32 ms | 35.3 ms | 2.5 MB/s |
| pe packrat, numbers | 95.0 ms | 8.62 ms | 88.1 ms | 1.0 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.66 ms | 518 µs | 7.21 ms | 257,381 lines/s |
| CPython parser and symbol table, no AST objects | 5.08 ms | 253 µs | 4.87 ms | 387,882 lines/s |
| compile to bytecode | 6.72 ms | 305 µs | 6.43 ms | 293,272 lines/s |
| tree-sitter parse, native tree | 5.92 ms | 311 µs | 5.55 ms | 332,924 lines/s |
| tree-sitter cursor walk over every node | 1.28 ms | 79.2 µs | 1.21 ms |  |
| tree-sitter walk, a Python Node per node | 1.80 ms | 99.8 µs | 1.70 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 3.55 ms | 236 µs | 3.37 ms |  |
| LibCST parse_module | 63.8 ms | 11.0 ms | 57.7 ms | 30,895 lines/s |
| Lark Python grammar load | 223 ms | 20.3 ms | 204 ms |  |
| Lark LALR, bundled Python grammar, tree | 88.4 ms | 22.3 ms | 74.2 ms | 22,298 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 508 ms | 40.0 ms | 460 ms |  |
| grammar load with a warm cache | 27.0 ms | 2.23 ms | 25.2 ms |  |
| LALR parse, tree | 145 ms | 17.8 ms | 135 ms | 13,595 lines/s |
| Transformer with the real callbacks | 17.1 ms | 2.17 ms | 15.6 ms |  |
| Transformer, trivial method on every rule | 8.13 ms | 962 µs | 7.59 ms |  |
| end to end: parse, transform, fix locations | 169 ms | 13.4 ms | 156 ms | 11,646 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| expressions/fpeg machine, every rule remembered, tree | 85.1 ms | 4.83 ms | 81.0 ms | 1.2 MB/s |
| expressions/fpeg machine, tree | 69.0 ms | 3.56 ms | 64.9 ms | 1.5 MB/s |
| fpeg grammar load | 251 µs | 34.4 µs | 228 µs |  |
| fpeg machine on a small sum | 4.77 µs | 999 ns | 4.34 µs |  |
| fpeg machine on a small sum, nothing remembered | 5.85 ms | 1.62 ms | 5.26 ms |  |
| json/fpeg machine with its profile, tree | 352 ms | 9.93 ms | 338 ms | 7.1 MB/s |
| json/fpeg machine, tree | 66.5 ms | 2.64 ms | 64.0 ms | 37.7 MB/s |
| json/pest generated, token queue | 50.7 ms | 1.73 ms | 48.3 ms | 49.5 MB/s |
| json/pest_vm interpreter, token queue | 257 ms | 8.46 ms | 239 ms | 9.7 MB/s |
| json/serde_json to Value | 6.77 ms | 247 µs | 6.35 ms | 370.0 MB/s |
| pest_vm grammar load | 382 µs | 54.1 µs | 347 µs |  |
