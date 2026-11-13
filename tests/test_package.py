# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from importlib.metadata import version

import fpeg


def test_native_module_reports_the_installed_package_version() -> None:
    assert fpeg.__version__ == version("fpeg")
