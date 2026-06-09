use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, OrbitError>;

#[derive(Debug, thiserror::Error)]
pub enum OrbitError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("refused: {message}")]
    Refused {
        code: &'static str,
        message: String,
        path: Option<PathBuf>,
    },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("process failed with status {status}: {command}")]
    Process { status: i32, command: String },
}

impl OrbitError {
    pub fn refused(code: &'static str, message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self::Refused {
            code,
            message: message.into(),
            path,
        }
    }
}
