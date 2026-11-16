# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A grammar and a comparison that several test files share."""

import math
from typing import Any

# A list whose items and closing bracket are named errors, each with a recovery.
RECOVERING = r"""
start: "[" (item ("," (item ^ item))*)? "]" ^ close
item: NUMBER
NUMBER: [0-9]+
%ignore " "+
%recover item: (!("," | "]") .)*
%recover close: ()
"""


def same(a: Any, b: Any) -> bool:
    """Equal values of equal types, keys in the same order, -0.0 apart from 0.0, nan as nan."""
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return list(a) == list(b) and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, float):
        both_nan = math.isnan(a) and math.isnan(b)
        return both_nan or (a == b and math.copysign(1, a) == math.copysign(1, b))
    return a == b
