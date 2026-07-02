use thiserror::Error;

#[derive(Debug, Error)]
pub enum KdsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("print connect error: {0}")]
    PrintConnect(std::io::Error),
    #[error("print write error: {0}")]
    PrintWrite(std::io::Error),
    #[error("print file error: {0}")]
    PrintFile(std::io::Error),
    #[error("station not found: {0}")]
    StationNotFound(String),
    #[error("no active profile configured")]
    NoActiveProfile,
}
