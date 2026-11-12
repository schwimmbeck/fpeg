// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! The native module `fastpeg._fastpeg` behind the Python package.

use pyo3::pymodule;

/// The native part of the fastpeg Python package.
#[pymodule]
mod _fastpeg {
    use pyo3::prelude::*;

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add("__version__", fastpeg::VERSION)
    }
}
