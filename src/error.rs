pub type Result<T> = std::result::Result<T, OrbitError>;

#[derive(Debug, thiserror::Error)]
pub enum OrbitError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("process failed with status {status}: {command}")]
    Process { status: i32, command: String },
}

impl OrbitError {
    pub fn refused(msg: impl Into<String>) -> Self {
        Self::Refused(msg.into())
    }
}
