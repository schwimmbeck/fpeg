# Benchmark results 2026-11-15-windows

Written by `benchmarks/run.py` in default mode at commit 57fe64c.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 11.5 ms | 506 µs | 11.0 ms | 218.5 MB/s |
| fpeg grammar load | 175 µs | 6.41 µs | 162 µs |  |
| fpeg parse, tree | 31.2 ms | 2.28 ms | 28.9 ms | 80.4 MB/s |
| fpeg parse and grammar builders, Python objects | 49.4 ms | 2.87 ms | 46.1 ms | 50.8 MB/s |
| fpeg parse and Transformer methods, Python objects | 88.0 ms | 10.1 ms | 78.7 ms | 28.5 MB/s |
| Lark grammar load | 5.39 ms | 506 µs | 4.90 ms |  |
| Lark LALR, tree | 1.27 s | 120 ms | 1.16 s | 2.0 MB/s |
| Lark Transformer over the tree | 404 ms | 41.3 ms | 355 ms | 6.2 MB/s |
| Lark LALR with inlined Transformer, Python objects | 999 ms | 41.1 ms | 937 ms | 2.5 MB/s |
| pe machine grammar load | 2.44 ms | 187 µs | 2.26 ms |  |
| pe machine, match only | 135 ms | 9.69 ms | 129 ms | 18.5 MB/s |
| pe machine, Python objects | 183 ms | 8.63 ms | 175 ms | 13.7 MB/s |
| pe packrat grammar load | 2.65 ms | 151 µs | 2.49 ms |  |
| pe packrat, match only | 471 ms | 34.7 ms | 426 ms | 5.3 MB/s |
| pe packrat, Python objects | 486 ms | 33.9 ms | 454 ms | 5.2 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 146 µs | 14.1 µs | 136 µs |  |
| transform, a method on every node, list | 2.35 ms | 279 µs | 2.11 ms |  |
| transform, a method on every node, *children | 2.33 ms | 162 µs | 2.08 ms |  |
| transform, the same with a member by field | 2.24 ms | 178 µs | 2.08 ms |  |
| transform, a method on every token, taking nothing | 2.49 ms | 147 µs | 2.34 ms |  |
| transform, a method on every token, taking it | 3.93 ms | 742 µs | 3.54 ms |  |
| interpreter, no method | 9.72 ms | 541 µs | 9.21 ms |  |
| interpreter, a method on every node | 10.9 ms | 621 µs | 10.2 ms |  |
| visitor, no method | 2.09 ms | 121 µs | 1.97 ms |  |
| visitor, a method on every node | 3.36 ms | 217 µs | 3.12 ms |  |
| parse | 3.20 ms | 180 µs | 3.01 ms |  |
| parse, then every view and token | 8.53 ms | 631 µs | 7.85 ms |  |
| 2000 small CSV parses with a transformer | 4.82 ms | 307 µs | 4.45 ms |  |
| 2000 small CSV parses with csv.reader | 1.74 ms | 218 µs | 1.58 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 17.2 ms | 713 µs | 16.2 ms | 5.5 MB/s |
| fpeg parse, tree | 5.62 ms | 435 µs | 5.09 ms | 16.7 MB/s |
| fpeg parse and Transformer, numbers | 8.42 ms | 601 µs | 7.76 ms | 11.2 MB/s |
| Lark LALR, tree | 96.7 ms | 1.36 ms | 94.1 ms | 1.0 MB/s |
| Lark LALR with inlined Transformer, numbers | 98.5 ms | 8.00 ms | 90.3 ms | 1.0 MB/s |
| pe machine, numbers | 48.3 ms | 4.11 ms | 44.9 ms | 1.9 MB/s |
| pe packrat, numbers | 103 ms | 7.00 ms | 96.9 ms | 0.9 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.90 ms | 610 µs | 7.17 ms | 249,407 lines/s |
| fpeg Python grammar load | 4.73 ms | 528 µs | 4.21 ms |  |
| fpeg parse, tree | 9.84 ms | 776 µs | 8.98 ms | 200,218 lines/s |
| fpeg parse and transform to ast, positions stamped natively | 21.2 ms | 1.38 ms | 19.9 ms | 93,013 lines/s |
| fpeg parse and transform to ast, stamped natively, each method in a wrapper | 24.4 ms | 1.51 ms | 22.8 ms | 80,934 lines/s |
| fpeg parse and transform to ast, positions set in Python by that wrapper | 25.9 ms | 2.00 ms | 23.3 ms | 76,084 lines/s |
| CPython parser and symbol table, no AST objects | 5.79 ms | 429 µs | 5.11 ms | 340,517 lines/s |
| compile to bytecode | 8.85 ms | 870 µs | 8.08 ms | 222,823 lines/s |
| tree-sitter parse, native tree | 7.14 ms | 651 µs | 6.44 ms | 276,071 lines/s |
| tree-sitter cursor walk over every node | 2.38 ms | 107 µs | 2.27 ms |  |
| tree-sitter walk, a Python Node per node | 2.94 ms | 183 µs | 2.74 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.72 ms | 256 µs | 4.50 ms |  |
| LibCST parse_module | 86.2 ms | 11.1 ms | 77.2 ms | 22,856 lines/s |
| Lark Python grammar load | 247 ms | 24.9 ms | 226 ms |  |
| Lark LALR, bundled Python grammar, tree | 82.1 ms | 3.07 ms | 78.7 ms | 24,010 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 553 ms | 35.7 ms | 505 ms |  |
| grammar load with a warm cache | 28.0 ms | 1.51 ms | 26.6 ms |  |
| LALR parse, tree | 148 ms | 11.6 ms | 137 ms | 13,324 lines/s |
| Transformer with the real callbacks | 18.5 ms | 1.74 ms | 16.7 ms |  |
| Transformer, trivial method on every rule | 8.96 ms | 335 µs | 8.63 ms |  |
| end to end: parse, transform, fix locations | 178 ms | 22.5 ms | 158 ms | 11,070 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.17 ms | 215 µs | 2.01 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 27.7 ms | 1.50 ms | 26.4 ms | 3.6 MB/s |
| expressions/fpeg machine, tree | 11.7 ms | 639 µs | 11.1 ms | 8.6 MB/s |
| fpeg grammar load | 210 µs | 45.4 µs | 158 µs |  |
| fpeg machine on a small sum | 1.40 µs | 270 ns | 1.20 µs |  |
| fpeg machine on a small sum, nothing remembered | 3.04 µs | 499 ns | 2.66 µs |  |
| json/fpeg machine with its profile, tree | 357 ms | 17.5 ms | 328 ms | 7.0 MB/s |
| json/fpeg machine, tree | 32.6 ms | 1.69 ms | 30.4 ms | 76.8 MB/s |
| json/pest generated, token queue | 58.1 ms | 3.49 ms | 53.6 ms | 43.2 MB/s |
| json/pest_vm interpreter, token queue | 312 ms | 9.28 ms | 300 ms | 8.0 MB/s |
| json/serde_json to Value | 12.4 ms | 1.26 ms | 11.0 ms | 202.7 MB/s |
| passes on CSV/every pass | 1.71 ms | 37.0 µs | 1.67 ms |  |
| passes on CSV/without dispatch | 1.73 ms | 123 µs | 1.61 ms |  |
| passes on CSV/without hasher | 1.74 ms | 64.5 µs | 1.67 ms |  |
| passes on CSV/without inline | 1.94 ms | 102 µs | 1.86 ms |  |
| passes on CSV/without left_loop | 1.75 ms | 45.3 µs | 1.69 ms |  |
| passes on CSV/without skip | 1.74 ms | 54.2 µs | 1.68 ms |  |
| passes on CSV/without span | 1.85 ms | 49.1 µs | 1.78 ms |  |
| passes on CSV/without split | 2.14 ms | 41.0 µs | 2.08 ms |  |
| passes on expressions/every pass | 11.6 ms | 808 µs | 10.9 ms | 8.6 MB/s |
| passes on expressions/without dispatch | 17.5 ms | 1.01 ms | 16.5 ms | 5.7 MB/s |
| passes on expressions/without hasher | 12.4 ms | 544 µs | 11.7 ms | 8.1 MB/s |
| passes on expressions/without inline | 13.4 ms | 1.90 ms | 10.9 ms | 7.5 MB/s |
| passes on expressions/without left_loop | 44.5 ms | 5.06 ms | 39.4 ms | 2.2 MB/s |
| passes on expressions/without skip | 14.8 ms | 1.43 ms | 12.8 ms | 6.8 MB/s |
| passes on expressions/without span | 12.7 ms | 1.17 ms | 11.5 ms | 7.9 MB/s |
| passes on expressions/without split | 12.0 ms | 999 µs | 10.4 ms | 8.4 MB/s |
| passes on JSON/every pass | 32.7 ms | 2.18 ms | 30.4 ms | 76.6 MB/s |
| passes on JSON/without dispatch | 34.7 ms | 2.32 ms | 31.8 ms | 72.3 MB/s |
| passes on JSON/without hasher | 32.6 ms | 1.83 ms | 29.8 ms | 76.9 MB/s |
| passes on JSON/without inline | 47.7 ms | 2.83 ms | 44.5 ms | 52.5 MB/s |
| passes on JSON/without left_loop | 32.0 ms | 2.17 ms | 29.6 ms | 78.5 MB/s |
| passes on JSON/without skip | 41.1 ms | 2.36 ms | 38.3 ms | 61.0 MB/s |
| passes on JSON/without span | 33.6 ms | 2.05 ms | 30.8 ms | 74.5 MB/s |
| passes on JSON/without split | 41.5 ms | 2.84 ms | 38.3 ms | 60.5 MB/s |
| passes on Lua/every pass | 7.11 ms | 169 µs | 6.95 ms |  |
| passes on Lua/without dispatch | 11.9 ms | 749 µs | 10.9 ms |  |
| passes on Lua/without hasher | 7.85 ms | 352 µs | 7.46 ms |  |
| passes on Lua/without inline | 7.20 ms | 216 µs | 6.96 ms |  |
| passes on Lua/without left_loop | 18.9 ms | 1.54 ms | 16.5 ms |  |
| passes on Lua/without skip | 8.44 ms | 252 µs | 8.08 ms |  |
| passes on Lua/without span | 7.49 ms | 205 µs | 7.24 ms |  |
| passes on Lua/without split | 7.20 ms | 364 µs | 6.86 ms |  |
| passes on Python/every pass | 9.61 ms | 160 µs | 9.36 ms | 205,102 lines/s |
| passes on Python/without dispatch | 16.2 ms | 483 µs | 15.6 ms | 121,577 lines/s |
| passes on Python/without hasher | 11.3 ms | 902 µs | 10.3 ms | 174,677 lines/s |
| passes on Python/without inline | 9.71 ms | 321 µs | 9.40 ms | 202,995 lines/s |
| passes on Python/without left_loop | 14.5 ms | 310 µs | 14.1 ms | 136,250 lines/s |
| passes on Python/without skip | 11.7 ms | 319 µs | 11.4 ms | 168,057 lines/s |
| passes on Python/without span | 10.7 ms | 911 µs | 9.94 ms | 184,816 lines/s |
| passes on Python/without split | 9.74 ms | 243 µs | 9.46 ms | 202,348 lines/s |
| passes on SQL/every pass | 6.59 ms | 510 µs | 5.99 ms |  |
| passes on SQL/without dispatch | 6.55 ms | 444 µs | 6.11 ms |  |
| passes on SQL/without hasher | 5.91 ms | 489 µs | 5.30 ms |  |
| passes on SQL/without inline | 5.32 ms | 447 µs | 4.79 ms |  |
| passes on SQL/without left_loop | 11.9 ms | 1.48 ms | 10.3 ms |  |
| passes on SQL/without skip | 6.33 ms | 603 µs | 5.68 ms |  |
| passes on SQL/without span | 5.19 ms | 177 µs | 4.93 ms |  |
| passes on SQL/without split | 5.14 ms | 499 µs | 4.62 ms |  |
| passes on the grammar files/every pass | 4.94 ms | 97.1 µs | 4.83 ms |  |
| passes on the grammar files/without dispatch | 9.04 ms | 292 µs | 8.54 ms |  |
| passes on the grammar files/without hasher | 4.93 ms | 101 µs | 4.81 ms |  |
| passes on the grammar files/without inline | 5.24 ms | 196 µs | 5.02 ms |  |
| passes on the grammar files/without left_loop | 5.07 ms | 285 µs | 4.80 ms |  |
| passes on the grammar files/without skip | 5.72 ms | 87.6 µs | 5.63 ms |  |
| passes on the grammar files/without span | 5.57 ms | 230 µs | 5.36 ms |  |
| passes on the grammar files/without split | 4.95 ms | 118 µs | 4.81 ms |  |
| passes on the outline language/every pass | 1.41 ms | 61.2 µs | 1.35 ms |  |
| passes on the outline language/without dispatch | 1.91 ms | 177 µs | 1.73 ms |  |
| passes on the outline language/without hasher | 1.46 ms | 110 µs | 1.36 ms |  |
| passes on the outline language/without inline | 1.63 ms | 162 µs | 1.41 ms |  |
| passes on the outline language/without left_loop | 1.42 ms | 86.5 µs | 1.33 ms |  |
| passes on the outline language/without skip | 2.45 ms | 465 µs | 1.90 ms |  |
| passes on the outline language/without span | 1.96 ms | 202 µs | 1.74 ms |  |
| passes on the outline language/without split | 1.41 ms | 86.6 µs | 1.32 ms |  |
| passes on tiny/every pass | 3.92 ms | 126 µs | 3.75 ms |  |
| passes on tiny/without dispatch | 5.13 ms | 163 µs | 4.96 ms |  |
| passes on tiny/without hasher | 4.25 ms | 167 µs | 4.00 ms |  |
| passes on tiny/without inline | 4.09 ms | 179 µs | 3.87 ms |  |
| passes on tiny/without left_loop | 11.8 ms | 1.27 ms | 9.78 ms |  |
| passes on tiny/without skip | 4.69 ms | 213 µs | 4.44 ms |  |
| passes on tiny/without span | 4.48 ms | 175 µs | 4.33 ms |  |
| passes on tiny/without split | 3.87 ms | 56.9 µs | 3.77 ms |  |
| passes on TOML/every pass | 2.76 ms | 88.8 µs | 2.66 ms |  |
| passes on TOML/without dispatch | 3.13 ms | 162 µs | 2.96 ms |  |
| passes on TOML/without hasher | 2.86 ms | 112 µs | 2.73 ms |  |
| passes on TOML/without inline | 2.92 ms | 86.0 µs | 2.82 ms |  |
| passes on TOML/without left_loop | 2.84 ms | 110 µs | 2.74 ms |  |
| passes on TOML/without skip | 3.45 ms | 95.5 µs | 3.33 ms |  |
| passes on TOML/without span | 3.14 ms | 78.2 µs | 3.04 ms |  |
| passes on TOML/without split | 2.86 ms | 80.1 µs | 2.76 ms |  |
| pest_vm grammar load | 774 µs | 154 µs | 575 µs |  |

