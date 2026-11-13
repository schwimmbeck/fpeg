# Benchmark results 2026-11-13-linux

Written by `benchmarks/run.py` in default mode at commit c21f10d.
Times are per call.

- Machine: Linux-6.18.33.2-microsoft-standard-WSL2-x86_64-with-glibc2.36, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.15.2, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.3 ms | 838 µs | 9.38 ms | 243.7 MB/s |
| fpeg grammar load | 259 µs | 29.9 µs | 229 µs |  |
| fpeg parse, tree | 63.5 ms | 5.22 ms | 58.7 ms | 39.5 MB/s |
| fpeg parse and grammar builders, Python objects | 80.9 ms | 6.53 ms | 72.7 ms | 31.0 MB/s |
| fpeg parse and Transformer methods, Python objects | 110 ms | 17.1 ms | 100 ms | 22.7 MB/s |
| Lark grammar load | 4.66 ms | 677 µs | 4.29 ms |  |
| Lark LALR, tree | 1.13 s | 18.8 ms | 1.09 s | 2.2 MB/s |
| Lark Transformer over the tree | 337 ms | 30.4 ms | 303 ms | 7.4 MB/s |
| Lark LALR with inlined Transformer, Python objects | 917 ms | 35.1 ms | 868 ms | 2.7 MB/s |
| pe machine grammar load | 2.09 ms | 85.5 µs | 2.00 ms |  |
| pe machine, match only | 113 ms | 9.52 ms | 105 ms | 22.2 MB/s |
| pe machine, Python objects | 159 ms | 19.5 ms | 146 ms | 15.8 MB/s |
| pe packrat grammar load | 2.49 ms | 444 µs | 2.24 ms |  |
| pe packrat, match only | 432 ms | 37.7 ms | 389 ms | 5.8 MB/s |
| pe packrat, Python objects | 432 ms | 26.6 ms | 404 ms | 5.8 MB/s |

## binding

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| transform, no method | 136 µs | 31.2 µs | 118 µs |  |
| transform, a method on every node, list | 1.92 ms | 157 µs | 1.75 ms |  |
| transform, a method on every node, *children | 1.80 ms | 286 µs | 1.64 ms |  |
| transform, the same with a member by field | 1.87 ms | 267 µs | 1.69 ms |  |
| transform, a method on every token, taking nothing | 2.42 ms | 293 µs | 2.22 ms |  |
| transform, a method on every token, taking it | 3.55 ms | 161 µs | 3.39 ms |  |
| parse | 5.66 ms | 577 µs | 5.31 ms |  |
| parse, then every view and token | 10.4 ms | 1.37 ms | 9.57 ms |  |
| 2000 small CSV parses with a transformer | 4.74 ms | 706 µs | 4.34 ms |  |
| 2000 small CSV parses with csv.reader | 1.33 ms | 147 µs | 1.21 ms |  |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.54 ms | 933 µs | 7.03 ms | 261,279 lines/s |
| CPython parser and symbol table, no AST objects | 5.30 ms | 664 µs | 4.88 ms | 372,188 lines/s |
| compile to bytecode | 6.71 ms | 619 µs | 6.41 ms | 293,596 lines/s |
| tree-sitter parse, native tree | 5.89 ms | 603 µs | 5.53 ms | 334,449 lines/s |
| tree-sitter cursor walk over every node | 1.33 ms | 179 µs | 1.19 ms |  |
| tree-sitter walk, a Python Node per node | 1.88 ms | 329 µs | 1.71 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 3.89 ms | 870 µs | 3.36 ms |  |
| LibCST parse_module | 62.1 ms | 6.78 ms | 57.9 ms | 31,727 lines/s |
| Lark Python grammar load | 214 ms | 6.71 ms | 206 ms |  |
| Lark LALR, bundled Python grammar, tree | 80.0 ms | 12.8 ms | 72.8 ms | 24,645 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 495 ms | 39.0 ms | 456 ms |  |
| grammar load with a warm cache | 27.5 ms | 4.44 ms | 24.9 ms |  |
| LALR parse, tree | 138 ms | 4.60 ms | 134 ms | 14,289 lines/s |
| Transformer with the real callbacks | 16.5 ms | 1.95 ms | 15.4 ms |  |
| Transformer, trivial method on every rule | 8.03 ms | 1.18 ms | 7.52 ms |  |
| end to end: parse, transform, fix locations | 164 ms | 16.9 ms | 152 ms | 12,023 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| fpeg grammar load | 245 µs | 56.7 µs | 215 µs |  |
| json/fpeg machine, tree | 64.4 ms | 3.20 ms | 61.3 ms | 38.9 MB/s |
| json/pest generated, token queue | 51.2 ms | 1.68 ms | 48.8 ms | 49.0 MB/s |
| json/pest_vm interpreter, token queue | 257 ms | 6.11 ms | 247 ms | 9.8 MB/s |
| json/serde_json to Value | 6.91 ms | 200 µs | 6.66 ms | 362.6 MB/s |
| pest_vm grammar load | 390 µs | 61.8 µs | 347 µs |  |
