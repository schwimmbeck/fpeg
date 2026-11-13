// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Diagnostic codes: one stable code for every kind of problem a grammar can have.
//!
//! The page `docs/reference/diagnostics/<code>.md` of each code explains it with an input that
//! triggers it and the fix, and a test holds the pages and the codes together. The hundreds of
//! a code tell its area: 1 reading the text, 2 names, 3 forms, 4 limits, 5 what the roadmap
//! brings later, 9 bugs of fpeg itself.

use std::fmt;

/// The kinds of problems a grammar can have. Each has a stable code, and its message says what
/// exactly is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// E101: the text does not follow the syntax of grammars.
    Syntax,
    /// E102: an escape the language does not define.
    Escape,
    /// E103: a Unicode property the language does not support.
    Property,
    /// E104: a number too large for a count.
    Number,
    /// E201: a name no rule, level or parameter has.
    UnknownRule,
    /// E202: a flag no `%flag` declares.
    UnknownFlag,
    /// E203: an attribute the language does not define.
    UnknownAttribute,
    /// E204: a name defined twice.
    Duplicate,
    /// E205: a name that does not start with a letter after its underscores.
    Name,
    /// E206: a use with the wrong number of arguments.
    Arguments,
    /// E301: a terminal or the skip expression that uses a syntactic rule.
    RuleInTerminal,
    /// E302: a form that does not fit the kind of its rule.
    RuleForm,
    /// E303: a precedence block of the wrong form.
    Precedence,
    /// E304: a field in a transparent rule outside an aliased alternative.
    TransparentField,
    /// E305: a field that can capture several items in one match.
    FieldItems,
    /// E306: an empty literal.
    EmptyLiteral,
    /// E307: a character class that lists nothing or runs backwards, or `%word` without one.
    Class,
    /// E308: a count out of its range: a repetition's maximum below its minimum, a tab width.
    Bounds,
    /// E401: an expression or a grammar text that nests too deeply.
    TooDeep,
    /// E402: a grammar too large: too many template instances, expressions, names or flags.
    TooLarge,
    /// E501: a form that a later milestone of the roadmap brings.
    NotYet,
    /// E901: a bug of fpeg itself.
    Internal,
}

impl DiagnosticCode {
    /// Every code, in the order of their numbers.
    pub const ALL: [DiagnosticCode; 22] = [
        DiagnosticCode::Syntax,
        DiagnosticCode::Escape,
        DiagnosticCode::Property,
        DiagnosticCode::Number,
        DiagnosticCode::UnknownRule,
        DiagnosticCode::UnknownFlag,
        DiagnosticCode::UnknownAttribute,
        DiagnosticCode::Duplicate,
        DiagnosticCode::Name,
        DiagnosticCode::Arguments,
        DiagnosticCode::RuleInTerminal,
        DiagnosticCode::RuleForm,
        DiagnosticCode::Precedence,
        DiagnosticCode::TransparentField,
        DiagnosticCode::FieldItems,
        DiagnosticCode::EmptyLiteral,
        DiagnosticCode::Class,
        DiagnosticCode::Bounds,
        DiagnosticCode::TooDeep,
        DiagnosticCode::TooLarge,
        DiagnosticCode::NotYet,
        DiagnosticCode::Internal,
    ];

    /// The code, such as `E201`.
    pub fn code(self) -> &'static str {
        match self {
            DiagnosticCode::Syntax => "E101",
            DiagnosticCode::Escape => "E102",
            DiagnosticCode::Property => "E103",
            DiagnosticCode::Number => "E104",
            DiagnosticCode::UnknownRule => "E201",
            DiagnosticCode::UnknownFlag => "E202",
            DiagnosticCode::UnknownAttribute => "E203",
            DiagnosticCode::Duplicate => "E204",
            DiagnosticCode::Name => "E205",
            DiagnosticCode::Arguments => "E206",
            DiagnosticCode::RuleInTerminal => "E301",
            DiagnosticCode::RuleForm => "E302",
            DiagnosticCode::Precedence => "E303",
            DiagnosticCode::TransparentField => "E304",
            DiagnosticCode::FieldItems => "E305",
            DiagnosticCode::EmptyLiteral => "E306",
            DiagnosticCode::Class => "E307",
            DiagnosticCode::Bounds => "E308",
            DiagnosticCode::TooDeep => "E401",
            DiagnosticCode::TooLarge => "E402",
            DiagnosticCode::NotYet => "E501",
            DiagnosticCode::Internal => "E901",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