## rust-pgo

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| a parse that starts again with every rule remembered | 2.23 ms | 138 µs | 2.10 ms |  |
| expressions/fpeg machine, every rule remembered, tree | 26.8 ms | 551 µs | 26.0 ms | 3.7 MB/s |
| expressions/fpeg machine, tree | 10.9 ms | 234 µs | 10.6 ms | 9.2 MB/s |
| fpeg grammar load | 173 µs | 25.0 µs | 147 µs |  |
| fpeg machine on a small sum | 1.34 µs | 124 ns | 1.23 µs |  |
| fpeg machine on a small sum, nothing remembered | 2.85 µs | 206 ns | 2.64 µs |  |
| json/fpeg machine with its profile, tree | 411 ms | 44.5 ms | 343 ms | 6.1 MB/s |
| json/fpeg machine, tree | 26.9 ms | 600 µs | 26.1 ms | 93.0 MB/s |
| json/pest generated, token queue | 77.5 ms | 4.61 ms | 72.9 ms | 32.3 MB/s |
| json/pest_vm interpreter, token queue | 338 ms | 18.9 ms | 319 ms | 7.4 MB/s |
| json/serde_json to Value | 9.62 ms | 801 µs | 8.50 ms | 260.6 MB/s |
| passes on CSV/every pass | 1.55 ms | 99.4 µs | 1.45 ms |  |
| passes on CSV/without dispatch | 1.55 ms | 103 µs | 1.45 ms |  |
| passes on CSV/without hasher | 1.54 ms | 86.9 µs | 1.45 ms |  |
| passes on CSV/without inline | 1.90 ms | 106 µs | 1.78 ms |  |
| passes on CSV/without left_loop | 1.58 ms | 118 µs | 1.44 ms |  |
| passes on CSV/without skip | 1.51 ms | 65.0 µs | 1.42 ms |  |
| passes on CSV/without span | 1.71 ms | 59.6 µs | 1.65 ms |  |
| passes on CSV/without split | 2.12 ms | 139 µs | 1.95 ms |  |
| passes on expressions/every pass | 11.0 ms | 942 µs | 9.88 ms | 9.1 MB/s |
| passes on expressions/without dispatch | 17.1 ms | 1.40 ms | 15.6 ms | 5.8 MB/s |
| passes on expressions/without hasher | 10.9 ms | 550 µs | 10.3 ms | 9.2 MB/s |
| passes on expressions/without inline | 10.8 ms | 603 µs | 9.89 ms | 9.3 MB/s |
| passes on expressions/without left_loop | 36.7 ms | 2.99 ms | 32.8 ms | 2.7 MB/s |
| passes on expressions/without skip | 12.7 ms | 1.13 ms | 11.6 ms | 7.9 MB/s |
| passes on expressions/without span | 11.6 ms | 805 µs | 10.3 ms | 8.6 MB/s |
| passes on expressions/without split | 11.3 ms | 968 µs | 9.54 ms | 8.9 MB/s |
| passes on JSON/every pass | 26.9 ms | 1.68 ms | 24.7 ms | 93.2 MB/s |
| passes on JSON/without dispatch | 30.5 ms | 2.14 ms | 28.1 ms | 82.3 MB/s |
| passes on JSON/without hasher | 26.5 ms | 1.17 ms | 24.8 ms | 94.7 MB/s |
| passes on JSON/without inline | 46.4 ms | 2.87 ms | 43.2 ms | 54.1 MB/s |
| passes on JSON/without left_loop | 26.9 ms | 2.18 ms | 24.4 ms | 93.1 MB/s |
| passes on JSON/without skip | 45.1 ms | 3.09 ms | 41.2 ms | 55.6 MB/s |
| passes on JSON/without span | 28.2 ms | 1.86 ms | 25.5 ms | 89.0 MB/s |
| passes on JSON/without split | 37.4 ms | 1.78 ms | 35.2 ms | 67.0 MB/s |
| passes on Lua/every pass | 7.17 ms | 665 µs | 6.23 ms |  |
| passes on Lua/without dispatch | 11.8 ms | 870 µs | 10.9 ms |  |
| passes on Lua/without hasher | 6.92 ms | 425 µs | 6.54 ms |  |
| passes on Lua/without inline | 7.99 ms | 349 µs | 7.26 ms |  |
| passes on Lua/without left_loop | 20.2 ms | 756 µs | 19.1 ms |  |
| passes on Lua/without skip | 7.98 ms | 797 µs | 7.22 ms |  |
| passes on Lua/without span | 6.98 ms | 368 µs | 6.49 ms |  |
| passes on Lua/without split | 7.17 ms | 479 µs | 6.64 ms |  |
| passes on Python/every pass | 9.25 ms | 556 µs | 8.48 ms | 212,985 lines/s |
| passes on Python/without dispatch | 15.8 ms | 1.39 ms | 14.3 ms | 125,115 lines/s |
| passes on Python/without hasher | 9.37 ms | 334 µs | 9.04 ms | 210,430 lines/s |
| passes on Python/without inline | 9.04 ms | 308 µs | 8.68 ms | 217,925 lines/s |
| passes on Python/without left_loop | 13.3 ms | 771 µs | 12.3 ms | 148,474 lines/s |
| passes on Python/without skip | 11.4 ms | 1.18 ms | 10.0 ms | 173,116 lines/s |
| passes on Python/without span | 9.64 ms | 453 µs | 9.01 ms | 204,469 lines/s |
| passes on Python/without split | 9.51 ms | 676 µs | 8.73 ms | 207,347 lines/s |
| passes on SQL/every pass | 4.95 ms | 224 µs | 4.63 ms |  |
| passes on SQL/without dispatch | 6.64 ms | 355 µs | 6.11 ms |  |
| passes on SQL/without hasher | 5.27 ms | 465 µs | 4.77 ms |  |
| passes on SQL/without inline | 5.02 ms | 613 µs | 4.46 ms |  |
| passes on SQL/without left_loop | 9.66 ms | 736 µs | 8.72 ms |  |
| passes on SQL/without skip | 5.86 ms | 289 µs | 5.47 ms |  |
| passes on SQL/without span | 5.24 ms | 267 µs | 4.94 ms |  |
| passes on SQL/without split | 4.97 ms | 438 µs | 4.53 ms |  |
| passes on the grammar files/every pass | 4.69 ms | 210 µs | 4.47 ms |  |
| passes on the grammar files/without dispatch | 9.18 ms | 870 µs | 8.34 ms |  |
| passes on the grammar files/without hasher | 4.90 ms | 355 µs | 4.60 ms |  |
| passes on the grammar files/without inline | 5.04 ms | 224 µs | 4.82 ms |  |
| passes on the grammar files/without left_loop | 4.55 ms | 189 µs | 4.39 ms |  |
| passes on the grammar files/without skip | 5.77 ms | 249 µs | 5.49 ms |  |
| passes on the grammar files/without span | 5.35 ms | 502 µs | 4.99 ms |  |
| passes on the grammar files/without split | 4.62 ms | 404 µs | 4.35 ms |  |
| passes on the outline language/every pass | 1.31 ms | 93.8 µs | 1.21 ms |  |
| passes on the outline language/without dispatch | 1.29 ms | 64.1 µs | 1.22 ms |  |
| passes on the outline language/without hasher | 1.29 ms | 85.6 µs | 1.21 ms |  |
| passes on the outline language/without inline | 1.38 ms | 107 µs | 1.25 ms |  |
| passes on the outline language/without left_loop | 1.32 ms | 103 µs | 1.21 ms |  |
| passes on the outline language/without skip | 1.96 ms | 97.3 µs | 1.85 ms |  |
| passes on the outline language/without span | 1.93 ms | 63.3 µs | 1.87 ms |  |
| passes on the outline language/without split | 1.40 ms | 98.5 µs | 1.29 ms |  |
| passes on tiny/every pass | 3.59 ms | 230 µs | 3.33 ms |  |
| passes on tiny/without dispatch | 4.98 ms | 418 µs | 4.51 ms |  |
| passes on tiny/without hasher | 3.77 ms | 378 µs | 3.33 ms |  |
| passes on tiny/without inline | 3.57 ms | 221 µs | 3.31 ms |  |
| passes on tiny/without left_loop | 8.70 ms | 411 µs | 8.25 ms |  |
| passes on tiny/without skip | 4.20 ms | 159 µs | 3.94 ms |  |
| passes on tiny/without span | 4.15 ms | 180 µs | 3.92 ms |  |
| passes on tiny/without split | 3.74 ms | 303 µs | 3.43 ms |  |
| passes on TOML/every pass | 2.67 ms | 146 µs | 2.48 ms |  |
| passes on TOML/without dispatch | 2.98 ms | 197 µs | 2.74 ms |  |
| passes on TOML/without hasher | 2.76 ms | 175 µs | 2.47 ms |  |
| passes on TOML/without inline | 2.91 ms | 303 µs | 2.66 ms |  |
| passes on TOML/without left_loop | 2.66 ms | 183 µs | 2.45 ms |  |
| passes on TOML/without skip | 3.47 ms | 185 µs | 3.14 ms |  |
| passes on TOML/without span | 3.14 ms | 94.2 µs | 3.00 ms |  |
| passes on TOML/without split | 2.78 ms | 192 µs | 2.55 ms |  |
| pest_vm grammar load | 670 µs | 100 µs | 586 µs |  |
