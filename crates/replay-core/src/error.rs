use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("invalid replay: data too short (expected at least {expected} bytes, got {actual})")]
    TooShort { expected: usize, actual: usize },

    #[error("invalid replay magic: expected \"seRS\" or \"reRS\", got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("decompression failed: {0}")]
    Decompression(String),

    #[error("unsupported replay format: legacy (pre-1.18) replays are not yet supported")]
    LegacyFormat,

    #[error("invalid section {index}: {reason}")]
    InvalidSection { index: usize, reason: String },

    #[error("invalid header: {0}")]
    InvalidHeader(String),
}

pub type Result<T> = std::result::Result<T, ReplayError>;
