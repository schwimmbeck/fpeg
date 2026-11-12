# Benchmark results 2026-11-12-linux

Written by `benchmarks/run.py` in default mode at commit 8a174ef.
Times are per call.

- Machine: Linux-6.18.33.2-microsoft-standard-WSL2-x86_64-with-glibc2.36, AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical CPUs
- CPython 3.13.18, rustc 1.98.1 (48a229cea 2026-09-01)
- Python packages: lark 1.3.1, pe 0.6.0, libcst 1.9.0, tree-sitter 0.26.1, tree-sitter-python 0.25.0, pyperf 2.11.0
- Rust crates: criterion 0.8.2, pest 2.9.6, pest_derive 2.9.6, pest_meta 2.9.6, pest_vm 2.9.6, serde_json 1.0.154
- Inputs: records.json 2,506,935 bytes, 153,001 lines; python_transformer.py 69,525 bytes, 1,971 lines

## json

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json.loads | 10.8 ms | 1.53 ms | 9.58 ms | 231.9 MB/s |
| Lark grammar load | 4.94 ms | 470 µs | 4.51 ms |  |
| Lark LALR, tree | 1.18 s | 45.4 ms | 1.13 s | 2.1 MB/s |
| Lark Transformer over the tree | 337 ms | 33.6 ms | 311 ms | 7.4 MB/s |
| Lark LALR with inlined Transformer, Python objects | 959 ms | 46.5 ms | 911 ms | 2.6 MB/s |
| pe machine grammar load | 2.19 ms | 128 µs | 2.06 ms |  |
| pe machine, match only | 114 ms | 11.8 ms | 105 ms | 21.9 MB/s |
| pe machine, Python objects | 178 ms | 30.7 ms | 154 ms | 14.1 MB/s |
| pe packrat grammar load | 2.83 ms | 768 µs | 2.32 ms |  |
| pe packrat, match only | 429 ms | 27.5 ms | 389 ms | 5.8 MB/s |
| pe packrat, Python objects | 432 ms | 23.1 ms | 403 ms | 5.8 MB/s |

## python

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| ast.parse | 7.19 ms | 276 µs | 6.86 ms | 274,080 lines/s |
| CPython parser and symbol table, no AST objects | 4.95 ms | 691 µs | 4.49 ms | 398,287 lines/s |
| compile to bytecode | 6.42 ms | 498 µs | 6.02 ms | 306,791 lines/s |
| tree-sitter parse, native tree | 5.36 ms | 388 µs | 5.03 ms | 367,477 lines/s |
| tree-sitter cursor walk over every node | 1.06 ms | 128 µs | 965 µs |  |
| tree-sitter walk, a Python Node per node | 1.66 ms | 80.1 µs | 1.58 ms |  |
| tree-sitter walk, a Node per node and the text of every leaf | 3.60 ms | 503 µs | 3.25 ms |  |
| LibCST parse_module | 64.9 ms | 7.25 ms | 58.2 ms | 30,383 lines/s |
| Lark Python grammar load | 238 ms | 15.1 ms | 223 ms |  |
| Lark LALR, bundled Python grammar, tree | 82.2 ms | 9.34 ms | 75.2 ms | 23,972 lines/s |

## ord

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| grammar load without cache | 548 ms | 32.3 ms | 497 ms |  |
| grammar load with a warm cache | 28.4 ms | 2.14 ms | 26.4 ms |  |
| LALR parse, tree | 147 ms | 11.4 ms | 135 ms | 13,408 lines/s |
| Transformer with the real callbacks | 17.6 ms | 1.03 ms | 16.5 ms |  |
| Transformer, trivial method on every rule | 8.31 ms | 1.06 ms | 7.71 ms |  |
| end to end: parse, transform, fix locations | 171 ms | 16.0 ms | 155 ms | 11,554 lines/s |

## rust

| Benchmark | Mean | Std dev | Min | Throughput |
|---|--:|--:|--:|--:|
| json/pest generated, token queue | 54.1 ms | 3.25 ms | 50.3 ms | 46.3 MB/s |
| json/pest_vm interpreter, token queue | 250 ms | 11.3 ms | 239 ms | 10.0 MB/s |
| json/serde_json to Value | 13.7 ms | 993 µs | 12.9 ms | 182.6 MB/s |
| pest_vm grammar load | 400 µs | 92.7 µs | 359 µs |  |
