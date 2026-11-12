# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""Shared parts of the pyperf benchmark scripts."""

import gc
import time
from collections.abc import Callable

import pyperf
from corpus import Input

# pyperf's default of 20 worker processes takes minutes for the second-long Lark rows.
# --fast halves the processes and --rigorous doubles them.
WORKER_PROCESSES = 6
VALUES_PER_PROCESS = 3

# The key under which a benchmark names its input, so the summary can report throughput.
INPUT_METADATA_KEY = "fastpeg_input"


def runner() -> pyperf.Runner:
    """Create the one pyperf runner of a benchmark script."""
    return pyperf.Runner(processes=WORKER_PROCESSES, values=VALUES_PER_PROCESS)


def add(
    runner: pyperf.Runner,
    name: str,
    setup: Callable[[], Callable[[], object]],
    input: Input | None = None,
) -> None:
    """Register a benchmark.

    ``setup`` runs once in the worker process that measures this benchmark, outside the timing,
    and returns the function to time. Garbage is collected before each timed value.
    """
    timed: list[Callable[[], object]] = []

    def time_func(loops: int) -> float:
        if not timed:
            timed.append(setup())
        func = timed[0]
        gc.collect()
        start = time.perf_counter()
        for _ in range(loops):
            func()
        return time.perf_counter() - start

    metadata = {INPUT_METADATA_KEY: input.name} if input else None
    runner.bench_time_func(name, time_func, metadata=metadata)
