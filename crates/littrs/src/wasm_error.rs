use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("WASM error: {0}")]
    Wasm(#[from] wasmtime::Error),

    #[error("Out of fuel (CPU limit exceeded)")]
    OutOfFuel,

    #[error("Memory limit exceeded")]
    MemoryLimit,

    #[error("Sandbox not initialized")]
    NotInitialized,

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid UTF-8 in result")]
    InvalidUtf8,
}

pub type Result<T> = std::result::Result<T, Error>;
