# Benchmark results 2026-11-14b-linux

Written by `benchmarks/run.py` in default mode at commit 68f7020.
Times are per call.

- Machine: Linux-6.18.33.2-microsoft-standard-WSL2-x86_64-with-glibc2.36, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.8 ms | 1.17 ms | 9.23 ms | 232.1 MB/s |
| fpeg grammar load | 174 µs | 15.3 µs | 158 µs |  |
| fpeg parse, tree | 23.7 ms | 2.70 ms | 20.9 ms | 106.0 MB/s |
| fpeg parse and grammar builders, Python objects | 38.6 ms | 3.22 ms | 35.4 ms | 65.0 MB/s |
| fpeg parse and Transformer methods, Python objects | 69.5 ms | 7.38 ms | 63.2 ms | 36.1 MB/s |
| Lark grammar load | 4.80 ms | 510 µs | 4.35 ms |  |
| Lark LALR, tree | 1.16 s | 32.0 ms | 1.11 s | 2.2 MB/s |
| Lark Transformer over the tree | 341 ms | 23.7 ms | 315 ms | 7.3 MB/s |
| Lark LALR with inlined Transformer, Python objects | 934 ms | 52.0 ms | 866 ms | 2.7 MB/s |
| pe machine grammar load | 2.23 ms | 285 µs | 2.02 ms |  |
| pe machine, match only | 118 ms | 8.93 ms | 107 ms | 21.2 MB/s |
| pe machine, Python objects | 162 ms | 13.6 ms | 147 ms | 15.5 MB/s |
| pe packrat grammar load | 2.53 ms | 303 µs | 2.23 ms |  |
| pe packrat, match only | 417 ms | 22.6 ms | 397 ms | 6.0 MB/s |
| pe packrat, Python objects | 450 ms | 36.0 ms | 405 ms | 5.6 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 128 µs | 13.6 µs | 117 µs |  |
| transform, a method on every node, list | 2.03 ms | 251 µs | 1.85 ms |  |
| transform, a method on every node, *children | 1.88 ms | 160 µs | 1.75 ms |  |
| transform, the same with a member by field | 1.92 ms | 245 µs | 1.72 ms |  |
| transform, a method on every token, taking nothing | 2.36 ms | 181 µs | 2.20 ms |  |
| transform, a method on every token, taking it | 3.63 ms | 308 µs | 3.39 ms |  |
| interpreter, no method | 9.30 ms | 631 µs | 8.49 ms |  |
| interpreter, a method on every node | 10.4 ms | 487 µs | 9.73 ms |  |
| visitor, no method | 2.06 ms | 235 µs | 1.87 ms |  |
| visitor, a method on every node | 3.18 ms | 196 µs | 3.01 ms |  |
| parse | 2.41 ms | 168 µs | 2.22 ms |  |
| parse, then every view and token | 7.01 ms | 485 µs | 6.59 ms |  |
| 2000 small CSV parses with a transformer | 3.89 ms | 535 µs | 3.45 ms |  |
| 2000 small CSV parses with csv.reader | 1.35 ms | 158 µs | 1.22 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 14.5 ms | 656 µs | 13.9 ms | 6.5 MB/s |
| fpeg parse, tree | 4.96 ms | 397 µs | 4.52 ms | 19.0 MB/s |
| fpeg parse and Transformer, numbers | 7.50 ms | 537 µs | 6.93 ms | 12.5 MB/s |
| Lark LALR, tree | 97.1 ms | 8.40 ms | 87.4 ms | 1.0 MB/s |
| Lark LALR with inlined Transformer, numbers | 96.2 ms | 20.3 ms | 81.9 ms | 1.0 MB/s |
| pe machine, numbers | 39.7 ms | 4.17 ms | 35.8 ms | 2.4 MB/s |
| pe packrat, numbers | 92.2 ms | 4.10 ms | 89.1 ms | 1.0 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.76 ms | 726 µs | 7.17 ms | 253,925 lines/s |
| CPython parser and symbol table, no AST objects | 5.27 ms | 577 µs | 4.85 ms | 373,693 lines/s |
| compile to bytecode | 7.11 ms | 737 µs | 6.55 ms | 277,352 lines/s |
| tree-sitter parse, native tree | 5.99 ms | 402 µs | 5.57 ms | 329,278 lines/s |
| tree-sitter cursor walk over every node | 1.38 ms | 183 µs | 1.22 ms |  |
| tree-sitter walk, a Python Node per node | 1.86 ms | 183 µs | 1.72 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 3.87 ms | 408 µs | 3.49 ms |  |
| LibCST parse_module | 69.8 ms | 5.16 ms | 63.9 ms | 28,247 lines/s |
| Lark Python grammar load | 232 ms | 7.98 ms | 222 ms |  |
| Lark LALR, bundled Python grammar, tree | 84.3 ms | 7.75 ms | 79.0 ms | 23,389 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 519 ms | 37.8 ms | 469 ms |  |
| grammar load with a warm cache | 28.2 ms | 3.11 ms | 25.6 ms |  |
| LALR parse, tree | 152 ms | 15.3 ms | 137 ms | 12,994 lines/s |
| Transformer with the real callbacks | 18.6 ms | 3.96 ms | 15.4 ms |  |
| Transformer, trivial method on every rule | 8.24 ms | 696 µs | 7.55 ms |  |
| end to end: parse, transform, fix locations | 185 ms | 21.3 ms | 159 ms | 10,679 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.28 ms | 300 µs | 2.09 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 30.9 ms | 480 µs | 30.1 ms | 3.2 MB/s |
| expressions/fpeg machine, tree | 14.9 ms | 422 µs | 14.2 ms | 6.7 MB/s |
| fpeg grammar load | 192 µs | 24.5 µs | 169 µs |  |
| fpeg machine on a small sum | 1.19 µs | 73.9 ns | 1.08 µs |  |
| fpeg machine on a small sum, nothing remembered | 3.19 µs | 159 ns | 2.90 µs |  |
| json/fpeg machine with its profile, tree | 414 ms | 11.7 ms | 404 ms | 6.0 MB/s |
| json/fpeg machine, tree | 24.3 ms | 405 µs | 23.8 ms | 103.4 MB/s |
| json/pest generated, token queue | 38.9 ms | 1.99 ms | 36.3 ms | 64.5 MB/s |
| json/pest_vm interpreter, token queue | 277 ms | 7.64 ms | 270 ms | 9.0 MB/s |
| json/serde_json to Value | 6.96 ms | 90.3 µs | 6.90 ms | 360.0 MB/s |
| passes on CSV/every pass | 1.49 ms | 74.6 µs | 1.42 ms |  |
| passes on CSV/without dispatch | 1.54 ms | 120 µs | 1.37 ms |  |
| passes on CSV/without hasher | 1.57 ms | 238 µs | 1.43 ms |  |
| passes on CSV/without inline | 1.90 ms | 169 µs | 1.67 ms |  |
| passes on CSV/without left_loop | 1.51 ms | 93.3 µs | 1.41 ms |  |
| passes on CSV/without skip | 1.53 ms | 90.8 µs | 1.43 ms |  |
| passes on CSV/without span | 1.68 ms | 109 µs | 1.54 ms |  |
| passes on CSV/without split | 2.06 ms | 142 µs | 1.88 ms |  |
| passes on JSON/every pass | 24.3 ms | 531 µs | 23.6 ms | 103.3 MB/s |
| passes on JSON/without dispatch | 26.7 ms | 381 µs | 26.3 ms | 93.8 MB/s |
| passes on JSON/without hasher | 24.3 ms | 571 µs | 23.5 ms | 103.3 MB/s |
| passes on JSON/without inline | 44.5 ms | 2.20 ms | 43.0 ms | 56.4 MB/s |
| passes on JSON/without left_loop | 24.7 ms | 1.19 ms | 23.6 ms | 101.3 MB/s |
| passes on JSON/without skip | 34.2 ms | 621 µs | 33.4 ms | 73.4 MB/s |
| passes on JSON/without span | 25.7 ms | 542 µs | 25.0 ms | 97.7 MB/s |
| passes on JSON/without split | 35.8 ms | 803 µs | 34.8 ms | 70.1 MB/s |
| passes on SQL/every pass | 4.86 ms | 360 µs | 4.44 ms |  |
| passes on SQL/without dispatch | 6.69 ms | 119 µs | 6.50 ms |  |
| passes on SQL/without hasher | 5.72 ms | 432 µs | 5.17 ms |  |
| passes on SQL/without inline | 5.21 ms | 107 µs | 5.08 ms |  |
| passes on SQL/without left_loop | 10.5 ms | 426 µs | 9.97 ms |  |
| passes on SQL/without skip | 6.05 ms | 133 µs | 5.87 ms |  |
| passes on SQL/without span | 5.84 ms | 99.1 µs | 5.73 ms |  |
| passes on SQL/without split | 5.59 ms | 712 µs | 4.63 ms |  |
| passes on TOML/every pass | 3.07 ms | 164 µs | 2.89 ms |  |
| passes on TOML/without dispatch | 3.16 ms | 53.2 µs | 3.09 ms |  |
| passes on TOML/without hasher | 2.99 ms | 84.2 µs | 2.91 ms |  |
| passes on TOML/without inline | 2.99 ms | 41.9 µs | 2.92 ms |  |
| passes on TOML/without left_loop | 2.97 ms | 280 µs | 2.67 ms |  |
| passes on TOML/without skip | 3.58 ms | 69.8 µs | 3.52 ms |  |
| passes on TOML/without span | 3.46 ms | 55.6 µs | 3.39 ms |  |
| passes on TOML/without split | 2.94 ms | 158 µs | 2.77 ms |  |
| passes on expressions/every pass | 12.0 ms | 473 µs | 11.6 ms | 8.3 MB/s |
| passes on expressions/without dispatch | 18.2 ms | 919 µs | 17.3 ms | 5.5 MB/s |
| passes on expressions/without hasher | 13.0 ms | 333 µs | 12.6 ms | 7.7 MB/s |
| passes on expressions/without inline | 12.1 ms | 327 µs | 11.8 ms | 8.3 MB/s |
| passes on expressions/without left_loop | 34.2 ms | 3.16 ms | 31.5 ms | 2.9 MB/s |
| passes on expressions/without skip | 13.3 ms | 287 µs | 13.0 ms | 7.5 MB/s |
| passes on expressions/without span | 12.7 ms | 135 µs | 12.4 ms | 7.9 MB/s |
| passes on expressions/without split | 11.9 ms | 879 µs | 10.8 ms | 8.4 MB/s |
| passes on the grammar files/every pass | 4.91 ms | 146 µs | 4.74 ms |  |
| passes on the grammar files/without dispatch | 9.15 ms | 797 µs | 8.38 ms |  |
| passes on the grammar files/without hasher | 5.25 ms | 884 µs | 4.62 ms |  |
| passes on the grammar files/without inline | 4.78 ms | 202 µs | 4.54 ms |  |
| passes on the grammar files/without left_loop | 5.21 ms | 515 µs | 4.71 ms |  |
| passes on the grammar files/without skip | 6.56 ms | 1.38 ms | 5.19 ms |  |
| passes on the grammar files/without span | 5.56 ms | 255 µs | 5.31 ms |  |
| passes on the grammar files/without split | 4.96 ms | 253 µs | 4.71 ms |  |
| passes on tiny/every pass | 3.80 ms | 135 µs | 3.56 ms |  |
| passes on tiny/without dispatch | 5.18 ms | 136 µs | 4.97 ms |  |
| passes on tiny/without hasher | 4.08 ms | 83.4 µs | 3.99 ms |  |
| passes on tiny/without inline | 3.87 ms | 88.5 µs | 3.79 ms |  |
| passes on tiny/without left_loop | 8.94 ms | 223 µs | 8.70 ms |  |
| passes on tiny/without skip | 4.30 ms | 64.9 µs | 4.21 ms |  |
| passes on tiny/without span | 4.56 ms | 111 µs | 4.43 ms |  |
| passes on tiny/without split | 3.83 ms | 150 µs | 3.65 ms |  |
| pest_vm grammar load | 418 µs | 23.4 µs | 368 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 1.95 ms | 78.5 µs | 1.79 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 26.7 ms | 1.17 ms | 24.9 ms | 3.8 MB/s |
| expressions/fpeg machine, tree | 11.9 ms | 539 µs | 11.3 ms | 8.4 MB/s |
| fpeg grammar load | 167 µs | 51.3 µs | 134 µs |  |
| fpeg machine on a small sum | 1.04 µs | 112 ns | 944 ns |  |
| fpeg machine on a small sum, nothing remembered | 2.89 µs | 285 ns | 2.67 µs |  |
| json/fpeg machine with its profile, tree | 380 ms | 34.0 ms | 342 ms | 6.6 MB/s |
| json/fpeg machine, tree | 17.9 ms | 888 µs | 17.2 ms | 139.9 MB/s |
| json/pest generated, token queue | 60.9 ms | 4.70 ms | 56.0 ms | 41.1 MB/s |
| json/pest_vm interpreter, token queue | 284 ms | 11.9 ms | 269 ms | 8.8 MB/s |
| json/serde_json to Value | 6.87 ms | 480 µs | 6.45 ms | 365.0 MB/s |
| passes on CSV/every pass | 1.21 ms | 110 µs | 1.12 ms |  |
| passes on CSV/without dispatch | 1.25 ms | 15.0 µs | 1.23 ms |  |
| passes on CSV/without hasher | 1.25 ms | 18.7 µs | 1.22 ms |  |
| passes on CSV/without inline | 1.36 ms | 26.2 µs | 1.32 ms |  |
| passes on CSV/without left_loop | 1.24 ms | 23.4 µs | 1.22 ms |  |
| passes on CSV/without skip | 1.25 ms | 30.4 µs | 1.22 ms |  |
| passes on CSV/without span | 1.39 ms | 33.5 µs | 1.34 ms |  |
| passes on CSV/without split | 1.59 ms | 20.4 µs | 1.56 ms |  |
| passes on JSON/every pass | 18.5 ms | 1.94 ms | 16.4 ms | 135.4 MB/s |
| passes on JSON/without dispatch | 21.5 ms | 2.49 ms | 19.4 ms | 116.5 MB/s |
| passes on JSON/without hasher | 17.5 ms | 1.11 ms | 16.7 ms | 143.2 MB/s |
| passes on JSON/without inline | 33.1 ms | 2.82 ms | 30.1 ms | 75.7 MB/s |
| passes on JSON/without left_loop | 18.6 ms | 374 µs | 18.1 ms | 135.1 MB/s |
| passes on JSON/without skip | 29.0 ms | 1.46 ms | 27.7 ms | 86.4 MB/s |
| passes on JSON/without span | 19.2 ms | 1.26 ms | 17.7 ms | 130.8 MB/s |
| passes on JSON/without split | 29.7 ms | 3.69 ms | 27.5 ms | 84.4 MB/s |
| passes on SQL/every pass | 4.67 ms | 234 µs | 4.41 ms |  |
| passes on SQL/without dispatch | 5.85 ms | 136 µs | 5.70 ms |  |
| passes on SQL/without hasher | 4.75 ms | 72.6 µs | 4.65 ms |  |
| passes on SQL/without inline | 4.99 ms | 533 µs | 4.16 ms |  |
| passes on SQL/without left_loop | 9.20 ms | 153 µs | 9.03 ms |  |
| passes on SQL/without skip | 5.59 ms | 243 µs | 5.30 ms |  |
| passes on SQL/without span | 4.82 ms | 78.9 µs | 4.70 ms |  |
| passes on SQL/without split | 4.80 ms | 85.7 µs | 4.68 ms |  |
| passes on TOML/every pass | 2.33 ms | 130 µs | 2.13 ms |  |
| passes on TOML/without dispatch | 2.63 ms | 153 µs | 2.44 ms |  |
| passes on TOML/without hasher | 2.35 ms | 178 µs | 2.14 ms |  |
| passes on TOML/without inline | 2.33 ms | 68.8 µs | 2.24 ms |  |
| passes on TOML/without left_loop | 2.38 ms | 298 µs | 2.12 ms |  |
| passes on TOML/without skip | 3.01 ms | 339 µs | 2.56 ms |  |
| passes on TOML/without span | 2.43 ms | 122 µs | 2.31 ms |  |
| passes on TOML/without split | 2.37 ms | 205 µs | 2.19 ms |  |
| passes on expressions/every pass | 10.4 ms | 116 µs | 10.3 ms | 9.6 MB/s |
| passes on expressions/without dispatch | 16.7 ms | 2.41 ms | 14.1 ms | 6.0 MB/s |
| passes on expressions/without hasher | 10.6 ms | 127 µs | 10.5 ms | 9.4 MB/s |
| passes on expressions/without inline | 10.8 ms | 116 µs | 10.7 ms | 9.3 MB/s |
| passes on expressions/without left_loop | 33.6 ms | 5.68 ms | 27.6 ms | 3.0 MB/s |
| passes on expressions/without skip | 12.1 ms | 205 µs | 11.8 ms | 8.3 MB/s |
| passes on expressions/without span | 10.2 ms | 180 µs | 9.99 ms | 9.8 MB/s |
| passes on expressions/without split | 10.7 ms | 182 µs | 10.4 ms | 9.4 MB/s |
| passes on the grammar files/every pass | 4.01 ms | 173 µs | 3.76 ms |  |
| passes on the grammar files/without dispatch | 7.14 ms | 370 µs | 6.74 ms |  |
| passes on the grammar files/without hasher | 3.94 ms | 188 µs | 3.74 ms |  |
| passes on the grammar files/without inline | 3.95 ms | 205 µs | 3.71 ms |  |
| passes on the grammar files/without left_loop | 4.11 ms | 252 µs | 3.73 ms |  |
| passes on the grammar files/without skip | 4.61 ms | 328 µs | 4.19 ms |  |
| passes on the grammar files/without span | 4.35 ms | 274 µs | 4.09 ms |  |
| passes on the grammar files/without split | 5.45 ms | 1.72 ms | 3.85 ms |  |
| passes on tiny/every pass | 3.56 ms | 72.6 µs | 3.48 ms |  |
| passes on tiny/without dispatch | 4.42 ms | 91.8 µs | 4.28 ms |  |
| passes on tiny/without hasher | 3.45 ms | 85.2 µs | 3.33 ms |  |
| passes on tiny/without inline | 3.43 ms | 46.3 µs | 3.38 ms |  |
| passes on tiny/without left_loop | 7.29 ms | 392 µs | 6.82 ms |  |
| passes on tiny/without skip | 3.99 ms | 100 µs | 3.86 ms |  |
| passes on tiny/without span | 3.57 ms | 60.0 µs | 3.49 ms |  |
| passes on tiny/without split | 3.41 ms | 74.8 µs | 3.32 ms |  |
| pest_vm grammar load | 423 µs | 73.0 µs | 364 µs |  |
