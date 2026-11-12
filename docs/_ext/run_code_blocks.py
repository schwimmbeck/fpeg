# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""Run the Python code blocks of every page while the docs build.

The blocks of one page run in order and share a namespace, so a page can build an example up
step by step. A block that raises is reported as a warning at its line, which fails a build
with ``-W``.
"""

import contextlib
import io
from typing import Any

from docutils import nodes
from sphinx.application import Sphinx
from sphinx.environment import BuildEnvironment
from sphinx.util import logging

logger = logging.getLogger(__name__)

RUN_LANGUAGES = {"python"}


def run_blocks(app: Sphinx, doctree: nodes.document) -> None:
    """Execute the Python blocks of the page that was just read."""
    docname = app.env.docname
    namespace: dict[str, Any] = {"__name__": f"docs.{docname}"}
    for block in doctree.findall(nodes.literal_block):
        if block.get("language") not in RUN_LANGUAGES:
            continue
        try:
            code = compile(block.astext(), f"{docname}.md, line {block.line}", "exec")
            with contextlib.redirect_stdout(io.StringIO()):
                exec(code, namespace)
        except Exception as error:
            logger.warning("code block failed: %s: %s", type(error).__name__, error, location=block)


def read_all(
    app: Sphinx, env: BuildEnvironment, added: set[str], changed: set[str], removed: set[str]
) -> list[str]:
    """Read every page again, since the library under an unchanged example may have changed."""
    return sorted(env.found_docs)


def setup(app: Sphinx) -> dict[str, Any]:
    """Register the extension."""
    app.connect("doctree-read", run_blocks)
    app.connect("env-get-outdated", read_all)
    return {"parallel_read_safe": True, "parallel_write_safe": True}
