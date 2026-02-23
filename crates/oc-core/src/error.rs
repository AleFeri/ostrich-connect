use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OcError {
    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("invalid connection profile: {0}")]
    InvalidProfile(String),

    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("connection failed: {0}")]
    Connection(String),

    #[error("authentication failed")]
    Authentication,

    #[error("io error: {0}")]
    Io(String),

    #[error("session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("operation not supported: {0}")]
    OperationNotSupported(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type OcResult<T> = Result<T, OcError>;
