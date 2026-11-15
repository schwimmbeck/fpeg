# Benchmark results 2026-11-15b-windows

Written by `benchmarks/run.py` in default mode at commit 58e5100.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 11.3 ms | 294 µs | 10.9 ms | 221.3 MB/s |
| fpeg grammar load | 193 µs | 15.2 µs | 176 µs |  |
| fpeg parse, tree | 32.2 ms | 1.20 ms | 30.8 ms | 77.8 MB/s |
| fpeg parse and grammar builders, Python objects | 51.3 ms | 2.98 ms | 45.0 ms | 48.9 MB/s |
| fpeg parse and Transformer methods, Python objects | 93.3 ms | 4.62 ms | 86.8 ms | 26.9 MB/s |
| Lark grammar load | 5.56 ms | 290 µs | 5.18 ms |  |
| Lark LALR, tree | 1.35 s | 60.5 ms | 1.22 s | 1.9 MB/s |
| Lark Transformer over the tree | 413 ms | 21.7 ms | 386 ms | 6.1 MB/s |
| Lark LALR with inlined Transformer, Python objects | 1.03 s | 22.6 ms | 1.01 s | 2.4 MB/s |
| pe machine grammar load | 2.55 ms | 150 µs | 2.45 ms |  |
| pe machine, match only | 152 ms | 13.1 ms | 141 ms | 16.5 MB/s |
| pe machine, Python objects | 211 ms | 14.0 ms | 196 ms | 11.9 MB/s |
| pe packrat grammar load | 2.76 ms | 95.0 µs | 2.66 ms |  |
| pe packrat, match only | 476 ms | 10.4 ms | 464 ms | 5.3 MB/s |
| pe packrat, Python objects | 496 ms | 10.8 ms | 483 ms | 5.1 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 146 µs | 16.2 µs | 134 µs |  |
| transform, a method on every node, list | 2.32 ms | 208 µs | 2.09 ms |  |
| transform, a method on every node, *children | 2.32 ms | 289 µs | 2.02 ms |  |
| transform, the same with a member by field | 2.22 ms | 253 µs | 2.04 ms |  |
| transform, a method on every token, taking nothing | 2.48 ms | 131 µs | 2.36 ms |  |
| transform, a method on every token, taking it | 3.90 ms | 399 µs | 3.58 ms |  |
| interpreter, no method | 9.85 ms | 528 µs | 9.14 ms |  |
| interpreter, a method on every node | 11.2 ms | 694 µs | 10.4 ms |  |
| visitor, no method | 2.22 ms | 211 µs | 2.06 ms |  |
| visitor, a method on every node | 3.41 ms | 213 µs | 3.18 ms |  |
| parse | 3.09 ms | 156 µs | 2.85 ms |  |
| parse, then every view and token | 8.84 ms | 989 µs | 7.94 ms |  |
| 2000 small CSV parses with a transformer | 4.99 ms | 373 µs | 4.54 ms |  |
| 2000 small CSV parses with csv.reader | 1.69 ms | 119 µs | 1.57 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 18.3 ms | 2.08 ms | 16.6 ms | 5.1 MB/s |
| fpeg parse, tree | 5.59 ms | 301 µs | 5.18 ms | 16.8 MB/s |
| fpeg parse and Transformer, numbers | 8.74 ms | 795 µs | 7.72 ms | 10.8 MB/s |
| Lark LALR, tree | 106 ms | 13.2 ms | 93.1 ms | 0.9 MB/s |
| Lark LALR with inlined Transformer, numbers | 98.5 ms | 8.05 ms | 90.6 ms | 1.0 MB/s |
| pe machine, numbers | 46.9 ms | 2.98 ms | 44.3 ms | 2.0 MB/s |
| pe packrat, numbers | 105 ms | 4.76 ms | 99.2 ms | 0.9 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.96 ms | 861 µs | 7.08 ms | 247,548 lines/s |
| fpeg Python grammar load | 4.42 ms | 286 µs | 4.09 ms |  |
| fpeg parse, tree | 7.52 ms | 502 µs | 6.90 ms | 262,079 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 19.9 ms | 1.33 ms | 19.0 ms | 99,102 lines/s |
| fpeg parse and transform to ast, stamped natively, each method in a wrapper | 22.8 ms | 1.52 ms | 21.0 ms | 86,295 lines/s |
| fpeg parse and transform to ast, positions set in Python by that wrapper | 23.3 ms | 1.41 ms | 21.7 ms | 84,537 lines/s |
| CPython parser and symbol table, no AST objects | 5.70 ms | 409 µs | 5.27 ms | 345,832 lines/s |
| compile to bytecode | 8.68 ms | 537 µs | 8.14 ms | 227,088 lines/s |
| tree-sitter parse, native tree | 7.03 ms | 581 µs | 6.48 ms | 280,180 lines/s |
| tree-sitter cursor walk over every node | 2.43 ms | 171 µs | 2.28 ms |  |
| tree-sitter walk, a Python Node per node | 3.02 ms | 232 µs | 2.74 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.89 ms | 249 µs | 4.63 ms |  |
| LibCST parse_module | 85.5 ms | 5.60 ms | 76.6 ms | 23,051 lines/s |
| Lark Python grammar load | 241 ms | 19.2 ms | 225 ms |  |
| Lark LALR, bundled Python grammar, tree | 89.1 ms | 11.5 ms | 80.1 ms | 22,116 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 559 ms | 31.9 ms | 515 ms |  |
| grammar load with a warm cache | 29.2 ms | 2.13 ms | 27.0 ms |  |
| LALR parse, tree | 149 ms | 15.7 ms | 135 ms | 13,269 lines/s |
| Transformer with the real callbacks | 17.6 ms | 983 µs | 16.5 ms |  |
| Transformer, trivial method on every rule | 9.38 ms | 690 µs | 8.84 ms |  |
| end to end: parse, transform, fix locations | 178 ms | 19.9 ms | 160 ms | 11,066 lines/s |
| fpeg ORD grammar load | 4.91 ms | 240 µs | 4.66 ms |  |
| fpeg parse, tree | 8.35 ms | 797 µs | 7.45 ms | 236,121 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 21.2 ms | 2.43 ms | 19.1 ms | 92,829 lines/s |
| fpeg end to end: parse, transform, fix locations | 25.3 ms | 1.48 ms | 23.9 ms | 77,813 lines/s |
| end to end over the 76 ORD files | 635 ms | 33.2 ms | 595 ms |  |
| fpeg end to end over the 76 ORD files | 133 ms | 4.98 ms | 127 ms |  |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.57 ms | 393 µs | 2.02 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 29.0 ms | 907 µs | 27.5 ms | 3.5 MB/s |
| expressions/fpeg machine, tree | 11.7 ms | 342 µs | 11.3 ms | 8.5 MB/s |
| fpeg grammar load | 182 µs | 24.3 µs | 168 µs |  |
| fpeg machine on a small sum | 1.48 µs | 293 ns | 1.25 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.93 µs | 380 ns | 2.71 µs |  |
| json/fpeg machine with its profile, tree | 349 ms | 15.7 ms | 324 ms | 7.2 MB/s |
| json/fpeg machine, tree | 32.0 ms | 1.31 ms | 30.3 ms | 78.3 MB/s |
| json/pest generated, token queue | 58.6 ms | 2.72 ms | 55.1 ms | 42.8 MB/s |
| json/pest_vm interpreter, token queue | 315 ms | 9.43 ms | 304 ms | 8.0 MB/s |
| json/serde_json to Value | 9.18 ms | 516 µs | 8.62 ms | 273.1 MB/s |
| passes on CSV/every pass | 1.61 ms | 130 µs | 1.48 ms |  |
| passes on CSV/without dispatch | 1.56 ms | 110 µs | 1.46 ms |  |
| passes on CSV/without hasher | 1.55 ms | 58.6 µs | 1.49 ms |  |
| passes on CSV/without inline | 1.83 ms | 112 µs | 1.72 ms |  |
| passes on CSV/without left_loop | 1.59 ms | 116 µs | 1.47 ms |  |
| passes on CSV/without skip | 1.59 ms | 94.1 µs | 1.47 ms |  |
| passes on CSV/without span | 1.76 ms | 59.8 µs | 1.70 ms |  |
| passes on CSV/without split | 1.96 ms | 61.3 µs | 1.91 ms |  |
| passes on expressions/every pass | 11.5 ms | 509 µs | 11.1 ms | 8.7 MB/s |
| passes on expressions/without dispatch | 18.3 ms | 2.89 ms | 16.2 ms | 5.5 MB/s |
| passes on expressions/without hasher | 12.2 ms | 1.18 ms | 11.3 ms | 8.2 MB/s |
| passes on expressions/without inline | 11.3 ms | 405 µs | 10.9 ms | 8.9 MB/s |
| passes on expressions/without left_loop | 41.0 ms | 1.82 ms | 38.4 ms | 2.4 MB/s |
| passes on expressions/without skip | 13.0 ms | 533 µs | 12.3 ms | 7.7 MB/s |
| passes on expressions/without span | 11.4 ms | 347 µs | 11.1 ms | 8.7 MB/s |
| passes on expressions/without split | 11.0 ms | 384 µs | 10.6 ms | 9.1 MB/s |
| passes on JSON/every pass | 31.2 ms | 1.38 ms | 29.1 ms | 80.4 MB/s |
| passes on JSON/without dispatch | 33.3 ms | 1.37 ms | 31.9 ms | 75.3 MB/s |
| passes on JSON/without hasher | 31.3 ms | 1.93 ms | 29.0 ms | 80.1 MB/s |
| passes on JSON/without inline | 47.7 ms | 2.77 ms | 44.8 ms | 52.6 MB/s |
| passes on JSON/without left_loop | 31.3 ms | 1.54 ms | 29.5 ms | 80.1 MB/s |
| passes on JSON/without skip | 41.9 ms | 4.11 ms | 38.2 ms | 59.9 MB/s |
| passes on JSON/without span | 32.7 ms | 1.06 ms | 31.6 ms | 76.7 MB/s |
| passes on JSON/without split | 42.2 ms | 2.81 ms | 38.0 ms | 59.5 MB/s |
| passes on Lua/every pass | 7.79 ms | 806 µs | 6.78 ms |  |
| passes on Lua/without dispatch | 10.7 ms | 393 µs | 10.4 ms |  |
| passes on Lua/without hasher | 7.42 ms | 623 µs | 6.83 ms |  |
| passes on Lua/without inline | 6.93 ms | 304 µs | 6.64 ms |  |
| passes on Lua/without left_loop | 18.2 ms | 1.05 ms | 16.7 ms |  |
| passes on Lua/without skip | 8.10 ms | 560 µs | 7.45 ms |  |
| passes on Lua/without span | 7.24 ms | 547 µs | 6.81 ms |  |
| passes on Lua/without split | 6.74 ms | 257 µs | 6.50 ms |  |
| passes on Python/every pass | 7.01 ms | 228 µs | 6.80 ms | 281,317 lines/s |
| passes on Python/without dispatch | 11.4 ms | 839 µs | 10.8 ms | 172,411 lines/s |
| passes on Python/without hasher | 8.45 ms | 417 µs | 7.86 ms | 233,121 lines/s |
| passes on Python/without inline | 8.22 ms | 398 µs | 7.60 ms | 239,891 lines/s |
| passes on Python/without left_loop | 18.0 ms | 3.55 ms | 14.0 ms | 109,243 lines/s |
| passes on Python/without skip | 8.65 ms | 564 µs | 7.85 ms | 227,796 lines/s |
| passes on Python/without span | 7.66 ms | 596 µs | 7.03 ms | 257,301 lines/s |
| passes on Python/without split | 8.25 ms | 790 µs | 7.02 ms | 238,950 lines/s |
| passes on SQL/every pass | 5.04 ms | 189 µs | 4.79 ms |  |
| passes on SQL/without dispatch | 6.89 ms | 844 µs | 6.27 ms |  |
| passes on SQL/without hasher | 6.49 ms | 1.72 ms | 5.24 ms |  |
| passes on SQL/without inline | 5.17 ms | 190 µs | 4.96 ms |  |
| passes on SQL/without left_loop | 12.3 ms | 1.06 ms | 9.84 ms |  |
| passes on SQL/without skip | 6.21 ms | 368 µs | 5.77 ms |  |
| passes on SQL/without span | 6.26 ms | 602 µs | 5.31 ms |  |
| passes on SQL/without split | 5.58 ms | 546 µs | 4.84 ms |  |
| passes on the grammar files/every pass | 5.32 ms | 472 µs | 4.94 ms |  |
| passes on the grammar files/without dispatch | 11.0 ms | 1.73 ms | 9.27 ms |  |
| passes on the grammar files/without hasher | 5.43 ms | 475 µs | 4.86 ms |  |
| passes on the grammar files/without inline | 7.10 ms | 672 µs | 6.30 ms |  |
| passes on the grammar files/without left_loop | 5.38 ms | 346 µs | 4.92 ms |  |
| passes on the grammar files/without skip | 6.23 ms | 531 µs | 5.54 ms |  |
| passes on the grammar files/without span | 5.81 ms | 383 µs | 5.43 ms |  |
| passes on the grammar files/without split | 6.23 ms | 306 µs | 5.87 ms |  |
| passes on the outline language/every pass | 1.70 ms | 362 µs | 1.34 ms |  |
| passes on the outline language/without dispatch | 1.42 ms | 108 µs | 1.34 ms |  |
| passes on the outline language/without hasher | 1.86 ms | 126 µs | 1.58 ms |  |
| passes on the outline language/without inline | 1.72 ms | 231 µs | 1.46 ms |  |
| passes on the outline language/without left_loop | 1.73 ms | 132 µs | 1.53 ms |  |
| passes on the outline language/without skip | 1.92 ms | 162 µs | 1.79 ms |  |
| passes on the outline language/without span | 2.00 ms | 208 µs | 1.80 ms |  |
| passes on the outline language/without split | 1.78 ms | 557 µs | 1.36 ms |  |
| passes on tiny/every pass | 3.63 ms | 169 µs | 3.46 ms |  |
| passes on tiny/without dispatch | 5.06 ms | 441 µs | 4.51 ms |  |
| passes on tiny/without hasher | 4.69 ms | 330 µs | 4.20 ms |  |
| passes on tiny/without inline | 3.79 ms | 187 µs | 3.54 ms |  |
| passes on tiny/without left_loop | 9.79 ms | 535 µs | 9.06 ms |  |
| passes on tiny/without skip | 4.25 ms | 317 µs | 4.03 ms |  |
| passes on tiny/without span | 4.49 ms | 543 µs | 3.83 ms |  |
| passes on tiny/without split | 3.79 ms | 271 µs | 3.47 ms |  |
| passes on TOML/every pass | 3.22 ms | 582 µs | 2.51 ms |  |
| passes on TOML/without dispatch | 2.89 ms | 168 µs | 2.74 ms |  |
| passes on TOML/without hasher | 3.36 ms | 286 µs | 2.86 ms |  |
| passes on TOML/without inline | 2.86 ms | 306 µs | 2.59 ms |  |
| passes on TOML/without left_loop | 2.87 ms | 197 µs | 2.59 ms |  |
| passes on TOML/without skip | 3.23 ms | 160 µs | 3.01 ms |  |
| passes on TOML/without span | 3.05 ms | 164 µs | 2.84 ms |  |
| passes on TOML/without split | 2.68 ms | 113 µs | 2.59 ms |  |
| pest_vm grammar load | 644 µs | 78.5 µs | 556 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.26 ms | 212 µs | 2.09 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 25.2 ms | 818 µs | 23.7 ms | 4.0 MB/s |
| expressions/fpeg machine, tree | 10.5 ms | 399 µs | 10.2 ms | 9.5 MB/s |
| fpeg grammar load | 169 µs | 16.9 µs | 151 µs |  |
| fpeg machine on a small sum | 1.31 µs | 238 ns | 1.15 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.89 µs | 427 ns | 2.61 µs |  |
| json/fpeg machine with its profile, tree | 358 ms | 11.0 ms | 343 ms | 7.0 MB/s |
| json/fpeg machine, tree | 26.8 ms | 516 µs | 26.3 ms | 93.7 MB/s |
| json/pest generated, token queue | 75.6 ms | 1.68 ms | 74.2 ms | 33.2 MB/s |
| json/pest_vm interpreter, token queue | 335 ms | 9.56 ms | 325 ms | 7.5 MB/s |
| json/serde_json to Value | 9.02 ms | 385 µs | 8.43 ms | 278.0 MB/s |
| passes on CSV/every pass | 1.51 ms | 30.9 µs | 1.46 ms |  |
| passes on CSV/without dispatch | 1.46 ms | 95.2 µs | 1.33 ms |  |
| passes on CSV/without hasher | 1.55 ms | 135 µs | 1.37 ms |  |
| passes on CSV/without inline | 1.71 ms | 156 µs | 1.60 ms |  |
| passes on CSV/without left_loop | 1.50 ms | 153 µs | 1.39 ms |  |
| passes on CSV/without skip | 1.40 ms | 59.5 µs | 1.35 ms |  |
| passes on CSV/without span | 1.49 ms | 48.7 µs | 1.43 ms |  |
| passes on CSV/without split | 2.05 ms | 148 µs | 1.93 ms |  |
| passes on expressions/every pass | 11.5 ms | 141 µs | 11.3 ms | 8.7 MB/s |
| passes on expressions/without dispatch | 17.8 ms | 486 µs | 17.2 ms | 5.6 MB/s |
| passes on expressions/without hasher | 11.6 ms | 309 µs | 11.2 ms | 8.6 MB/s |
| passes on expressions/without inline | 11.3 ms | 333 µs | 10.8 ms | 8.9 MB/s |
| passes on expressions/without left_loop | 40.7 ms | 3.42 ms | 38.4 ms | 2.5 MB/s |
| passes on expressions/without skip | 13.6 ms | 451 µs | 13.1 ms | 7.4 MB/s |
| passes on expressions/without span | 12.0 ms | 400 µs | 11.5 ms | 8.3 MB/s |
| passes on expressions/without split | 11.3 ms | 329 µs | 10.9 ms | 8.9 MB/s |
| passes on JSON/every pass | 27.3 ms | 2.78 ms | 25.1 ms | 91.7 MB/s |
| passes on JSON/without dispatch | 33.1 ms | 4.68 ms | 30.5 ms | 75.7 MB/s |
| passes on JSON/without hasher | 27.8 ms | 3.12 ms | 24.5 ms | 90.2 MB/s |
| passes on JSON/without inline | 47.3 ms | 1.25 ms | 45.8 ms | 53.0 MB/s |
| passes on JSON/without left_loop | 26.9 ms | 648 µs | 25.9 ms | 93.3 MB/s |
| passes on JSON/without skip | 40.7 ms | 2.32 ms | 37.1 ms | 61.6 MB/s |
| passes on JSON/without span | 27.7 ms | 1.79 ms | 26.4 ms | 90.5 MB/s |
| passes on JSON/without split | 38.0 ms | 905 µs | 37.0 ms | 65.9 MB/s |
| passes on Lua/every pass | 6.76 ms | 537 µs | 6.25 ms |  |
| passes on Lua/without dispatch | 10.8 ms | 253 µs | 10.4 ms |  |
| passes on Lua/without hasher | 7.26 ms | 689 µs | 6.38 ms |  |
| passes on Lua/without inline | 6.76 ms | 281 µs | 6.45 ms |  |
| passes on Lua/without left_loop | 16.7 ms | 963 µs | 15.2 ms |  |
| passes on Lua/without skip | 7.65 ms | 338 µs | 7.33 ms |  |
| passes on Lua/without span | 7.00 ms | 456 µs | 6.50 ms |  |
| passes on Lua/without split | 6.62 ms | 233 µs | 6.40 ms |  |
| passes on Python/every pass | 7.11 ms | 617 µs | 6.52 ms | 277,054 lines/s |
| passes on Python/without dispatch | 11.2 ms | 557 µs | 10.7 ms | 175,647 lines/s |
| passes on Python/without hasher | 7.27 ms | 299 µs | 7.01 ms | 271,175 lines/s |
| passes on Python/without inline | 6.92 ms | 213 µs | 6.73 ms | 284,919 lines/s |
| passes on Python/without left_loop | 13.0 ms | 890 µs | 12.3 ms | 152,066 lines/s |
| passes on Python/without skip | 8.37 ms | 435 µs | 7.90 ms | 235,545 lines/s |
| passes on Python/without span | 7.57 ms | 595 µs | 7.01 ms | 260,322 lines/s |
| passes on Python/without split | 6.99 ms | 262 µs | 6.74 ms | 282,057 lines/s |
| passes on SQL/every pass | 5.00 ms | 85.1 µs | 4.89 ms |  |
| passes on SQL/without dispatch | 6.76 ms | 154 µs | 6.60 ms |  |
| passes on SQL/without hasher | 5.24 ms | 136 µs | 5.05 ms |  |
| passes on SQL/without inline | 5.28 ms | 176 µs | 5.08 ms |  |
| passes on SQL/without left_loop | 9.52 ms | 589 µs | 8.98 ms |  |
| passes on SQL/without skip | 6.13 ms | 99.4 µs | 5.97 ms |  |
| passes on SQL/without span | 5.39 ms | 131 µs | 5.17 ms |  |
| passes on SQL/without split | 4.82 ms | 279 µs | 4.56 ms |  |
| passes on the grammar files/every pass | 5.28 ms | 102 µs | 5.15 ms |  |
| passes on the grammar files/without dispatch | 9.85 ms | 151 µs | 9.67 ms |  |
| passes on the grammar files/without hasher | 5.33 ms | 158 µs | 5.08 ms |  |
| passes on the grammar files/without inline | 5.37 ms | 120 µs | 5.21 ms |  |
| passes on the grammar files/without left_loop | 5.29 ms | 90.7 µs | 5.14 ms |  |
| passes on the grammar files/without skip | 6.27 ms | 112 µs | 6.14 ms |  |
| passes on the grammar files/without span | 5.86 ms | 109 µs | 5.72 ms |  |
| passes on the grammar files/without split | 5.22 ms | 100 µs | 5.09 ms |  |
| passes on the outline language/every pass | 1.32 ms | 109 µs | 1.22 ms |  |
| passes on the outline language/without dispatch | 1.27 ms | 37.8 µs | 1.23 ms |  |
| passes on the outline language/without hasher | 1.27 ms | 57.2 µs | 1.22 ms |  |
| passes on the outline language/without inline | 1.36 ms | 66.5 µs | 1.29 ms |  |
| passes on the outline language/without left_loop | 1.32 ms | 78.1 µs | 1.23 ms |  |
| passes on the outline language/without skip | 1.93 ms | 128 µs | 1.81 ms |  |
| passes on the outline language/without span | 2.00 ms | 99.8 µs | 1.89 ms |  |
| passes on the outline language/without split | 1.30 ms | 81.5 µs | 1.22 ms |  |
| passes on tiny/every pass | 3.59 ms | 198 µs | 3.41 ms |  |
| passes on tiny/without dispatch | 5.04 ms | 106 µs | 4.93 ms |  |
| passes on tiny/without hasher | 3.80 ms | 202 µs | 3.57 ms |  |
| passes on tiny/without inline | 3.83 ms | 94.6 µs | 3.72 ms |  |
| passes on tiny/without left_loop | 9.22 ms | 207 µs | 8.88 ms |  |
| passes on tiny/without skip | 4.52 ms | 99.3 µs | 4.37 ms |  |
| passes on tiny/without span | 4.60 ms | 750 µs | 4.01 ms |  |
| passes on tiny/without split | 3.87 ms | 106 µs | 3.74 ms |  |
| passes on TOML/every pass | 2.85 ms | 67.5 µs | 2.77 ms |  |
| passes on TOML/without dispatch | 3.04 ms | 59.7 µs | 2.97 ms |  |
| passes on TOML/without hasher | 2.91 ms | 338 µs | 2.73 ms |  |
| passes on TOML/without inline | 2.83 ms | 74.7 µs | 2.74 ms |  |
| passes on TOML/without left_loop | 2.69 ms | 65.7 µs | 2.61 ms |  |
| passes on TOML/without skip | 3.37 ms | 87.3 µs | 3.29 ms |  |
| passes on TOML/without span | 3.25 ms | 69.1 µs | 3.16 ms |  |
| passes on TOML/without split | 2.81 ms | 67.9 µs | 2.73 ms |  |
| pest_vm grammar load | 638 µs | 81.1 µs | 575 µs |  |
