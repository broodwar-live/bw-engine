/// Replay file format generation, detected from raw bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Pre-1.18: PKWare DCL compressed sections.
    Legacy,
    /// 1.18–1.20: zlib compressed sections.
    Modern,
    /// 1.21+ (Remastered): zlib compressed, extra sections (SKIN, CCLR, etc).
    Modern121,
}

/// Magic bytes at the start of a replay (after decompressing section 0).
pub const MAGIC_MODERN: &[u8; 4] = b"seRS";
pub const MAGIC_LEGACY: &[u8; 4] = b"reRS";

/// Detect the replay format from the first 30 raw bytes of the file.
///
/// - Byte 12 == 0x73 ('s') → Modern 1.21+
/// - Byte 28 == 0x78 (zlib magic) → Modern (1.18–1.20)
/// - Otherwise → Legacy (pre-1.18)
pub fn detect(data: &[u8]) -> Format {
    if data.len() >= 13 && data[12] == 0x73 {
        Format::Modern121
    } else if data.len() >= 29 && data[28] == 0x78 {
        Format::Modern
    } else {
        Format::Legacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_legacy_short_data() {
        assert_eq!(detect(&[0u8; 10]), Format::Legacy);
    }

    #[test]
    fn test_detect_modern_zlib_at_28() {
        let mut data = [0u8; 30];
        data[28] = 0x78;
        assert_eq!(detect(&data), Format::Modern);
    }

    #[test]
    fn test_detect_modern_121() {
        let mut data = [0u8; 30];
        data[12] = 0x73;
        assert_eq!(detect(&data), Format::Modern121);
    }

    #[test]
    fn test_detect_121_takes_precedence_over_zlib() {
        let mut data = [0u8; 30];
        data[12] = 0x73;
        data[28] = 0x78;
        assert_eq!(detect(&data), Format::Modern121);
    }
}
