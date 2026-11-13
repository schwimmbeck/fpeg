# Benchmark results 2026-11-13-windows

Written by `benchmarks/run.py` in default mode at commit c21f10d.
Times are per call.

- Machine: Windows-11-10.0.26200-SP0, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.9 ms | 990 µs | 9.91 ms | 230.4 MB/s |
| fpeg grammar load | 238 µs | 18.4 µs | 222 µs |  |
| fpeg parse, tree | 64.7 ms | 6.13 ms | 60.5 ms | 38.8 MB/s |
| fpeg parse and grammar builders, Python objects | 80.3 ms | 4.41 ms | 76.6 ms | 31.2 MB/s |
| fpeg parse and Transformer methods, Python objects | 114 ms | 6.26 ms | 108 ms | 22.0 MB/s |
| Lark grammar load | 5.15 ms | 338 µs | 4.89 ms |  |
| Lark LALR, tree | 1.18 s | 42.5 ms | 1.11 s | 2.1 MB/s |
| Lark Transformer over the tree | 384 ms | 19.3 ms | 357 ms | 6.5 MB/s |
| Lark LALR with inlined Transformer, Python objects | 974 ms | 28.5 ms | 945 ms | 2.6 MB/s |
| pe machine grammar load | 2.65 ms | 297 µs | 2.34 ms |  |
| pe machine, match only | 139 ms | 15.3 ms | 128 ms | 18.0 MB/s |
| pe machine, Python objects | 192 ms | 17.6 ms | 175 ms | 13.1 MB/s |
| pe packrat grammar load | 2.72 ms | 174 µs | 2.56 ms |  |
| pe packrat, match only | 486 ms | 26.3 ms | 450 ms | 5.2 MB/s |
| pe packrat, Python objects | 493 ms | 17.8 ms | 466 ms | 5.1 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 152 µs | 21.6 µs | 137 µs |  |
| transform, a method on every node, list | 2.22 ms | 185 µs | 2.03 ms |  |
| transform, a method on every node, *children | 2.12 ms | 146 µs | 2.02 ms |  |
| transform, the same with a member by field | 2.27 ms | 421 µs | 1.96 ms |  |
| transform, a method on every token, taking nothing | 2.47 ms | 201 µs | 2.32 ms |  |
| transform, a method on every token, taking it | 3.83 ms | 461 µs | 3.56 ms |  |
| parse | 6.44 ms | 315 µs | 6.12 ms |  |
| parse, then every view and token | 11.9 ms | 1.10 ms | 10.9 ms |  |
| 2000 small CSV parses with a transformer | 6.12 ms | 646 µs | 5.73 ms |  |
| 2000 small CSV parses with csv.reader | 1.68 ms | 147 µs | 1.57 ms |  |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.85 ms | 812 µs | 7.28 ms | 251,095 lines/s |
| CPython parser and symbol table, no AST objects | 5.85 ms | 323 µs | 5.43 ms | 337,165 lines/s |
| compile to bytecode | 8.89 ms | 740 µs | 8.39 ms | 221,689 lines/s |
| tree-sitter parse, native tree | 6.97 ms | 207 µs | 6.78 ms | 282,648 lines/s |
| tree-sitter cursor walk over every node | 2.40 ms | 150 µs | 2.26 ms |  |
| tree-sitter walk, a Python Node per node | 2.89 ms | 240 µs | 2.71 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 4.75 ms | 372 µs | 4.46 ms |  |
| LibCST parse_module | 81.0 ms | 3.66 ms | 76.6 ms | 24,339 lines/s |
| Lark Python grammar load | 244 ms | 17.7 ms | 227 ms |  |
| Lark LALR, bundled Python grammar, tree | 81.0 ms | 4.39 ms | 77.1 ms | 24,323 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 555 ms | 27.3 ms | 504 ms |  |
| grammar load with a warm cache | 29.9 ms | 4.06 ms | 27.0 ms |  |
| LALR parse, tree | 144 ms | 16.3 ms | 134 ms | 13,706 lines/s |
| Transformer with the real callbacks | 18.3 ms | 1.98 ms | 17.1 ms |  |
| Transformer, trivial method on every rule | 9.57 ms | 537 µs | 8.93 ms |  |
| end to end: parse, transform, fix locations | 174 ms | 19.6 ms | 157 ms | 11,337 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| fpeg grammar load | 253 µs | 36.1 µs | 229 µs |  |
| json/fpeg machine, tree | 69.6 ms | 1.89 ms | 67.8 ms | 36.0 MB/s |
| json/pest generated, token queue | 57.3 ms | 1.91 ms | 54.4 ms | 43.8 MB/s |
| json/pest_vm interpreter, token queue | 302 ms | 6.93 ms | 292 ms | 8.3 MB/s |
| json/serde_json to Value | 8.70 ms | 445 µs | 8.09 ms | 288.1 MB/s |
| pest_vm grammar load | 597 µs | 137 µs | 545 µs |  |
