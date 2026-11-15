# Benchmark results 2026-11-15c-windows

Written by `benchmarks/run.py` in default mode at commit 5f1b8f9.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 11.2 ms | 393 µs | 10.6 ms | 224.3 MB/s |
| fpeg grammar load | 303 µs | 32.7 µs | 281 µs |  |
| fpeg parse, tree | 33.9 ms | 2.96 ms | 30.0 ms | 74.0 MB/s |
| fpeg parse and grammar builders, Python objects | 53.6 ms | 6.81 ms | 48.9 ms | 46.8 MB/s |
| fpeg parse and Transformer methods, Python objects | 101 ms | 13.0 ms | 83.6 ms | 24.7 MB/s |
| Lark grammar load | 5.85 ms | 295 µs | 5.37 ms |  |
| Lark LALR, tree | 1.32 s | 38.3 ms | 1.26 s | 1.9 MB/s |
| Lark Transformer over the tree | 407 ms | 13.1 ms | 385 ms | 6.2 MB/s |
| Lark LALR with inlined Transformer, Python objects | 1.06 s | 21.5 ms | 1.02 s | 2.4 MB/s |
| pe machine grammar load | 2.68 ms | 174 µs | 2.48 ms |  |
| pe machine, match only | 158 ms | 9.18 ms | 143 ms | 15.9 MB/s |
| pe machine, Python objects | 211 ms | 11.9 ms | 193 ms | 11.9 MB/s |
| pe packrat grammar load | 2.82 ms | 139 µs | 2.62 ms |  |
| pe packrat, match only | 495 ms | 16.7 ms | 473 ms | 5.1 MB/s |
| pe packrat, Python objects | 522 ms | 16.7 ms | 495 ms | 4.8 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 175 µs | 14.0 µs | 159 µs |  |
| transform, a method on every node, list | 2.61 ms | 218 µs | 2.29 ms |  |
| transform, a method on every node, *children | 2.36 ms | 117 µs | 2.19 ms |  |
| transform, the same with a member by field | 2.52 ms | 234 µs | 2.21 ms |  |
| transform, a method on every token, taking nothing | 2.62 ms | 92.4 µs | 2.50 ms |  |
| transform, a method on every token, taking it | 4.15 ms | 265 µs | 3.91 ms |  |
| interpreter, no method | 11.0 ms | 715 µs | 9.90 ms |  |
| interpreter, a method on every node | 12.1 ms | 710 µs | 11.2 ms |  |
| visitor, no method | 2.31 ms | 94.4 µs | 2.19 ms |  |
| visitor, a method on every node | 3.74 ms | 282 µs | 3.38 ms |  |
| parse | 3.65 ms | 271 µs | 3.23 ms |  |
| parse, then every view and token | 9.43 ms | 562 µs | 8.42 ms |  |
| 2000 small CSV parses with a transformer | 5.21 ms | 271 µs | 4.71 ms |  |
| 2000 small CSV parses with csv.reader | 1.86 ms | 124 µs | 1.68 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 18.5 ms | 1.25 ms | 16.9 ms | 5.1 MB/s |
| fpeg parse, tree | 5.75 ms | 189 µs | 5.49 ms | 16.3 MB/s |
| fpeg parse and Transformer, numbers | 9.43 ms | 691 µs | 8.54 ms | 10.0 MB/s |
| Lark LALR, tree | 112 ms | 4.92 ms | 105 ms | 0.8 MB/s |
| Lark LALR with inlined Transformer, numbers | 105 ms | 6.08 ms | 96.0 ms | 0.9 MB/s |
| pe machine, numbers | 52.7 ms | 3.35 ms | 47.0 ms | 1.8 MB/s |
| pe packrat, numbers | 113 ms | 4.93 ms | 104 ms | 0.8 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 8.01 ms | 289 µs | 7.63 ms | 246,197 lines/s |
| fpeg Python grammar load | 5.20 ms | 262 µs | 4.84 ms |  |
| fpeg parse, tree | 8.45 ms | 325 µs | 8.08 ms | 233,245 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 18.6 ms | 889 µs | 17.2 ms | 105,720 lines/s |
| fpeg parse and transform to ast, stamped natively, each method in a wrapper | 22.0 ms | 858 µs | 20.8 ms | 89,536 lines/s |
| fpeg parse and transform to ast, positions set in Python by that wrapper | 22.7 ms | 1.26 ms | 21.2 ms | 86,705 lines/s |
| CPython parser and symbol table, no AST objects | 5.89 ms | 184 µs | 5.62 ms | 334,370 lines/s |
| compile to bytecode | 9.31 ms | 332 µs | 8.75 ms | 211,776 lines/s |
| tree-sitter parse, native tree | 7.65 ms | 353 µs | 7.21 ms | 257,630 lines/s |
| tree-sitter cursor walk over every node | 2.47 ms | 68.3 µs | 2.38 ms |  |
| tree-sitter walk, a Python Node per node | 3.06 ms | 134 µs | 2.90 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 5.22 ms | 296 µs | 4.87 ms |  |
| LibCST parse_module | 91.1 ms | 4.14 ms | 86.8 ms | 21,628 lines/s |
| Lark Python grammar load | 266 ms | 13.4 ms | 241 ms |  |
| Lark LALR, bundled Python grammar, tree | 94.2 ms | 6.95 ms | 84.6 ms | 20,929 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 599 ms | 24.5 ms | 564 ms |  |
| grammar load with a warm cache | 32.4 ms | 2.43 ms | 28.5 ms |  |
| LALR parse, tree | 155 ms | 5.37 ms | 145 ms | 12,691 lines/s |
| Transformer with the real callbacks | 19.5 ms | 2.07 ms | 17.8 ms |  |
| Transformer, trivial method on every rule | 9.82 ms | 514 µs | 9.23 ms |  |
| end to end: parse, transform, fix locations | 188 ms | 10.9 ms | 170 ms | 10,493 lines/s |
| fpeg ORD grammar load | 5.89 ms | 352 µs | 5.34 ms |  |
| fpeg parse, tree | 9.08 ms | 436 µs | 8.41 ms | 217,067 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 20.0 ms | 1.15 ms | 17.9 ms | 98,682 lines/s |
| fpeg end to end: parse, transform, fix locations | 24.9 ms | 1.10 ms | 23.6 ms | 79,104 lines/s |
| end to end over the 76 ORD files | 697 ms | 29.3 ms | 647 ms |  |
| fpeg end to end over the 76 ORD files | 144 ms | 7.88 ms | 130 ms |  |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.33 ms | 208 µs | 2.13 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 29.9 ms | 768 µs | 28.7 ms | 3.3 MB/s |
| expressions/fpeg machine, tree | 13.3 ms | 841 µs | 12.5 ms | 7.5 MB/s |
| fpeg grammar load | 351 µs | 28.9 µs | 282 µs |  |
| fpeg machine on a small sum | 1.70 µs | 197 ns | 1.37 µs |  |
| fpeg machine on a small sum, nothing remembered | 3.53 µs | 365 ns | 3.06 µs |  |
| json/fpeg machine with its profile, tree | 431 ms | 16.9 ms | 390 ms | 5.8 MB/s |
| json/fpeg machine, tree | 36.8 ms | 1.24 ms | 34.9 ms | 68.1 MB/s |
| json/pest generated, token queue | 63.9 ms | 2.79 ms | 60.3 ms | 39.3 MB/s |
| json/pest_vm interpreter, token queue | 326 ms | 6.76 ms | 317 ms | 7.7 MB/s |
| json/serde_json to Value | 10.3 ms | 528 µs | 9.53 ms | 242.6 MB/s |
| passes on CSV/every pass | 1.87 ms | 66.2 µs | 1.81 ms |  |
| passes on CSV/without dispatch | 1.76 ms | 69.4 µs | 1.69 ms |  |
| passes on CSV/without hasher | 1.85 ms | 82.6 µs | 1.77 ms |  |
| passes on CSV/without inline | 2.07 ms | 77.2 µs | 1.97 ms |  |
| passes on CSV/without left_loop | 1.87 ms | 67.8 µs | 1.79 ms |  |
| passes on CSV/without skip | 1.90 ms | 51.0 µs | 1.84 ms |  |
| passes on CSV/without span | 2.09 ms | 46.3 µs | 2.03 ms |  |
| passes on CSV/without split | 2.29 ms | 130 µs | 2.10 ms |  |
| passes on expressions/every pass | 14.0 ms | 438 µs | 13.3 ms | 7.2 MB/s |
| passes on expressions/without dispatch | 20.4 ms | 532 µs | 19.8 ms | 4.9 MB/s |
| passes on expressions/without hasher | 13.7 ms | 830 µs | 12.5 ms | 7.3 MB/s |
| passes on expressions/without inline | 13.5 ms | 402 µs | 12.4 ms | 7.4 MB/s |
| passes on expressions/without left_loop | 47.0 ms | 2.36 ms | 44.1 ms | 2.1 MB/s |
| passes on expressions/without skip | 16.1 ms | 377 µs | 15.7 ms | 6.2 MB/s |
| passes on expressions/without span | 13.6 ms | 736 µs | 12.5 ms | 7.3 MB/s |
| passes on expressions/without split | 12.5 ms | 462 µs | 11.7 ms | 8.0 MB/s |
| passes on JSON/every pass | 36.9 ms | 1.40 ms | 35.0 ms | 67.9 MB/s |
| passes on JSON/without dispatch | 38.4 ms | 1.66 ms | 36.5 ms | 65.2 MB/s |
| passes on JSON/without hasher | 35.6 ms | 975 µs | 34.3 ms | 70.4 MB/s |
| passes on JSON/without inline | 54.5 ms | 2.41 ms | 50.6 ms | 46.0 MB/s |
| passes on JSON/without left_loop | 37.2 ms | 1.47 ms | 34.7 ms | 67.3 MB/s |
| passes on JSON/without skip | 47.0 ms | 2.85 ms | 43.9 ms | 53.3 MB/s |
| passes on JSON/without span | 38.0 ms | 1.96 ms | 35.9 ms | 66.0 MB/s |
| passes on JSON/without split | 47.0 ms | 1.63 ms | 44.7 ms | 53.4 MB/s |
| passes on Lua/every pass | 7.80 ms | 273 µs | 7.50 ms |  |
| passes on Lua/without dispatch | 12.0 ms | 732 µs | 11.2 ms |  |
| passes on Lua/without hasher | 8.52 ms | 384 µs | 7.90 ms |  |
| passes on Lua/without inline | 7.79 ms | 226 µs | 7.51 ms |  |
| passes on Lua/without left_loop | 19.9 ms | 542 µs | 19.3 ms |  |
| passes on Lua/without skip | 9.04 ms | 263 µs | 8.63 ms |  |
| passes on Lua/without span | 8.25 ms | 364 µs | 7.91 ms |  |
| passes on Lua/without split | 7.73 ms | 373 µs | 7.00 ms |  |
| passes on Python/every pass | 8.25 ms | 377 µs | 7.83 ms | 238,776 lines/s |
| passes on Python/without dispatch | 12.5 ms | 796 µs | 11.6 ms | 157,300 lines/s |
| passes on Python/without hasher | 9.44 ms | 432 µs | 8.68 ms | 208,823 lines/s |
| passes on Python/without inline | 8.25 ms | 387 µs | 7.67 ms | 238,976 lines/s |
| passes on Python/without left_loop | 15.9 ms | 514 µs | 15.3 ms | 123,684 lines/s |
| passes on Python/without skip | 10.1 ms | 800 µs | 9.15 ms | 194,326 lines/s |
| passes on Python/without span | 8.49 ms | 384 µs | 7.92 ms | 232,057 lines/s |
| passes on Python/without split | 8.46 ms | 459 µs | 8.03 ms | 233,011 lines/s |
| passes on SQL/every pass | 5.93 ms | 329 µs | 5.60 ms |  |
| passes on SQL/without dispatch | 7.44 ms | 347 µs | 7.11 ms |  |
| passes on SQL/without hasher | 6.30 ms | 387 µs | 5.77 ms |  |
| passes on SQL/without inline | 5.96 ms | 339 µs | 5.61 ms |  |
| passes on SQL/without left_loop | 12.1 ms | 1.07 ms | 10.7 ms |  |
| passes on SQL/without skip | 7.01 ms | 438 µs | 6.48 ms |  |
| passes on SQL/without span | 6.15 ms | 213 µs | 5.92 ms |  |
| passes on SQL/without split | 5.89 ms | 239 µs | 5.47 ms |  |
| passes on the grammar files/every pass | 6.23 ms | 189 µs | 6.01 ms |  |
| passes on the grammar files/without dispatch | 10.8 ms | 512 µs | 10.3 ms |  |
| passes on the grammar files/without hasher | 6.19 ms | 267 µs | 5.73 ms |  |
| passes on the grammar files/without inline | 6.31 ms | 337 µs | 6.04 ms |  |
| passes on the grammar files/without left_loop | 6.28 ms | 199 µs | 5.95 ms |  |
| passes on the grammar files/without skip | 7.35 ms | 346 µs | 6.88 ms |  |
| passes on the grammar files/without span | 7.08 ms | 393 µs | 6.62 ms |  |
| passes on the grammar files/without split | 6.23 ms | 254 µs | 5.92 ms |  |
| passes on the outline language/every pass | 1.74 ms | 55.6 µs | 1.66 ms |  |
| passes on the outline language/without dispatch | 1.74 ms | 127 µs | 1.61 ms |  |
| passes on the outline language/without hasher | 1.78 ms | 71.8 µs | 1.68 ms |  |
| passes on the outline language/without inline | 1.86 ms | 91.8 µs | 1.72 ms |  |
| passes on the outline language/without left_loop | 1.77 ms | 59.4 µs | 1.70 ms |  |
| passes on the outline language/without skip | 2.33 ms | 131 µs | 2.17 ms |  |
| passes on the outline language/without span | 2.19 ms | 102 µs | 2.08 ms |  |
| passes on the outline language/without split | 1.75 ms | 89.9 µs | 1.63 ms |  |
| passes on tiny/every pass | 4.33 ms | 155 µs | 4.11 ms |  |
| passes on tiny/without dispatch | 5.43 ms | 197 µs | 5.14 ms |  |
| passes on tiny/without hasher | 4.62 ms | 142 µs | 4.44 ms |  |
| passes on tiny/without inline | 4.38 ms | 240 µs | 4.13 ms |  |
| passes on tiny/without left_loop | 10.5 ms | 440 µs | 10.1 ms |  |
| passes on tiny/without skip | 4.96 ms | 210 µs | 4.66 ms |  |
| passes on tiny/without span | 4.89 ms | 336 µs | 4.50 ms |  |
| passes on tiny/without split | 4.43 ms | 145 µs | 4.22 ms |  |
| passes on TOML/every pass | 3.16 ms | 248 µs | 2.90 ms |  |
| passes on TOML/without dispatch | 3.23 ms | 156 µs | 3.03 ms |  |
| passes on TOML/without hasher | 3.13 ms | 122 µs | 2.97 ms |  |
| passes on TOML/without inline | 3.24 ms | 89.2 µs | 3.09 ms |  |
| passes on TOML/without left_loop | 3.09 ms | 147 µs | 2.81 ms |  |
| passes on TOML/without skip | 3.89 ms | 207 µs | 3.53 ms |  |
| passes on TOML/without span | 3.53 ms | 128 µs | 3.31 ms |  |
| passes on TOML/without split | 3.25 ms | 150 µs | 2.97 ms |  |
| pest_vm grammar load | 766 µs | 50.6 µs | 633 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.00 ms | 125 µs | 1.80 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 25.7 ms | 679 µs | 24.6 ms | 3.9 MB/s |
| expressions/fpeg machine, tree | 10.6 ms | 151 µs | 10.4 ms | 9.4 MB/s |
| fpeg grammar load | 309 µs | 24.9 µs | 267 µs |  |
| fpeg machine on a small sum | 1.45 µs | 174 ns | 1.18 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.95 µs | 308 ns | 2.47 µs |  |
| json/fpeg machine with its profile, tree | 403 ms | 9.32 ms | 385 ms | 6.2 MB/s |
| json/fpeg machine, tree | 25.3 ms | 908 µs | 24.2 ms | 99.3 MB/s |
| json/pest generated, token queue | 88.4 ms | 4.61 ms | 78.7 ms | 28.4 MB/s |
| json/pest_vm interpreter, token queue | 369 ms | 10.1 ms | 354 ms | 6.8 MB/s |
| json/serde_json to Value | 9.69 ms | 715 µs | 8.64 ms | 258.8 MB/s |
| passes on CSV/every pass | 1.46 ms | 46.0 µs | 1.41 ms |  |
| passes on CSV/without dispatch | 1.40 ms | 37.1 µs | 1.34 ms |  |
| passes on CSV/without hasher | 1.50 ms | 52.1 µs | 1.42 ms |  |
| passes on CSV/without inline | 1.76 ms | 64.1 µs | 1.62 ms |  |
| passes on CSV/without left_loop | 1.50 ms | 81.8 µs | 1.33 ms |  |
| passes on CSV/without skip | 1.45 ms | 88.1 µs | 1.34 ms |  |
| passes on CSV/without span | 1.61 ms | 50.6 µs | 1.55 ms |  |
| passes on CSV/without split | 1.94 ms | 93.5 µs | 1.75 ms |  |
| passes on expressions/every pass | 10.7 ms | 453 µs | 9.85 ms | 9.3 MB/s |
| passes on expressions/without dispatch | 17.3 ms | 525 µs | 16.8 ms | 5.8 MB/s |
| passes on expressions/without hasher | 10.9 ms | 490 µs | 10.1 ms | 9.2 MB/s |
| passes on expressions/without inline | 11.2 ms | 336 µs | 10.7 ms | 8.9 MB/s |
| passes on expressions/without left_loop | 37.4 ms | 2.67 ms | 34.4 ms | 2.7 MB/s |
| passes on expressions/without skip | 12.5 ms | 284 µs | 12.0 ms | 8.0 MB/s |
| passes on expressions/without span | 11.0 ms | 360 µs | 10.7 ms | 9.1 MB/s |
| passes on expressions/without split | 11.0 ms | 457 µs | 10.3 ms | 9.1 MB/s |
| passes on JSON/every pass | 26.8 ms | 1.02 ms | 25.3 ms | 93.5 MB/s |
| passes on JSON/without dispatch | 28.9 ms | 1.91 ms | 26.2 ms | 86.8 MB/s |
| passes on JSON/without hasher | 26.5 ms | 772 µs | 25.5 ms | 94.6 MB/s |
| passes on JSON/without inline | 41.2 ms | 994 µs | 39.7 ms | 60.9 MB/s |
| passes on JSON/without left_loop | 24.8 ms | 631 µs | 23.7 ms | 101.1 MB/s |
| passes on JSON/without skip | 40.4 ms | 2.03 ms | 36.5 ms | 62.1 MB/s |
| passes on JSON/without span | 26.4 ms | 1.86 ms | 24.4 ms | 94.8 MB/s |
| passes on JSON/without split | 33.3 ms | 1.15 ms | 31.9 ms | 75.4 MB/s |
| passes on Lua/every pass | 6.77 ms | 269 µs | 6.46 ms |  |
| passes on Lua/without dispatch | 10.9 ms | 507 µs | 10.2 ms |  |
| passes on Lua/without hasher | 6.94 ms | 198 µs | 6.62 ms |  |
| passes on Lua/without inline | 6.73 ms | 456 µs | 6.29 ms |  |
| passes on Lua/without left_loop | 17.2 ms | 1.12 ms | 15.9 ms |  |
| passes on Lua/without skip | 8.19 ms | 164 µs | 8.00 ms |  |
| passes on Lua/without span | 7.50 ms | 287 µs | 7.19 ms |  |
| passes on Lua/without split | 7.21 ms | 234 µs | 6.96 ms |  |
| passes on Python/every pass | 7.58 ms | 423 µs | 6.82 ms | 260,053 lines/s |
| passes on Python/without dispatch | 11.6 ms | 305 µs | 11.2 ms | 169,721 lines/s |
| passes on Python/without hasher | 8.01 ms | 371 µs | 7.48 ms | 246,089 lines/s |
| passes on Python/without inline | 7.58 ms | 355 µs | 7.05 ms | 259,999 lines/s |
| passes on Python/without left_loop | 14.1 ms | 553 µs | 13.5 ms | 139,903 lines/s |
| passes on Python/without skip | 9.07 ms | 429 µs | 8.32 ms | 217,296 lines/s |
| passes on Python/without span | 8.14 ms | 431 µs | 7.39 ms | 242,123 lines/s |
| passes on Python/without split | 7.71 ms | 517 µs | 6.96 ms | 255,742 lines/s |
| passes on SQL/every pass | 5.00 ms | 359 µs | 4.35 ms |  |
| passes on SQL/without dispatch | 6.15 ms | 274 µs | 5.69 ms |  |
| passes on SQL/without hasher | 5.44 ms | 146 µs | 5.19 ms |  |
| passes on SQL/without inline | 4.97 ms | 234 µs | 4.58 ms |  |
| passes on SQL/without left_loop | 9.83 ms | 491 µs | 9.31 ms |  |
| passes on SQL/without skip | 6.00 ms | 238 µs | 5.57 ms |  |
| passes on SQL/without span | 5.44 ms | 442 µs | 4.85 ms |  |
| passes on SQL/without split | 4.68 ms | 121 µs | 4.53 ms |  |
| passes on the grammar files/every pass | 5.52 ms | 109 µs | 5.38 ms |  |
| passes on the grammar files/without dispatch | 9.34 ms | 476 µs | 8.59 ms |  |
| passes on the grammar files/without hasher | 5.54 ms | 146 µs | 5.32 ms |  |
| passes on the grammar files/without inline | 5.62 ms | 158 µs | 5.38 ms |  |
| passes on the grammar files/without left_loop | 5.15 ms | 225 µs | 4.93 ms |  |
| passes on the grammar files/without skip | 6.49 ms | 116 µs | 6.32 ms |  |
| passes on the grammar files/without span | 5.81 ms | 263 µs | 5.49 ms |  |
| passes on the grammar files/without split | 5.24 ms | 167 µs | 4.96 ms |  |
| passes on the outline language/every pass | 1.37 ms | 24.5 µs | 1.33 ms |  |
| passes on the outline language/without dispatch | 1.27 ms | 61.5 µs | 1.19 ms |  |
| passes on the outline language/without hasher | 1.30 ms | 96.6 µs | 1.23 ms |  |
| passes on the outline language/without inline | 1.35 ms | 35.6 µs | 1.30 ms |  |
| passes on the outline language/without left_loop | 1.30 ms | 79.1 µs | 1.20 ms |  |
| passes on the outline language/without skip | 1.73 ms | 58.9 µs | 1.67 ms |  |
| passes on the outline language/without span | 1.81 ms | 49.3 µs | 1.74 ms |  |
| passes on the outline language/without split | 1.28 ms | 63.6 µs | 1.22 ms |  |
| passes on tiny/every pass | 3.58 ms | 115 µs | 3.45 ms |  |
| passes on tiny/without dispatch | 4.99 ms | 166 µs | 4.82 ms |  |
| passes on tiny/without hasher | 3.72 ms | 234 µs | 3.37 ms |  |
| passes on tiny/without inline | 3.90 ms | 90.2 µs | 3.77 ms |  |
| passes on tiny/without left_loop | 9.70 ms | 496 µs | 8.58 ms |  |
| passes on tiny/without skip | 4.52 ms | 125 µs | 4.37 ms |  |
| passes on tiny/without span | 4.29 ms | 105 µs | 4.16 ms |  |
| passes on tiny/without split | 3.78 ms | 148 µs | 3.55 ms |  |
| passes on TOML/every pass | 2.41 ms | 118 µs | 2.23 ms |  |
| passes on TOML/without dispatch | 2.68 ms | 109 µs | 2.54 ms |  |
| passes on TOML/without hasher | 2.39 ms | 77.2 µs | 2.25 ms |  |
| passes on TOML/without inline | 2.43 ms | 59.3 µs | 2.35 ms |  |
| passes on TOML/without left_loop | 2.48 ms | 142 µs | 2.29 ms |  |
| passes on TOML/without skip | 3.06 ms | 163 µs | 2.84 ms |  |
| passes on TOML/without span | 2.86 ms | 74.8 µs | 2.80 ms |  |
| passes on TOML/without split | 2.41 ms | 149 µs | 2.27 ms |  |
| pest_vm grammar load | 725 µs | 69.3 µs | 581 µs |  |
