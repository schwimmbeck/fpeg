# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from importlib.metadata import version

import fastpeg


def test_native_module_reports_the_installed_package_version() -> None:
    assert fastpeg.__version__ == version("fastpeg")
