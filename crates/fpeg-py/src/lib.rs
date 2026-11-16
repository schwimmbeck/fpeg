// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The native module `fpeg._fpeg` behind the Python package.

mod builders;
mod errors;
mod grammar;
mod token;
mod transform;
mod tree;

use pyo3::prelude::*;
use pyo3::pymodule;
use pyo3::types::PyString;

/// A Python string of `text`. Where Python cannot allocate it this raises `MemoryError`, where
/// `PyString::new`, which makes the same call, panics.
fn string<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyString>> {
    PyString::from_bytes(py, text.as_bytes())
}

/// `version` as Python spells versions, which is how the package's metadata has it: Cargo's
/// `0.1.0-rc.1` is `0.1.0rc1` there.
fn python_version(version: &str) -> String {
    let Some((release, pre)) = version.split_once('-') else {
        return version.to_owned();
    };
    let (label, number) = pre.split_once('.').unwrap_or((pre, ""));
    let label = match label {
        "alpha" => "a",
        "beta" => "b",
        other => other,
    };
    format!("{release}{label}{number}")
}

/// The native part of the fpeg Python package.
#[pymodule(gil_used = false)]
mod _fpeg {
    use pyo3::prelude::*;

    #[pymodule_export]
    use crate::errors::Diagnostic;
    #[pymodule_export]
    use crate::grammar::{Grammar, check};
    #[pymodule_export]
    use crate::transform::{DiscardType, is_view, scan, token_value, walk_plan};
    #[pymodule_export]
    use crate::tree::{Meta, Tree, rebuild_tree};

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = module.py();
        module.add("__version__", crate::python_version(fpeg::VERSION))?;
        module.add("Token", crate::token::create_type(py)?)?;
        module.add("ParseError", py.get_type::<crate::errors::ParseError>())?;
        module.add("LimitError", py.get_type::<crate::errors::LimitError>())?;
        module.add("GrammarError", py.get_type::<crate::errors::GrammarError>())?;
        // Private, so that pickles of errors find it but __all__ leaves it out.
        module.setattr("_Text", py.get_type::<crate::errors::Text>())?;
        module.add("Discard", crate::transform::discard(py)?)?;
        module.add("NO_VALUE", crate::transform::no_value(py)?)?;
        Ok(())
    }
}
