# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Sphinx configuration of the fastpeg documentation."""

import importlib.metadata
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "_ext"))

project = "fastpeg"
author = "Dominik Schwimmbeck"
copyright = "2026 Dominik Schwimmbeck"
release = importlib.metadata.version("fastpeg")

extensions = ["myst_parser", "run_code_blocks", "fpeg_lexer"]
source_suffix = {".md": "markdown"}
exclude_patterns = ["_build"]

html_theme = "furo"
html_title = "fastpeg"
html_static_path = ["_static"]
html_favicon = "_static/favicon.svg"
# The logo spells the name, so the sidebar does not repeat it as text.
html_theme_options = {
    "light_logo": "logo-light.svg",
    "dark_logo": "logo-dark.svg",
    "sidebar_hide_name": True,
}
