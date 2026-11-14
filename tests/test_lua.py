# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Lua grammar over the test suite of Lua 5.4, a corpus that must parse.

`benchmarks/corpus.py` fetches the suite and pins it by hash. Lua reads its source as bytes,
which Latin-1 maps one to one to characters, and leaves out a first line that starts with `#`.
"""

import sys
import tarfile
from pathlib import Path

import fpeg
import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "benchmarks"))

import corpus

LUA = fpeg.Grammar.open(Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "lua.fpeg")


@pytest.mark.corpus
def test_every_file_of_lua_s_test_suite_parses() -> None:
    if not corpus.matches(corpus.LUA_TESTS.path, corpus.LUA_TESTS.sha256):
        pytest.skip("benchmarks/corpus.py fetches Lua's test suite")
    parsed = 0
    failed: list[str] = []
    with tarfile.open(corpus.LUA_TESTS.path) as archive:
        for member in archive.getmembers():
            file = archive.extractfile(member)
            if file is None or not member.name.endswith(".lua"):
                continue
            source = file.read().decode("latin-1")
            if source.startswith("#"):
                # The line break stays, so that the lines keep their numbers.
                source = "\n" + source.partition("\n")[2]
            parsed += 1
            try:
                LUA.parse(source)
            except fpeg.ParseError as error:
                failed.append(f"{member.name}: {str(error).splitlines()[0]}")
    assert parsed, f"no Lua file in {corpus.LUA_TESTS.path}"
    assert not failed, f"{len(failed)} of {parsed} files fail:\n" + "\n".join(failed)
