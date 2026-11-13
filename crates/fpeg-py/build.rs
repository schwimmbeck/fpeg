// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Set the `Py_3_12` and other version flags of the Python the module is built for, which the
//! token type's layout depends on.

fn main() {
    pyo3_build_config::use_pyo3_cfgs();
}
