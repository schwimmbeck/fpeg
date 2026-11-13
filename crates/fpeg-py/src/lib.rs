// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The native module `fpeg._fpeg` behind the Python package.

mod errors;
mod grammar;
mod token;

use pyo3::pymodule;

/// The native part of the fpeg Python package.
#[pymodule(gil_used = false)]
mod _fpeg {
    use pyo3::prelude::*;

    #[pymodule_export]
    use crate::errors::Diagnostic;
    #[pymodule_export]
    use crate::grammar::Grammar;

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = module.py();
        module.add("__version__", fpeg::VERSION)?;
        module.add("Token", crate::token::create_type(py)?)?;
        module.add("GrammarError", py.get_type::<crate::errors::GrammarError>())?;
        Ok(())
    }
}
