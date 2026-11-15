// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Diagnostics: what is wrong with a grammar, or worth a look in it, each with a stable code.
//!
//! The page `docs/reference/diagnostics/<code>.md` of each code explains it with an input that
//! triggers it and the fix, and a test holds the pages and the codes together. A code that
//! starts with E is an error, which keeps a grammar from being used, and the hundreds tell its
//! area: 1 reading the text, 2 names, 3 forms, 4 limits, 5 what the roadmap brings later, 9
//! bugs of fpeg itself. A code that starts with W is a warning, which [`crate::check`] gives
//! for a grammar that works: 1 choices, 2 repetition and recursion, 3 the shape
//! of the tree, 4 what nothing uses.

use std::fmt;

use crate::grammar::Span;

/// How serious a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The grammar cannot be used.
    Error,
    /// The grammar works, and likely not as its author meant.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// A problem that makes a grammar unusable, or a warning about one that works.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// The kind of problem, whose page in the documentation explains it.
    pub code: DiagnosticCode,
    /// What exactly is wrong.
    pub message: String,
    /// Where in the grammar source. Empty for grammars built in code.
    pub span: Span,
}

impl Diagnostic {
    /// Whether the diagnostic is an error or a warning, which its code says.
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (severity, span) = (self.severity(), self.span);
        write!(
            f,
            "{severity}[{}]: {} at {}..{}",
            self.code, self.message, span.start, span.end
        )
    }
}

impl std::error::Error for Diagnostic {}

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
    /// E204: a name defined twice, or a second `%extends`.
    Duplicate,
    /// E205: a name that does not start with a letter after its underscores.
    Name,
    /// E206: a use with the wrong number of arguments.
    Arguments,
    /// E207: a field that a value builder names and its alternative cannot hold.
    BuilderField,
    /// E208: a function that a value builder calls and the functions given with the grammar
    /// lack. The Python binding reports it, since only it is given functions.
    BuilderFunction,
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
    /// E309: a value builder that cannot apply where it stands.
    BuilderPlace,
    /// E310: a value builder whose arguments do not fit.
    BuilderArguments,
    /// E401: an expression or a grammar text that nests too deeply.
    TooDeep,
    /// E402: a grammar too large: too many template instances, expressions, names, flags or
    /// fields passed on to one node.
    TooLarge,
    /// E501: a form that a later milestone of the roadmap brings.
    NotYet,
    /// E601: a grammar extended that no module provides.
    UnknownModule,
    /// E901: a bug of fpeg itself.
    Internal,
    /// W101: an alternative after one that cannot fail.
    UnreachableAlternative,
    /// W102: a literal after a literal it starts with.
    ShadowedLiteral,
    /// W103: an alternative that starts with the whole of an earlier one.
    ShadowedAlternative,
    /// W201: a repetition whose expression can match nothing.
    EmptyRepetition,
    /// W202: a rule that calls itself at both ends of an alternative.
    LeftAndRightRecursion,
    /// W301: an alternative of a transparent rule that makes a node for some inputs only.
    UnsteadyShape,
    /// W302: a builder on an alternative of a transparent rule that never makes a node.
    UnusedBuilder,
    /// W401: a flag that is never set, or never tested.
    UnusedFlag,
    /// W402: a parameter that its rule never uses.
    UnusedParameter,
}

impl DiagnosticCode {
    /// Every code, in the order of their numbers.
    pub const ALL: [DiagnosticCode; 36] = [
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
        DiagnosticCode::BuilderField,
        DiagnosticCode::BuilderFunction,
        DiagnosticCode::RuleInTerminal,
        DiagnosticCode::RuleForm,
        DiagnosticCode::Precedence,
        DiagnosticCode::TransparentField,
        DiagnosticCode::FieldItems,
        DiagnosticCode::EmptyLiteral,
        DiagnosticCode::Class,
        DiagnosticCode::Bounds,
        DiagnosticCode::BuilderPlace,
        DiagnosticCode::BuilderArguments,
        DiagnosticCode::TooDeep,
        DiagnosticCode::TooLarge,
        DiagnosticCode::NotYet,
        DiagnosticCode::UnknownModule,
        DiagnosticCode::Internal,
        DiagnosticCode::UnreachableAlternative,
        DiagnosticCode::ShadowedLiteral,
        DiagnosticCode::ShadowedAlternative,
        DiagnosticCode::EmptyRepetition,
        DiagnosticCode::LeftAndRightRecursion,
        DiagnosticCode::UnsteadyShape,
        DiagnosticCode::UnusedBuilder,
        DiagnosticCode::UnusedFlag,
        DiagnosticCode::UnusedParameter,
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
            DiagnosticCode::BuilderField => "E207",
            DiagnosticCode::BuilderFunction => "E208",
            DiagnosticCode::RuleInTerminal => "E301",
            DiagnosticCode::RuleForm => "E302",
            DiagnosticCode::Precedence => "E303",
            DiagnosticCode::TransparentField => "E304",
            DiagnosticCode::FieldItems => "E305",
            DiagnosticCode::EmptyLiteral => "E306",
            DiagnosticCode::Class => "E307",
            DiagnosticCode::Bounds => "E308",
            DiagnosticCode::BuilderPlace => "E309",
            DiagnosticCode::BuilderArguments => "E310",
            DiagnosticCode::TooDeep => "E401",
            DiagnosticCode::TooLarge => "E402",
            DiagnosticCode::NotYet => "E501",
            DiagnosticCode::UnknownModule => "E601",
            DiagnosticCode::Internal => "E901",
            DiagnosticCode::UnreachableAlternative => "W101",
            DiagnosticCode::ShadowedLiteral => "W102",
            DiagnosticCode::ShadowedAlternative => "W103",
            DiagnosticCode::EmptyRepetition => "W201",
            DiagnosticCode::LeftAndRightRecursion => "W202",
            DiagnosticCode::UnsteadyShape => "W301",
            DiagnosticCode::UnusedBuilder => "W302",
            DiagnosticCode::UnusedFlag => "W401",
            DiagnosticCode::UnusedParameter => "W402",
        }
    }

    /// Whether diagnostics of the code are errors or warnings.
    pub fn severity(self) -> Severity {
        match self.code().starts_with('W') {
            true => Severity::Warning,
            false => Severity::Error,
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
