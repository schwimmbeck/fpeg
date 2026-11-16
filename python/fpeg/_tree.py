# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The list of the children of a view: it reads as a list and refuses changes."""

from collections.abc import Iterable
from typing import Any, NoReturn, SupportsIndex

MESSAGE = (
    "the children of a tree from a parse cannot change, since a transform reads the parse: "
    "build a fpeg.Tree with the children you want"
)


class Children(list[Any]):
    """The visible children of a view of a parse: a list that refuses changes.

    The native module fills it through the C API, which these methods do not see.
    """

    __slots__ = ()

    def __reduce__(self) -> tuple[type[list[Any]], tuple[list[Any]]]:
        """Copy and pickle as a plain list, since copying and unpickling fill a list by append."""
        return (list, (list(self),))

    def append(self, value: Any, /) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def extend(self, values: Iterable[Any], /) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def insert(self, index: SupportsIndex, value: Any, /) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def remove(self, value: Any, /) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def pop(self, index: SupportsIndex = -1, /) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def clear(self) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def sort(self, *args: Any, **kwargs: Any) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def reverse(self) -> NoReturn:
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def __setitem__(self, index: Any, value: Any) -> NoReturn:  # type: ignore[override] # refuses every form
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def __delitem__(self, index: Any) -> NoReturn:  # type: ignore[override] # refuses every form
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def __iadd__(self, values: Iterable[Any]) -> NoReturn:  # type: ignore[override,misc] # refuses
        """Refuse the change."""
        raise TypeError(MESSAGE)

    def __imul__(self, count: SupportsIndex) -> NoReturn:  # type: ignore[override,misc] # refuses
        """Refuse the change."""
        raise TypeError(MESSAGE)
