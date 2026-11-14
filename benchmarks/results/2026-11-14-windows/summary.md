# Benchmark results 2026-11-14-windows

Written by `benchmarks/run.py` in default mode at commit 91425fb.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines; expressions.c 100,054 bytes, 1,488 lines; calculator.txt 94,015 bytes, 2,000 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.5 ms | 616 µs | 9.82 ms | 237.8 MB/s |
| fpeg grammar load | 269 µs | 35.1 µs | 242 µs |  |
| fpeg parse, tree | 70.3 ms | 6.32 ms | 63.2 ms | 35.7 MB/s |
| fpeg parse and grammar builders, Python objects | 82.7 ms | 2.66 ms | 78.5 ms | 30.3 MB/s |
| fpeg parse and Transformer methods, Python objects | 121 ms | 11.0 ms | 113 ms | 20.8 MB/s |
| Lark grammar load | 5.16 ms | 388 µs | 4.84 ms |  |
| Lark LALR, tree | 1.15 s | 32.0 ms | 1.11 s | 2.2 MB/s |
| Lark Transformer over the tree | 378 ms | 27.5 ms | 344 ms | 6.6 MB/s |
| Lark LALR with inlined Transformer, Python objects | 980 ms | 22.4 ms | 937 ms | 2.6 MB/s |
| pe machine grammar load | 2.47 ms | 211 µs | 2.25 ms |  |
| pe machine, match only | 144 ms | 17.9 ms | 127 ms | 17.4 MB/s |
| pe machine, Python objects | 195 ms | 30.0 ms | 175 ms | 12.8 MB/s |
| pe packrat grammar load | 2.57 ms | 124 µs | 2.49 ms |  |
| pe packrat, match only | 455 ms | 19.4 ms | 425 ms | 5.5 MB/s |
| pe packrat, Python objects | 469 ms | 19.1 ms | 442 ms | 5.3 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 151 µs | 16.0 µs | 136 µs |  |
| transform, a method on every node, list | 2.23 ms | 160 µs | 2.06 ms |  |
| transform, a method on every node, *children | 2.21 ms | 192 µs | 2.04 ms |  |
| transform, the same with a member by field | 2.39 ms | 352 µs | 2.17 ms |  |
| transform, a method on every token, taking nothing | 2.43 ms | 142 µs | 2.30 ms |  |
| transform, a method on every token, taking it | 3.90 ms | 432 µs | 3.53 ms |  |
| interpreter, no method | 9.64 ms | 742 µs | 9.07 ms |  |
| interpreter, a method on every node | 11.7 ms | 1.52 ms | 10.3 ms |  |
| visitor, no method | 2.12 ms | 124 µs | 2.03 ms |  |
| visitor, a method on every node | 3.20 ms | 161 µs | 3.10 ms |  |
| parse | 6.76 ms | 382 µs | 6.46 ms |  |
| parse, then every view and token | 12.1 ms | 667 µs | 11.3 ms |  |
| 2000 small CSV parses with a transformer | 6.54 ms | 637 µs | 5.91 ms |  |
| 2000 small CSV parses with csv.reader | 1.62 ms | 92.6 µs | 1.55 ms |  |

## calculator

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| eval, a line at a time | 17.3 ms | 1.46 ms | 16.2 ms | 5.4 MB/s |
| fpeg parse, tree | 16.1 ms | 1.29 ms | 15.1 ms | 5.8 MB/s |
| fpeg parse and Transformer, numbers | 19.4 ms | 2.19 ms | 17.6 ms | 4.8 MB/s |
| Lark LALR, tree | 100 ms | 9.59 ms | 92.4 ms | 0.9 MB/s |
| Lark LALR with inlined Transformer, numbers | 97.1 ms | 10.2 ms | 87.9 ms | 1.0 MB/s |
| pe machine, numbers | 48.7 ms | 4.45 ms | 44.3 ms | 1.9 MB/s |
| pe packrat, numbers | 102 ms | 5.54 ms | 96.7 ms | 0.9 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.81 ms | 764 µs | 6.94 ms | 252,335 lines/s |
| CPython parser and symbol table, no AST objects | 5.68 ms | 444 µs | 5.23 ms | 347,077 lines/s |
| compile to bytecode | 8.78 ms | 839 µs | 8.08 ms | 224,433 lines/s |
| tree-sitter parse, native tree | 7.32 ms | 613 µs | 6.81 ms | 269,306 lines/s |
| tree-sitter cursor walk over every node | 2.45 ms | 171 µs | 2.28 ms |  |
| tree-sitter walk, a Python Node per node | 2.93 ms | 140 µs | 2.76 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.95 ms | 520 µs | 4.50 ms |  |
| LibCST parse_module | 81.3 ms | 4.86 ms | 75.3 ms | 24,237 lines/s |
| Lark Python grammar load | 244 ms | 22.7 ms | 227 ms |  |
| Lark LALR, bundled Python grammar, tree | 81.5 ms | 7.75 ms | 76.8 ms | 24,186 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 547 ms | 27.2 ms | 501 ms |  |
| grammar load with a warm cache | 28.5 ms | 1.75 ms | 26.8 ms |  |
| LALR parse, tree | 145 ms | 13.7 ms | 135 ms | 13,582 lines/s |
| Transformer with the real callbacks | 17.9 ms | 1.74 ms | 16.5 ms |  |
| Transformer, trivial method on every rule | 9.79 ms | 988 µs | 8.61 ms |  |
| end to end: parse, transform, fix locations | 173 ms | 17.2 ms | 159 ms | 11,404 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| expressions/fpeg machine, every rule remembered, tree | 83.7 ms | 3.70 ms | 80.1 ms | 1.2 MB/s |
| expressions/fpeg machine, tree | 69.6 ms | 2.95 ms | 65.9 ms | 1.4 MB/s |
| fpeg grammar load | 258 µs | 26.2 µs | 237 µs |  |
| fpeg machine on a small sum | 5.11 µs | 542 ns | 4.71 µs |  |
| fpeg machine on a small sum, nothing remembered | 5.53 ms | 1.04 ms | 5.05 ms |  |
| json/fpeg machine with its profile, tree | 315 ms | 8.68 ms | 305 ms | 7.9 MB/s |
| json/fpeg machine, tree | 67.7 ms | 1.24 ms | 65.9 ms | 37.0 MB/s |
| json/pest generated, token queue | 58.1 ms | 2.53 ms | 54.5 ms | 43.2 MB/s |
| json/pest_vm interpreter, token queue | 311 ms | 10.5 ms | 295 ms | 8.1 MB/s |
| json/serde_json to Value | 9.39 ms | 1.05 ms | 8.23 ms | 266.9 MB/s |
| pest_vm grammar load | 602 µs | 125 µs | 546 µs |  |
