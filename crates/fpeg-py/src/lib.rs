// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The native module `fpeg._fpeg` behind the Python package.

use pyo3::pymodule;

/// The native part of the fpeg Python package.
#[pymodule]
mod _fpeg {
    use pyo3::prelude::*;

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add("__version__", fpeg::VERSION)
    }
}
