use thiserror::Error;

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
}

pub type Result<T> = std::result::Result<T, Error>;
