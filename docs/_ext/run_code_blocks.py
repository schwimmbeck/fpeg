# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Run the Python code blocks of every page while the docs build.

The blocks of one page run in order and share a namespace, so a page can build an example up
step by step. A block in the language ``output`` right after a Python block shows what that
block printed, and must equal it. A block that raises, output that differs, and a block that
prints with no output block after it are reported as warnings at their line, which fail a build
with ``-W``.
"""

import contextlib
import io
import traceback
from typing import Any

from docutils import nodes
from pygments.lexers.special import TextLexer
from sphinx.application import Sphinx
from sphinx.environment import BuildEnvironment
from sphinx.util import logging

logger = logging.getLogger(__name__)

# The names Pygments accepts for Python 3 code.
RUN_LANGUAGES = {"python", "py", "python3", "py3"}
# The language of a block that shows what the Python block before it printed.
OUTPUT_LANGUAGE = "output"


def output_lines(text: str) -> list[str]:
    """Return the lines of printed text without trailing spaces or blank lines at either end."""
    return [line.rstrip() for line in text.strip("\n").splitlines()]


def failing_line(error: Exception, filename: str) -> int:
    """Return the line of the block, counted from 1, at which the error was raised."""
    if isinstance(error, SyntaxError) and error.filename == filename:
        return error.lineno or 1
    lines = [f.lineno for f in traceback.extract_tb(error.__traceback__) if f.filename == filename]
    return (lines[-1] or 1) if lines else 1


def missing_output(start: int, docname: str) -> None:
    """Report a Python block that printed with no output block after it to show what."""
    logger.warning(
        "the block prints, and no output block after it shows what", location=(docname, start)
    )


def run_blocks(app: Sphinx, doctree: nodes.document) -> None:
    """Execute the Python blocks of the page that was just read."""
    docname = app.env.docname
    namespace: dict[str, Any] = {"__name__": f"docs.{docname}"}
    # What the Python block just before printed, if it ran, and where it starts.
    printed: str | None = None
    printed_at = 0
    for block in doctree.findall(nodes.literal_block):
        language = block.get("language")
        # The opening fence is at block.line, so code line n is at block.line + n.
        start = block.line or 0
        if language == OUTPUT_LANGUAGE:
            if printed is None:
                logger.warning(
                    "an output block follows no Python block that ran", location=(docname, start)
                )
            elif output_lines(block.astext()) != output_lines(printed):
                logger.warning(
                    "the output block differs from what the block above printed:\n%s",
                    printed,
                    location=(docname, start),
                )
        elif printed:
            missing_output(printed_at, docname)
        printed = None
        if language not in RUN_LANGUAGES:
            continue
        filename = f"<{docname} block at line {start}>"
        out = io.StringIO()
        try:
            code = compile(block.astext(), filename, "exec")
            with contextlib.redirect_stdout(out):
                exec(code, namespace)
            printed, printed_at = out.getvalue(), start
        except Exception as error:
            line = start + failing_line(error, filename)
            logger.warning(
                "code block failed: %s: %s",
                type(error).__name__,
                error,
                location=(docname, line),
            )
    if printed:
        missing_output(printed_at, docname)


def read_all(
    app: Sphinx, env: BuildEnvironment, added: set[str], changed: set[str], removed: set[str]
) -> list[str]:
    """Read every page again, since the library under an unchanged example may have changed."""
    return sorted(env.found_docs)


def setup(app: Sphinx) -> dict[str, Any]:
    """Register the extension."""
    app.add_lexer(OUTPUT_LANGUAGE, TextLexer)
    app.connect("doctree-read", run_blocks)
    app.connect("env-get-outdated", read_all)
    return {"parallel_read_safe": True, "parallel_write_safe": True}
