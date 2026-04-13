use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("CHK data too short: expected at least {expected} bytes, got {actual}")]
    ChkTooShort { expected: usize, actual: usize },

    #[error("missing required CHK section: {tag}")]
    MissingSection { tag: String },

    #[error("invalid CHK section \"{tag}\": {reason}")]
    InvalidSection { tag: String, reason: String },

    #[error("invalid tileset index: {0} (expected 0-7)")]
    InvalidTileset(u16),

    #[error("tileset data too short for {file}: expected at least {expected} bytes, got {actual}")]
    TilesetDataTooShort {
        file: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("tile lookup out of bounds: group_index={group_index}, cv5 has {cv5_len} entries")]
    TileLookupOutOfBounds { group_index: usize, cv5_len: usize },

    #[error("megatile index out of bounds: index={index}, vf4 has {vf4_len} entries")]
    MegatileLookupOutOfBounds { index: usize, vf4_len: usize },

    #[error("dat file too short for {file}: expected at least {expected} bytes, got {actual}")]
    DatTooShort {
        file: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("invalid MPQ header: {0}")]
    MpqInvalidHeader(String),

    #[error("file not found in MPQ archive: {name}")]
    MpqFileNotFound { name: String },

    #[error("MPQ decompression failed: {0}")]
    MpqDecompression(String),

    #[error("invalid TBL data: {0}")]
    InvalidTbl(String),

    #[error("invalid GRP data: {0}")]
    InvalidGrp(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;
