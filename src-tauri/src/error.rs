//! Error type returned by every command. Serializes to { code, message, ...extra } (design section 10).

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", rename_all = "PascalCase")]
pub enum AppError {
    #[error("not found: {message}")]
    NotFound { message: String },
    #[error("failed to spawn: {message}")]
    SpawnFailed { message: String },
    #[error("io error: {message}")]
    Io { message: String },
    #[error("path is outside the notes root")]
    OutsideRoot { message: String },
    #[error("file changed on disk")]
    #[serde(rename_all = "camelCase")]
    Conflict {
        message: String,
        current_mtime_ms: u64,
    },
    #[error("invalid name: {message}")]
    InvalidName { message: String },
    #[error("git timed out")]
    GitTimeout { message: String },
    #[error("git is not installed or not on PATH")]
    GitNotInstalled { message: String },
    /// The cloud API could not be reached (no network, DNS, timeout).
    #[error("offline: {message}")]
    Offline { message: String },
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io {
            message: e.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
