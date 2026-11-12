# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""Sphinx configuration of the fastpeg documentation."""

import importlib.metadata
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "_ext"))

project = "fastpeg"
author = "Dominik Schwimmbeck"
copyright = "2026 Dominik Schwimmbeck"
release = importlib.metadata.version("fastpeg")

extensions = ["myst_parser", "run_code_blocks"]
source_suffix = {".md": "markdown"}
exclude_patterns = ["_build"]

html_theme = "furo"
html_title = "fastpeg"
