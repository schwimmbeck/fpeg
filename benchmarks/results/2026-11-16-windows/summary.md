# Benchmark results 2026-11-16-windows

Written by `benchmarks/run.py` in default mode at commit 9a1f830.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 11.2 ms | 904 µs | 10.1 ms | 223.1 MB/s |
| fpeg grammar load | 301 µs | 17.3 µs | 285 µs |  |
| fpeg parse, tree | 32.8 ms | 2.08 ms | 30.9 ms | 76.4 MB/s |
| fpeg parse and grammar builders, Python objects | 50.7 ms | 2.07 ms | 47.6 ms | 49.4 MB/s |
| fpeg parse and Transformer methods, Python objects | 86.9 ms | 6.41 ms | 81.1 ms | 28.8 MB/s |
| Lark grammar load | 5.32 ms | 380 µs | 5.00 ms |  |
| Lark LALR, tree | 1.23 s | 48.5 ms | 1.18 s | 2.0 MB/s |
| Lark Transformer over the tree | 386 ms | 18.1 ms | 360 ms | 6.5 MB/s |
| Lark LALR with inlined Transformer, Python objects | 1.01 s | 33.9 ms | 979 ms | 2.5 MB/s |
| pe machine grammar load | 2.46 ms | 114 µs | 2.36 ms |  |
| pe machine, match only | 145 ms | 14.0 ms | 132 ms | 17.3 MB/s |
| pe machine, Python objects | 196 ms | 14.1 ms | 178 ms | 12.8 MB/s |
| pe packrat grammar load | 2.75 ms | 149 µs | 2.57 ms |  |
| pe packrat, match only | 490 ms | 51.7 ms | 431 ms | 5.1 MB/s |
| pe packrat, Python objects | 494 ms | 20.2 ms | 465 ms | 5.1 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 145 µs | 14.0 µs | 133 µs |  |
| transform, a method on every node, list | 2.39 ms | 205 µs | 2.16 ms |  |
| transform, a method on every node, *children | 2.29 ms | 115 µs | 2.12 ms |  |
| transform, the same with a member by field | 2.23 ms | 148 µs | 2.06 ms |  |
| transform, a method on every token, taking nothing | 2.50 ms | 94.3 µs | 2.40 ms |  |
| transform, a method on every token, taking it | 3.79 ms | 151 µs | 3.67 ms |  |
| interpreter, no method | 9.62 ms | 257 µs | 9.28 ms |  |
| interpreter, a method on every node | 11.1 ms | 516 µs | 10.6 ms |  |
| visitor, no method | 2.18 ms | 68.7 µs | 2.12 ms |  |
| visitor, a method on every node | 3.30 ms | 92.8 µs | 3.16 ms |  |
| parse | 3.33 ms | 142 µs | 3.18 ms |  |
| parse, then every view and token | 8.30 ms | 347 µs | 8.01 ms |  |
| 2000 small CSV parses with a transformer | 4.89 ms | 187 µs | 4.66 ms |  |
| 2000 small CSV parses with csv.reader | 1.66 ms | 57.3 µs | 1.61 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 17.6 ms | 631 µs | 16.9 ms | 5.4 MB/s |
| fpeg parse, tree | 5.17 ms | 69.6 µs | 5.06 ms | 18.2 MB/s |
| fpeg parse and Transformer, numbers | 8.23 ms | 275 µs | 7.93 ms | 11.4 MB/s |
| Lark LALR, tree | 101 ms | 4.59 ms | 95.8 ms | 0.9 MB/s |
| Lark LALR with inlined Transformer, numbers | 96.6 ms | 3.34 ms | 92.7 ms | 1.0 MB/s |
| pe machine, numbers | 47.4 ms | 1.58 ms | 45.4 ms | 2.0 MB/s |
| pe packrat, numbers | 103 ms | 2.50 ms | 101 ms | 0.9 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.46 ms | 101 µs | 7.32 ms | 264,265 lines/s |
| fpeg Python grammar load | 5.05 ms | 187 µs | 4.74 ms |  |
| fpeg parse, tree | 7.87 ms | 409 µs | 7.43 ms | 250,564 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 17.5 ms | 856 µs | 16.5 ms | 112,790 lines/s |
| fpeg parse and transform to ast, stamped natively, each method in a wrapper | 20.3 ms | 975 µs | 19.3 ms | 97,168 lines/s |
| fpeg parse and transform to ast, positions set in Python by that wrapper | 21.2 ms | 1.29 ms | 20.2 ms | 92,757 lines/s |
| CPython parser and symbol table, no AST objects | 5.63 ms | 266 µs | 5.29 ms | 350,058 lines/s |
| compile to bytecode | 8.77 ms | 434 µs | 8.32 ms | 224,861 lines/s |
| tree-sitter parse, native tree | 7.03 ms | 289 µs | 6.78 ms | 280,370 lines/s |
| tree-sitter cursor walk over every node | 2.39 ms | 108 µs | 2.29 ms |  |
| tree-sitter walk, a Python Node per node | 2.91 ms | 95.5 µs | 2.79 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.77 ms | 177 µs | 4.61 ms |  |
| LibCST parse_module | 81.7 ms | 2.81 ms | 78.9 ms | 24,123 lines/s |
| Lark Python grammar load | 245 ms | 9.18 ms | 235 ms |  |
| Lark LALR, bundled Python grammar, tree | 85.7 ms | 4.54 ms | 81.2 ms | 22,988 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 528 ms | 12.7 ms | 513 ms |  |
| grammar load with a warm cache | 29.0 ms | 1.52 ms | 27.5 ms |  |
| LALR parse, tree | 147 ms | 8.72 ms | 137 ms | 13,427 lines/s |
| Transformer with the real callbacks | 17.7 ms | 367 µs | 17.1 ms |  |
| Transformer, trivial method on every rule | 9.12 ms | 288 µs | 8.84 ms |  |
| end to end: parse, transform, fix locations | 171 ms | 6.18 ms | 164 ms | 11,517 lines/s |
| fpeg ORD grammar load | 5.69 ms | 134 µs | 5.38 ms |  |
| fpeg parse, tree | 8.60 ms | 399 µs | 8.17 ms | 229,139 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 18.7 ms | 1.11 ms | 17.5 ms | 105,204 lines/s |
| fpeg end to end: parse, transform, fix locations | 22.9 ms | 721 µs | 22.1 ms | 85,988 lines/s |
| end to end over the 76 ORD files | 637 ms | 20.6 ms | 600 ms |  |
| fpeg end to end over the 76 ORD files | 132 ms | 6.95 ms | 125 ms |  |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.22 ms | 221 µs | 2.02 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 28.1 ms | 773 µs | 27.0 ms | 3.6 MB/s |
| expressions/fpeg machine, tree | 12.2 ms | 528 µs | 11.8 ms | 8.2 MB/s |
| fpeg grammar load | 304 µs | 32.7 µs | 276 µs |  |
| fpeg machine on a small sum | 1.50 µs | 130 ns | 1.35 µs |  |
| fpeg machine on a small sum, nothing remembered | 3.22 µs | 364 ns | 2.93 µs |  |
| json/fpeg machine with its profile, tree | 362 ms | 8.88 ms | 349 ms | 6.9 MB/s |
| json/fpeg machine, tree | 32.4 ms | 1.43 ms | 30.6 ms | 77.3 MB/s |
| json/pest generated, token queue | 57.8 ms | 2.10 ms | 55.2 ms | 43.4 MB/s |
| json/pest_vm interpreter, token queue | 313 ms | 9.36 ms | 299 ms | 8.0 MB/s |
| json/serde_json to Value | 9.79 ms | 902 µs | 8.83 ms | 256.0 MB/s |
| passes on CSV/every pass | 2.05 ms | 144 µs | 1.87 ms |  |
| passes on CSV/without dispatch | 1.93 ms | 52.2 µs | 1.88 ms |  |
| passes on CSV/without hasher | 1.91 ms | 93.2 µs | 1.82 ms |  |
| passes on CSV/without inline | 2.22 ms | 96.5 µs | 2.10 ms |  |
| passes on CSV/without left_loop | 1.94 ms | 105 µs | 1.81 ms |  |
| passes on CSV/without skip | 1.88 ms | 58.4 µs | 1.82 ms |  |
| passes on CSV/without span | 2.27 ms | 138 µs | 2.11 ms |  |
| passes on CSV/without split | 2.30 ms | 72.2 µs | 2.19 ms |  |
| passes on expressions/every pass | 12.4 ms | 670 µs | 11.8 ms | 8.1 MB/s |
| passes on expressions/without dispatch | 19.4 ms | 1.31 ms | 18.0 ms | 5.2 MB/s |
| passes on expressions/without hasher | 13.0 ms | 524 µs | 12.4 ms | 7.7 MB/s |
| passes on expressions/without inline | 12.2 ms | 818 µs | 11.5 ms | 8.2 MB/s |
| passes on expressions/without left_loop | 48.6 ms | 6.58 ms | 41.6 ms | 2.1 MB/s |
| passes on expressions/without skip | 14.3 ms | 808 µs | 13.5 ms | 7.0 MB/s |
| passes on expressions/without span | 13.3 ms | 1.50 ms | 11.8 ms | 7.5 MB/s |
| passes on expressions/without split | 12.4 ms | 1.03 ms | 11.2 ms | 8.1 MB/s |
| passes on JSON/every pass | 30.7 ms | 643 µs | 29.6 ms | 81.6 MB/s |
| passes on JSON/without dispatch | 34.3 ms | 2.98 ms | 31.9 ms | 73.0 MB/s |
| passes on JSON/without hasher | 34.2 ms | 2.16 ms | 31.2 ms | 73.4 MB/s |
| passes on JSON/without inline | 48.3 ms | 3.18 ms | 45.1 ms | 51.9 MB/s |
| passes on JSON/without left_loop | 33.0 ms | 2.36 ms | 30.2 ms | 76.0 MB/s |
| passes on JSON/without skip | 41.6 ms | 780 µs | 40.3 ms | 60.2 MB/s |
| passes on JSON/without span | 33.9 ms | 2.58 ms | 31.2 ms | 74.0 MB/s |
| passes on JSON/without split | 42.5 ms | 1.88 ms | 40.3 ms | 59.0 MB/s |
| passes on Lua/every pass | 7.80 ms | 602 µs | 7.07 ms |  |
| passes on Lua/without dispatch | 11.6 ms | 471 µs | 11.0 ms |  |
| passes on Lua/without hasher | 8.08 ms | 423 µs | 7.56 ms |  |
| passes on Lua/without inline | 7.22 ms | 261 µs | 6.88 ms |  |
| passes on Lua/without left_loop | 19.0 ms | 733 µs | 18.3 ms |  |
| passes on Lua/without skip | 8.37 ms | 387 µs | 8.04 ms |  |
| passes on Lua/without span | 7.45 ms | 157 µs | 7.25 ms |  |
| passes on Lua/without split | 7.18 ms | 279 µs | 6.65 ms |  |
| passes on Python/every pass | 7.68 ms | 280 µs | 7.33 ms | 256,745 lines/s |
| passes on Python/without dispatch | 12.0 ms | 424 µs | 11.5 ms | 164,379 lines/s |
| passes on Python/without hasher | 8.82 ms | 348 µs | 8.22 ms | 223,405 lines/s |
| passes on Python/without inline | 7.88 ms | 542 µs | 7.30 ms | 250,108 lines/s |
| passes on Python/without left_loop | 14.3 ms | 536 µs | 13.4 ms | 137,678 lines/s |
| passes on Python/without skip | 9.27 ms | 372 µs | 8.78 ms | 212,662 lines/s |
| passes on Python/without span | 7.88 ms | 274 µs | 7.63 ms | 249,975 lines/s |
| passes on Python/without split | 7.96 ms | 406 µs | 7.32 ms | 247,752 lines/s |
| passes on SQL/every pass | 5.35 ms | 251 µs | 5.13 ms |  |
| passes on SQL/without dispatch | 7.01 ms | 678 µs | 6.35 ms |  |
| passes on SQL/without hasher | 5.83 ms | 210 µs | 5.46 ms |  |
| passes on SQL/without inline | 5.17 ms | 163 µs | 5.02 ms |  |
| passes on SQL/without left_loop | 11.3 ms | 493 µs | 10.7 ms |  |
| passes on SQL/without skip | 6.46 ms | 448 µs | 5.93 ms |  |
| passes on SQL/without span | 5.66 ms | 285 µs | 5.36 ms |  |
| passes on SQL/without split | 5.18 ms | 203 µs | 4.93 ms |  |
| passes on the grammar files/every pass | 6.40 ms | 331 µs | 5.97 ms |  |
| passes on the grammar files/without dispatch | 10.3 ms | 564 µs | 9.63 ms |  |
| passes on the grammar files/without hasher | 6.17 ms | 623 µs | 5.61 ms |  |
| passes on the grammar files/without inline | 5.97 ms | 366 µs | 5.56 ms |  |
| passes on the grammar files/without left_loop | 6.03 ms | 278 µs | 5.63 ms |  |
| passes on the grammar files/without skip | 6.87 ms | 421 µs | 6.40 ms |  |
| passes on the grammar files/without span | 7.11 ms | 628 µs | 6.14 ms |  |
| passes on the grammar files/without split | 6.18 ms | 613 µs | 5.58 ms |  |
| passes on the outline language/every pass | 1.50 ms | 61.8 µs | 1.44 ms |  |
| passes on the outline language/without dispatch | 1.46 ms | 38.3 µs | 1.40 ms |  |
| passes on the outline language/without hasher | 1.55 ms | 47.6 µs | 1.49 ms |  |
| passes on the outline language/without inline | 1.62 ms | 58.0 µs | 1.53 ms |  |
| passes on the outline language/without left_loop | 1.59 ms | 74.2 µs | 1.47 ms |  |
| passes on the outline language/without skip | 2.06 ms | 114 µs | 1.98 ms |  |
| passes on the outline language/without span | 1.94 ms | 37.2 µs | 1.90 ms |  |
| passes on the outline language/without split | 1.53 ms | 76.4 µs | 1.46 ms |  |
| passes on tiny/every pass | 3.85 ms | 123 µs | 3.73 ms |  |
| passes on tiny/without dispatch | 5.23 ms | 332 µs | 4.84 ms |  |
| passes on tiny/without hasher | 4.24 ms | 303 µs | 3.91 ms |  |
| passes on tiny/without inline | 3.94 ms | 203 µs | 3.65 ms |  |
| passes on tiny/without left_loop | 10.3 ms | 420 µs | 9.76 ms |  |
| passes on tiny/without skip | 4.64 ms | 226 µs | 4.43 ms |  |
| passes on tiny/without span | 4.64 ms | 218 µs | 4.31 ms |  |
| passes on tiny/without split | 3.89 ms | 179 µs | 3.69 ms |  |
| passes on TOML/every pass | 2.72 ms | 138 µs | 2.56 ms |  |
| passes on TOML/without dispatch | 2.94 ms | 101 µs | 2.81 ms |  |
| passes on TOML/without hasher | 2.71 ms | 147 µs | 2.52 ms |  |
| passes on TOML/without inline | 2.73 ms | 214 µs | 2.52 ms |  |
| passes on TOML/without left_loop | 2.93 ms | 297 µs | 2.60 ms |  |
| passes on TOML/without skip | 3.33 ms | 182 µs | 3.16 ms |  |
| passes on TOML/without span | 3.11 ms | 185 µs | 2.86 ms |  |
| passes on TOML/without split | 2.98 ms | 219 µs | 2.73 ms |  |
| pest_vm grammar load | 648 µs | 65.1 µs | 580 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 1.79 ms | 117 µs | 1.68 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 24.2 ms | 474 µs | 23.5 ms | 4.1 MB/s |
| expressions/fpeg machine, tree | 10.3 ms | 292 µs | 9.92 ms | 9.8 MB/s |
| fpeg grammar load | 281 µs | 29.8 µs | 255 µs |  |
| fpeg machine on a small sum | 1.29 µs | 111 ns | 1.16 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.70 µs | 337 ns | 2.36 µs |  |
| json/fpeg machine with its profile, tree | 351 ms | 14.9 ms | 338 ms | 7.1 MB/s |
| json/fpeg machine, tree | 24.2 ms | 964 µs | 22.9 ms | 103.8 MB/s |
| json/pest generated, token queue | 75.1 ms | 3.61 ms | 69.5 ms | 33.4 MB/s |
| json/pest_vm interpreter, token queue | 337 ms | 10.8 ms | 326 ms | 7.4 MB/s |
| json/serde_json to Value | 9.03 ms | 523 µs | 8.52 ms | 277.5 MB/s |
| passes on CSV/every pass | 1.50 ms | 52.8 µs | 1.40 ms |  |
| passes on CSV/without dispatch | 1.48 ms | 94.8 µs | 1.36 ms |  |
| passes on CSV/without hasher | 1.53 ms | 112 µs | 1.40 ms |  |
| passes on CSV/without inline | 1.76 ms | 95.9 µs | 1.67 ms |  |
| passes on CSV/without left_loop | 1.53 ms | 97.8 µs | 1.44 ms |  |
| passes on CSV/without skip | 1.57 ms | 140 µs | 1.47 ms |  |
| passes on CSV/without span | 1.70 ms | 88.5 µs | 1.60 ms |  |
| passes on CSV/without split | 1.82 ms | 48.9 µs | 1.76 ms |  |
| passes on expressions/every pass | 10.1 ms | 440 µs | 9.83 ms | 9.9 MB/s |
| passes on expressions/without dispatch | 16.1 ms | 547 µs | 15.4 ms | 6.2 MB/s |
| passes on expressions/without hasher | 10.3 ms | 266 µs | 10.0 ms | 9.7 MB/s |
| passes on expressions/without inline | 10.1 ms | 279 µs | 9.80 ms | 9.9 MB/s |
| passes on expressions/without left_loop | 33.8 ms | 1.26 ms | 31.8 ms | 3.0 MB/s |
| passes on expressions/without skip | 12.5 ms | 589 µs | 11.7 ms | 8.0 MB/s |
| passes on expressions/without span | 10.6 ms | 425 µs | 9.89 ms | 9.4 MB/s |
| passes on expressions/without split | 9.99 ms | 279 µs | 9.61 ms | 10.0 MB/s |
| passes on JSON/every pass | 24.0 ms | 834 µs | 23.2 ms | 104.4 MB/s |
| passes on JSON/without dispatch | 26.9 ms | 816 µs | 25.7 ms | 93.3 MB/s |
| passes on JSON/without hasher | 24.2 ms | 1.16 ms | 22.7 ms | 103.8 MB/s |
| passes on JSON/without inline | 38.2 ms | 1.29 ms | 36.6 ms | 65.6 MB/s |
| passes on JSON/without left_loop | 23.2 ms | 1.02 ms | 22.0 ms | 107.9 MB/s |
| passes on JSON/without skip | 38.4 ms | 1.08 ms | 37.2 ms | 65.3 MB/s |
| passes on JSON/without span | 24.8 ms | 914 µs | 23.3 ms | 101.2 MB/s |
| passes on JSON/without split | 30.9 ms | 1.12 ms | 29.8 ms | 81.0 MB/s |
| passes on Lua/every pass | 6.14 ms | 192 µs | 5.94 ms |  |
| passes on Lua/without dispatch | 10.0 ms | 418 µs | 9.59 ms |  |
| passes on Lua/without hasher | 6.79 ms | 344 µs | 6.13 ms |  |
| passes on Lua/without inline | 6.25 ms | 203 µs | 6.04 ms |  |
| passes on Lua/without left_loop | 15.8 ms | 923 µs | 14.8 ms |  |
| passes on Lua/without skip | 7.22 ms | 366 µs | 6.73 ms |  |
| passes on Lua/without span | 6.36 ms | 464 µs | 6.05 ms |  |
| passes on Lua/without split | 6.14 ms | 183 µs | 5.94 ms |  |
| passes on Python/every pass | 6.73 ms | 248 µs | 6.42 ms | 292,734 lines/s |
| passes on Python/without dispatch | 10.6 ms | 251 µs | 10.4 ms | 185,212 lines/s |
| passes on Python/without hasher | 7.24 ms | 225 µs | 6.99 ms | 272,261 lines/s |
| passes on Python/without inline | 6.89 ms | 256 µs | 6.53 ms | 286,079 lines/s |
| passes on Python/without left_loop | 12.4 ms | 413 µs | 11.8 ms | 159,379 lines/s |
| passes on Python/without skip | 8.18 ms | 540 µs | 7.58 ms | 240,834 lines/s |
| passes on Python/without span | 7.19 ms | 414 µs | 6.74 ms | 274,036 lines/s |
| passes on Python/without split | 6.96 ms | 322 µs | 6.55 ms | 283,044 lines/s |
| passes on SQL/every pass | 4.48 ms | 154 µs | 4.28 ms |  |
| passes on SQL/without dispatch | 5.91 ms | 217 µs | 5.71 ms |  |
| passes on SQL/without hasher | 4.72 ms | 184 µs | 4.42 ms |  |
| passes on SQL/without inline | 4.68 ms | 186 µs | 4.38 ms |  |
| passes on SQL/without left_loop | 9.05 ms | 291 µs | 8.56 ms |  |
| passes on SQL/without skip | 5.53 ms | 208 µs | 5.18 ms |  |
| passes on SQL/without span | 4.84 ms | 208 µs | 4.60 ms |  |
| passes on SQL/without split | 4.71 ms | 254 µs | 4.26 ms |  |
| passes on the grammar files/every pass | 4.83 ms | 183 µs | 4.58 ms |  |
| passes on the grammar files/without dispatch | 9.40 ms | 353 µs | 9.02 ms |  |
| passes on the grammar files/without hasher | 4.84 ms | 141 µs | 4.65 ms |  |
| passes on the grammar files/without inline | 4.96 ms | 351 µs | 4.59 ms |  |
| passes on the grammar files/without left_loop | 4.82 ms | 241 µs | 4.48 ms |  |
| passes on the grammar files/without skip | 5.84 ms | 251 µs | 5.55 ms |  |
| passes on the grammar files/without span | 5.51 ms | 274 µs | 5.24 ms |  |
| passes on the grammar files/without split | 4.89 ms | 179 µs | 4.67 ms |  |
| passes on the outline language/every pass | 1.19 ms | 61.7 µs | 1.12 ms |  |
| passes on the outline language/without dispatch | 1.19 ms | 47.7 µs | 1.15 ms |  |
| passes on the outline language/without hasher | 1.22 ms | 91.3 µs | 1.12 ms |  |
| passes on the outline language/without inline | 1.22 ms | 33.7 µs | 1.17 ms |  |
| passes on the outline language/without left_loop | 1.21 ms | 70.5 µs | 1.13 ms |  |
| passes on the outline language/without skip | 1.64 ms | 99.2 µs | 1.55 ms |  |
| passes on the outline language/without span | 1.70 ms | 32.5 µs | 1.67 ms |  |
| passes on the outline language/without split | 1.21 ms | 81.3 µs | 1.13 ms |  |
| passes on tiny/every pass | 3.40 ms | 192 µs | 3.07 ms |  |
| passes on tiny/without dispatch | 4.57 ms | 251 µs | 4.25 ms |  |
| passes on tiny/without hasher | 3.44 ms | 157 µs | 3.19 ms |  |
| passes on tiny/without inline | 3.42 ms | 174 µs | 3.21 ms |  |
| passes on tiny/without left_loop | 8.21 ms | 298 µs | 7.81 ms |  |
| passes on tiny/without skip | 3.98 ms | 162 µs | 3.74 ms |  |
| passes on tiny/without span | 3.66 ms | 143 µs | 3.53 ms |  |
| passes on tiny/without split | 3.38 ms | 269 µs | 3.12 ms |  |
| passes on TOML/every pass | 2.30 ms | 99.5 µs | 2.17 ms |  |
| passes on TOML/without dispatch | 2.55 ms | 125 µs | 2.40 ms |  |
| passes on TOML/without hasher | 2.27 ms | 71.0 µs | 2.15 ms |  |
| passes on TOML/without inline | 2.38 ms | 120 µs | 2.25 ms |  |
| passes on TOML/without left_loop | 2.27 ms | 71.5 µs | 2.18 ms |  |
| passes on TOML/without skip | 2.96 ms | 170 µs | 2.74 ms |  |
| passes on TOML/without span | 2.62 ms | 99.7 µs | 2.50 ms |  |
| passes on TOML/without split | 2.38 ms | 88.5 µs | 2.29 ms |  |
| pest_vm grammar load | 656 µs | 66.7 µs | 590 µs |  |
