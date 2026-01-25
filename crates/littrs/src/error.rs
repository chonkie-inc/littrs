use thiserror::Error;

use crate::diagnostic::Diagnostic;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Type error: expected {expected}, got {got}")]
    Type { expected: String, got: String },

    #[error("Name error: '{0}' is not defined")]
    NameError(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Division by zero")]
    DivisionByZero,

    /// Rich diagnostic error with source context, labels, notes, and help.
    #[error("{0}")]
    Diagnostic(Diagnostic),
}

impl Error {
    /// Create a new diagnostic error.
    pub fn diagnostic(diagnostic: Diagnostic) -> Self {
        Error::Diagnostic(diagnostic)
    }

    /// Check if this error has rich diagnostic information.
    pub fn has_diagnostic(&self) -> bool {
        matches!(self, Error::Diagnostic(_))
    }

    /// Get the diagnostic if this is a Diagnostic error.
    pub fn as_diagnostic(&self) -> Option<&Diagnostic> {
        match self {
            Error::Diagnostic(d) => Some(d),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
