# Benchmark results 2026-11-14b-windows

Written by `benchmarks/run.py` in default mode at commit 68f7020.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.9 ms | 785 µs | 9.94 ms | 230.3 MB/s |
| fpeg grammar load | 181 µs | 18.2 µs | 159 µs |  |
| fpeg parse, tree | 30.6 ms | 1.96 ms | 27.9 ms | 81.9 MB/s |
| fpeg parse and grammar builders, Python objects | 48.7 ms | 3.38 ms | 45.4 ms | 51.4 MB/s |
| fpeg parse and Transformer methods, Python objects | 83.9 ms | 6.42 ms | 76.6 ms | 29.9 MB/s |
| Lark grammar load | 5.12 ms | 194 µs | 4.91 ms |  |
| Lark LALR, tree | 1.17 s | 24.7 ms | 1.13 s | 2.1 MB/s |
| Lark Transformer over the tree | 374 ms | 17.3 ms | 349 ms | 6.7 MB/s |
| Lark LALR with inlined Transformer, Python objects | 997 ms | 18.1 ms | 977 ms | 2.5 MB/s |
| pe machine grammar load | 2.43 ms | 173 µs | 2.27 ms |  |
| pe machine, match only | 138 ms | 9.37 ms | 129 ms | 18.2 MB/s |
| pe machine, Python objects | 203 ms | 24.5 ms | 176 ms | 12.4 MB/s |
| pe packrat grammar load | 2.64 ms | 163 µs | 2.51 ms |  |
| pe packrat, match only | 450 ms | 11.8 ms | 435 ms | 5.6 MB/s |
| pe packrat, Python objects | 475 ms | 21.9 ms | 452 ms | 5.3 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 149 µs | 18.6 µs | 137 µs |  |
| transform, a method on every node, list | 2.24 ms | 112 µs | 2.16 ms |  |
| transform, a method on every node, *children | 2.37 ms | 232 µs | 2.14 ms |  |
| transform, the same with a member by field | 2.34 ms | 173 µs | 2.18 ms |  |
| transform, a method on every token, taking nothing | 2.46 ms | 124 µs | 2.33 ms |  |
| transform, a method on every token, taking it | 3.77 ms | 209 µs | 3.62 ms |  |
| interpreter, no method | 9.75 ms | 625 µs | 9.29 ms |  |
| interpreter, a method on every node | 11.6 ms | 705 µs | 10.7 ms |  |
| visitor, no method | 2.08 ms | 84.0 µs | 2.02 ms |  |
| visitor, a method on every node | 3.37 ms | 249 µs | 3.11 ms |  |
| parse | 3.09 ms | 123 µs | 2.93 ms |  |
| parse, then every view and token | 8.20 ms | 588 µs | 7.56 ms |  |
| 2000 small CSV parses with a transformer | 4.63 ms | 204 µs | 4.44 ms |  |
| 2000 small CSV parses with csv.reader | 1.63 ms | 93.1 µs | 1.57 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 17.5 ms | 1.82 ms | 16.5 ms | 5.4 MB/s |
| fpeg parse, tree | 5.27 ms | 614 µs | 4.74 ms | 17.8 MB/s |
| fpeg parse and Transformer, numbers | 8.40 ms | 893 µs | 7.69 ms | 11.2 MB/s |
| Lark LALR, tree | 100 ms | 6.72 ms | 93.3 ms | 0.9 MB/s |
| Lark LALR with inlined Transformer, numbers | 99.1 ms | 7.92 ms | 91.6 ms | 0.9 MB/s |
| pe machine, numbers | 46.6 ms | 1.68 ms | 45.0 ms | 2.0 MB/s |
| pe packrat, numbers | 106 ms | 6.18 ms | 98.3 ms | 0.9 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.72 ms | 440 µs | 7.38 ms | 255,218 lines/s |
| CPython parser and symbol table, no AST objects | 5.87 ms | 273 µs | 5.59 ms | 335,879 lines/s |
| compile to bytecode | 8.66 ms | 160 µs | 8.47 ms | 227,477 lines/s |
| tree-sitter parse, native tree | 7.36 ms | 591 µs | 6.72 ms | 267,677 lines/s |
| tree-sitter cursor walk over every node | 2.41 ms | 87.5 µs | 2.29 ms |  |
| tree-sitter walk, a Python Node per node | 2.93 ms | 163 µs | 2.75 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.87 ms | 269 µs | 4.52 ms |  |
| LibCST parse_module | 85.2 ms | 10.1 ms | 75.7 ms | 23,146 lines/s |
| Lark Python grammar load | 244 ms | 13.0 ms | 227 ms |  |
| Lark LALR, bundled Python grammar, tree | 84.1 ms | 5.01 ms | 78.1 ms | 23,444 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 551 ms | 19.1 ms | 517 ms |  |
| grammar load with a warm cache | 29.4 ms | 2.34 ms | 26.9 ms |  |
| LALR parse, tree | 147 ms | 9.90 ms | 138 ms | 13,378 lines/s |
| Transformer with the real callbacks | 17.4 ms | 401 µs | 16.8 ms |  |
| Transformer, trivial method on every rule | 8.98 ms | 149 µs | 8.81 ms |  |
| end to end: parse, transform, fix locations | 170 ms | 11.5 ms | 158 ms | 11,618 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.13 ms | 179 µs | 2.03 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 27.4 ms | 387 µs | 26.8 ms | 3.7 MB/s |
| expressions/fpeg machine, tree | 11.8 ms | 488 µs | 11.5 ms | 8.5 MB/s |
| fpeg grammar load | 184 µs | 22.6 µs | 165 µs |  |
| fpeg machine on a small sum | 1.48 µs | 257 ns | 1.28 µs |  |
| fpeg machine on a small sum, nothing remembered | 3.33 µs | 715 ns | 2.85 µs |  |
| json/fpeg machine with its profile, tree | 343 ms | 19.2 ms | 321 ms | 7.3 MB/s |
| json/fpeg machine, tree | 31.3 ms | 1.44 ms | 29.5 ms | 80.2 MB/s |
| json/pest generated, token queue | 57.5 ms | 1.21 ms | 56.6 ms | 43.6 MB/s |
| json/pest_vm interpreter, token queue | 302 ms | 6.49 ms | 296 ms | 8.3 MB/s |
| json/serde_json to Value | 8.89 ms | 234 µs | 8.66 ms | 282.1 MB/s |
| passes on CSV/every pass | 1.57 ms | 66.1 µs | 1.50 ms |  |
| passes on CSV/without dispatch | 1.54 ms | 39.2 µs | 1.51 ms |  |
| passes on CSV/without hasher | 1.58 ms | 97.3 µs | 1.51 ms |  |
| passes on CSV/without inline | 1.84 ms | 119 µs | 1.75 ms |  |
| passes on CSV/without left_loop | 1.56 ms | 80.2 µs | 1.50 ms |  |
| passes on CSV/without skip | 1.54 ms | 51.6 µs | 1.48 ms |  |
| passes on CSV/without span | 1.76 ms | 70.2 µs | 1.68 ms |  |
| passes on CSV/without split | 2.03 ms | 79.4 µs | 1.95 ms |  |
| passes on expressions/every pass | 11.5 ms | 533 µs | 10.9 ms | 8.7 MB/s |
| passes on expressions/without dispatch | 16.9 ms | 1.11 ms | 15.7 ms | 5.9 MB/s |
| passes on expressions/without hasher | 12.6 ms | 976 µs | 11.7 ms | 8.0 MB/s |
| passes on expressions/without inline | 11.6 ms | 949 µs | 10.7 ms | 8.7 MB/s |
| passes on expressions/without left_loop | 40.5 ms | 2.99 ms | 37.9 ms | 2.5 MB/s |
| passes on expressions/without skip | 13.0 ms | 535 µs | 12.5 ms | 7.7 MB/s |
| passes on expressions/without span | 11.9 ms | 561 µs | 11.3 ms | 8.4 MB/s |
| passes on expressions/without split | 11.4 ms | 432 µs | 10.8 ms | 8.8 MB/s |
| passes on JSON/every pass | 30.6 ms | 1.53 ms | 28.8 ms | 81.9 MB/s |
| passes on JSON/without dispatch | 32.4 ms | 1.14 ms | 31.2 ms | 77.3 MB/s |
| passes on JSON/without hasher | 31.2 ms | 2.37 ms | 29.0 ms | 80.4 MB/s |
| passes on JSON/without inline | 46.4 ms | 1.93 ms | 44.5 ms | 54.0 MB/s |
| passes on JSON/without left_loop | 30.6 ms | 1.51 ms | 28.7 ms | 82.0 MB/s |
| passes on JSON/without skip | 40.1 ms | 3.34 ms | 37.1 ms | 62.4 MB/s |
| passes on JSON/without span | 32.1 ms | 1.51 ms | 30.4 ms | 78.0 MB/s |
| passes on JSON/without split | 40.0 ms | 1.82 ms | 37.5 ms | 62.7 MB/s |
| passes on SQL/every pass | 5.32 ms | 273 µs | 5.02 ms |  |
| passes on SQL/without dispatch | 6.44 ms | 308 µs | 6.00 ms |  |
| passes on SQL/without hasher | 5.62 ms | 290 µs | 5.31 ms |  |
| passes on SQL/without inline | 5.34 ms | 315 µs | 4.91 ms |  |
| passes on SQL/without left_loop | 10.4 ms | 376 µs | 10.0 ms |  |
| passes on SQL/without skip | 6.13 ms | 292 µs | 5.87 ms |  |
| passes on SQL/without span | 5.33 ms | 177 µs | 5.15 ms |  |
| passes on SQL/without split | 5.24 ms | 304 µs | 4.85 ms |  |
| passes on the grammar files/every pass | 5.06 ms | 341 µs | 4.78 ms |  |
| passes on the grammar files/without dispatch | 8.42 ms | 518 µs | 7.79 ms |  |
| passes on the grammar files/without hasher | 4.93 ms | 164 µs | 4.76 ms |  |
| passes on the grammar files/without inline | 4.95 ms | 269 µs | 4.74 ms |  |
| passes on the grammar files/without left_loop | 5.12 ms | 279 µs | 4.84 ms |  |
| passes on the grammar files/without skip | 5.60 ms | 311 µs | 5.39 ms |  |
| passes on the grammar files/without span | 5.63 ms | 363 µs | 5.29 ms |  |
| passes on the grammar files/without split | 5.00 ms | 247 µs | 4.79 ms |  |
| passes on tiny/every pass | 3.69 ms | 274 µs | 3.47 ms |  |
| passes on tiny/without dispatch | 4.89 ms | 487 µs | 4.50 ms |  |
| passes on tiny/without hasher | 4.08 ms | 434 µs | 3.83 ms |  |
| passes on tiny/without inline | 3.78 ms | 147 µs | 3.60 ms |  |
| passes on tiny/without left_loop | 10.4 ms | 733 µs | 9.59 ms |  |
| passes on tiny/without skip | 4.34 ms | 202 µs | 4.13 ms |  |
| passes on tiny/without span | 4.29 ms | 241 µs | 4.05 ms |  |
| passes on tiny/without split | 3.66 ms | 130 µs | 3.54 ms |  |
| passes on TOML/every pass | 2.90 ms | 122 µs | 2.82 ms |  |
| passes on TOML/without dispatch | 3.07 ms | 121 µs | 2.94 ms |  |
| passes on TOML/without hasher | 2.96 ms | 180 µs | 2.77 ms |  |
| passes on TOML/without inline | 2.94 ms | 123 µs | 2.82 ms |  |
| passes on TOML/without left_loop | 2.88 ms | 166 µs | 2.77 ms |  |
| passes on TOML/without skip | 3.56 ms | 158 µs | 3.29 ms |  |
| passes on TOML/without span | 3.30 ms | 131 µs | 3.18 ms |  |
| passes on TOML/without split | 2.92 ms | 118 µs | 2.82 ms |  |
| pest_vm grammar load | 609 µs | 50.7 µs | 574 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 1.79 ms | 203 µs | 1.64 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 25.1 ms | 829 µs | 23.6 ms | 4.0 MB/s |
| expressions/fpeg machine, tree | 9.83 ms | 188 µs | 9.65 ms | 10.2 MB/s |
| fpeg grammar load | 174 µs | 19.3 µs | 148 µs |  |
| fpeg machine on a small sum | 1.29 µs | 231 ns | 1.16 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.61 µs | 354 ns | 2.39 µs |  |
| json/fpeg machine with its profile, tree | 326 ms | 15.3 ms | 311 ms | 7.7 MB/s |
| json/fpeg machine, tree | 22.9 ms | 197 µs | 22.6 ms | 109.4 MB/s |
| json/pest generated, token queue | 73.9 ms | 1.60 ms | 71.4 ms | 33.9 MB/s |
| json/pest_vm interpreter, token queue | 325 ms | 8.51 ms | 318 ms | 7.7 MB/s |
| json/serde_json to Value | 8.90 ms | 316 µs | 8.42 ms | 281.5 MB/s |
| passes on CSV/every pass | 1.29 ms | 54.0 µs | 1.24 ms |  |
| passes on CSV/without dispatch | 1.84 ms | 318 µs | 1.34 ms |  |
| passes on CSV/without hasher | 1.30 ms | 87.9 µs | 1.22 ms |  |
| passes on CSV/without inline | 1.82 ms | 326 µs | 1.37 ms |  |
| passes on CSV/without left_loop | 1.38 ms | 124 µs | 1.22 ms |  |
| passes on CSV/without skip | 1.35 ms | 98.0 µs | 1.23 ms |  |
| passes on CSV/without span | 1.39 ms | 57.9 µs | 1.34 ms |  |
| passes on CSV/without split | 2.18 ms | 370 µs | 1.82 ms |  |
| passes on expressions/every pass | 10.9 ms | 920 µs | 10.1 ms | 9.2 MB/s |
| passes on expressions/without dispatch | 15.0 ms | 1.09 ms | 14.0 ms | 6.7 MB/s |
| passes on expressions/without hasher | 12.0 ms | 1.37 ms | 10.9 ms | 8.3 MB/s |
| passes on expressions/without inline | 10.2 ms | 541 µs | 9.65 ms | 9.8 MB/s |
| passes on expressions/without left_loop | 36.8 ms | 2.60 ms | 33.2 ms | 2.7 MB/s |
| passes on expressions/without skip | 11.8 ms | 788 µs | 10.9 ms | 8.5 MB/s |
| passes on expressions/without span | 11.3 ms | 533 µs | 10.6 ms | 8.8 MB/s |
| passes on expressions/without split | 10.1 ms | 517 µs | 9.52 ms | 9.9 MB/s |
| passes on JSON/every pass | 22.8 ms | 927 µs | 21.4 ms | 109.8 MB/s |
| passes on JSON/without dispatch | 26.4 ms | 2.29 ms | 24.5 ms | 94.9 MB/s |
| passes on JSON/without hasher | 24.2 ms | 1.90 ms | 21.2 ms | 103.4 MB/s |
| passes on JSON/without inline | 39.5 ms | 5.95 ms | 35.2 ms | 63.5 MB/s |
| passes on JSON/without left_loop | 22.8 ms | 1.43 ms | 21.1 ms | 110.0 MB/s |
| passes on JSON/without skip | 34.7 ms | 1.15 ms | 33.0 ms | 72.2 MB/s |
| passes on JSON/without span | 24.1 ms | 1.06 ms | 22.5 ms | 104.0 MB/s |
| passes on JSON/without split | 30.3 ms | 1.80 ms | 28.5 ms | 82.8 MB/s |
| passes on SQL/every pass | 4.50 ms | 264 µs | 4.09 ms |  |
| passes on SQL/without dispatch | 5.81 ms | 535 µs | 5.44 ms |  |
| passes on SQL/without hasher | 5.28 ms | 351 µs | 4.85 ms |  |
| passes on SQL/without inline | 4.76 ms | 399 µs | 4.34 ms |  |
| passes on SQL/without left_loop | 8.96 ms | 444 µs | 8.45 ms |  |
| passes on SQL/without skip | 5.58 ms | 664 µs | 4.90 ms |  |
| passes on SQL/without span | 5.23 ms | 244 µs | 4.97 ms |  |
| passes on SQL/without split | 4.54 ms | 289 µs | 4.18 ms |  |
| passes on the grammar files/every pass | 4.73 ms | 376 µs | 4.14 ms |  |
| passes on the grammar files/without dispatch | 8.09 ms | 589 µs | 7.53 ms |  |
| passes on the grammar files/without hasher | 4.32 ms | 164 µs | 4.14 ms |  |
| passes on the grammar files/without inline | 4.24 ms | 207 µs | 3.90 ms |  |
| passes on the grammar files/without left_loop | 4.61 ms | 529 µs | 4.02 ms |  |
| passes on the grammar files/without skip | 5.03 ms | 164 µs | 4.82 ms |  |
| passes on the grammar files/without span | 4.93 ms | 211 µs | 4.70 ms |  |
| passes on the grammar files/without split | 4.39 ms | 256 µs | 4.10 ms |  |
| passes on tiny/every pass | 3.22 ms | 205 µs | 2.90 ms |  |
| passes on tiny/without dispatch | 4.17 ms | 305 µs | 3.87 ms |  |
| passes on tiny/without hasher | 3.45 ms | 206 µs | 3.30 ms |  |
| passes on tiny/without inline | 3.31 ms | 311 µs | 3.04 ms |  |
| passes on tiny/without left_loop | 9.15 ms | 806 µs | 8.41 ms |  |
| passes on tiny/without skip | 3.66 ms | 241 µs | 3.47 ms |  |
| passes on tiny/without span | 3.98 ms | 464 µs | 3.58 ms |  |
| passes on tiny/without split | 3.15 ms | 118 µs | 3.03 ms |  |
| passes on TOML/every pass | 2.46 ms | 109 µs | 2.33 ms |  |
| passes on TOML/without dispatch | 2.63 ms | 158 µs | 2.47 ms |  |
| passes on TOML/without hasher | 2.45 ms | 106 µs | 2.36 ms |  |
| passes on TOML/without inline | 2.55 ms | 117 µs | 2.39 ms |  |
| passes on TOML/without left_loop | 2.95 ms | 318 µs | 2.62 ms |  |
| passes on TOML/without skip | 3.00 ms | 135 µs | 2.85 ms |  |
| passes on TOML/without span | 2.90 ms | 173 µs | 2.74 ms |  |
| passes on TOML/without split | 2.53 ms | 127 µs | 2.38 ms |  |
| pest_vm grammar load | 663 µs | 64.6 µs | 598 µs |  |
