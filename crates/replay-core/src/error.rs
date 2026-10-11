use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplayError {
    /// The input does not start with a Brood War replay identifier, so it is
    /// some other kind of file (or not a file from StarCraft at all).
    #[error("not a StarCraft: Brood War replay ({0})")]
    NotAReplay(&'static str),

    /// The replay identifier is valid, but the data ends before a section
    /// that should be there. `offset` is where the missing data starts.
    #[error(
        "replay file is truncated or damaged: data ends unexpectedly at byte {offset} \
         (file is {len} bytes)"
    )]
    Truncated { offset: usize, len: usize },

    #[error("replay file is damaged: decompression failed ({0})")]
    Decompression(String),

    #[error("replay file is damaged: invalid section {index} ({reason})")]
    InvalidSection { index: usize, reason: String },

    #[error("replay file is damaged: invalid section length {value} at byte {offset}")]
    InvalidLength { offset: usize, value: i32 },

    #[error("unsupported or damaged replay header: {0}")]
    InvalidHeader(String),
}

impl ReplayError {
    /// Stable machine-readable error code, e.g. for mapping to localized
    /// messages in a UI: `NOT_A_REPLAY`, `TRUNCATED`, `CORRUPT` or
    /// `UNSUPPORTED`.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            ReplayError::NotAReplay(_) => "NOT_A_REPLAY",
            ReplayError::Truncated { .. } => "TRUNCATED",
            ReplayError::Decompression(_)
            | ReplayError::InvalidSection { .. }
            | ReplayError::InvalidLength { .. } => "CORRUPT",
            ReplayError::InvalidHeader(_) => "UNSUPPORTED",
        }
    }
}

pub type Result<T> = std::result::Result<T, ReplayError>;
