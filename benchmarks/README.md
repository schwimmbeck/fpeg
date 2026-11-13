# Benchmarks

The benchmark harness, the baseline suites and the recorded results. How to run them is in `docs/development.md`.

| Path | What it holds |
|---|---|
| `run.py` | runs the suites and writes `results.json`, `summary.md` and the raw pyperf files |
| `corpus.py` | generates or downloads the inputs into `corpora/` and checks them against pinned hashes |
| `harness.py` | the shared part of the pyperf scripts |
| `bench_json.py` | JSON: fpeg, `json.loads`, Lark and pe on a generated 2.5 MB file |
| `bench_binding.py` | fpeg's Python binding apart from parsing: transforms with and without methods, tokens from views, many small parses against `csv` |
| `bench_python.py` | Python source: CPython, tree-sitter, LibCST and Lark on one real source file |
| `bench_ord.py` | the first real application: its Lark pipeline from source text to a Python `ast` |
| `rust/` | JSON in Rust with criterion: fpeg's parsing machine, pest generated, pest_vm and serde_json |
| `results/<date>-<platform>/` | recorded runs, committed as data |

`corpora/` and `.cache/` are local and ignored by git.

## Recorded results

Each folder under `results/` holds one run:

- `summary.md`, the tables for people.
- `results.json`, the same numbers for tools: the commit, the machine, the Python and Rust versions, the inputs with size and hash, and one row per benchmark with its mean, standard deviation, median and minimum in seconds per call, and the number of measured values.
- `<suite>.pyperf.json`, pyperf's own file for each Python suite, for `python -m pyperf compare_to` between runs. The host name and the home folder are removed.

Numbers in the documentation come from these files, never from memory.
