pub mod error;
pub mod format;
pub mod header;
pub mod section;

use error::{ReplayError, Result};
use format::Format;
use header::Header;

/// Parse a replay from raw `.rep` file bytes.
///
/// Currently supports modern format replays (1.18+, zlib compressed).
/// Returns the parsed header with player data.
pub fn parse(data: &[u8]) -> Result<Header> {
    if data.len() < 30 {
        return Err(ReplayError::TooShort {
            expected: 30,
            actual: data.len(),
        });
    }

    let fmt = format::detect(data);
    if fmt == Format::Legacy {
        return Err(ReplayError::LegacyFormat);
    }

    // Section 0: Replay ID (4 bytes after decompression).
    let (section0, consumed0) = section::decompress_section(data, 0)?;
    validate_magic(&section0)?;

    // Section 1: Header (633 bytes after decompression).
    let (section1, consumed1) = section::decompress_section(data, consumed0)?;
    let mut hdr = header::parse_header(&section1)?;

    // Section 2: Commands (skip for now — we only need header + player data).
    let (_section2, consumed2) = section::decompress_section(data, consumed0 + consumed1)?;

    // Section 3: Map data (skip for now).
    let (_section3, consumed3) =
        section::decompress_section(data, consumed0 + consumed1 + consumed2)?;

    // Section 4: Extended player names (768 bytes).
    let offset4 = consumed0 + consumed1 + consumed2 + consumed3;
    if offset4 < data.len()
        && let Ok((section4, _)) = section::decompress_section(data, offset4)
    {
        header::apply_extended_names(&mut hdr, &section4);
    }

    Ok(hdr)
}

fn validate_magic(section0: &[u8]) -> Result<()> {
    if section0.len() < 4 {
        return Err(ReplayError::TooShort {
            expected: 4,
            actual: section0.len(),
        });
    }

    let magic: [u8; 4] = section0[..4].try_into().unwrap();
    if &magic != format::MAGIC_MODERN && &magic != format::MAGIC_LEGACY {
        return Err(ReplayError::InvalidMagic(magic));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_too_short() {
        let result = parse(&[0u8; 10]);
        assert!(matches!(result, Err(ReplayError::TooShort { .. })));
    }

    #[test]
    fn test_parse_legacy_rejected() {
        // 30 bytes of zeros → detected as Legacy
        let result = parse(&[0u8; 30]);
        assert!(matches!(result, Err(ReplayError::LegacyFormat)));
    }
}
