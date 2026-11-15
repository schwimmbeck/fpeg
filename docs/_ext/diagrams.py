# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The directive ``diagram``: a flow chart of ``docs/_diagrams`` in a figure with its caption.

The drawing is put into the page as it is, not as an image, so that ``_static/diagrams.css``
colours it from the theme and it follows a switch between light and dark. The drawing names its
parts by class and holds no colour of its own. The content of the directive is the caption.
"""

from pathlib import Path
from typing import Any

from docutils import nodes
from sphinx.application import Sphinx
from sphinx.util.docutils import SphinxDirective

DIAGRAMS = Path(__file__).resolve().parent.parent / "_diagrams"


class Diagram(SphinxDirective):
    """A drawing of ``docs/_diagrams``, named by the argument, over a caption of one paragraph."""

    required_arguments = 1
    has_content = True

    def run(self) -> list[nodes.Node]:
        """Return the figure."""
        path = DIAGRAMS / self.arguments[0]
        if not path.is_file():
            raise self.error(f"no diagram {path.name} in docs/_diagrams")
        self.env.note_dependency(str(path))
        caption = nodes.Element()
        self.state.nested_parse(self.content, self.content_offset, caption)
        paragraph = caption[0] if len(caption) == 1 else None
        if not isinstance(paragraph, nodes.paragraph):
            raise self.error("the caption of a diagram is one paragraph")
        figure = nodes.figure(classes=["diagram"])
        figure += nodes.raw("", path.read_text(encoding="utf-8"), format="html")
        figure += nodes.caption(paragraph.rawsource, "", *paragraph.children)
        return [figure]


def setup(app: Sphinx) -> dict[str, Any]:
    """Register the directive and its stylesheet."""
    app.add_directive("diagram", Diagram)
    app.add_css_file("diagrams.css")
    return {"parallel_read_safe": True, "parallel_write_safe": True}
