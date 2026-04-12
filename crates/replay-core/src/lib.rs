pub mod analysis;
pub mod command;
pub mod error;
pub mod format;
pub mod header;
pub mod section;

use analysis::{ApmSample, BuildOrderEntry, PlayerApm};
use command::GameCommand;
use error::{ReplayError, Result};
use format::Format;
use header::Header;

/// A fully parsed replay.
#[derive(Debug, Clone)]
pub struct Replay {
    pub header: Header,
    pub commands: Vec<GameCommand>,
    pub build_order: Vec<BuildOrderEntry>,
    pub player_apm: Vec<PlayerApm>,
}

impl Replay {
    /// Calculate APM over time for graphing.
    ///
    /// `window_secs` — sliding window size in real seconds (default 60).
    /// `step_secs` — sample interval in real seconds (default 10).
    pub fn apm_over_time(&self, window_secs: f64, step_secs: f64) -> Vec<ApmSample> {
        let fps = 23.81;
        let window_frames = (window_secs * fps) as u32;
        let step_frames = (step_secs * fps) as u32;
        analysis::calculate_apm_over_time(
            &self.commands,
            self.header.frame_count,
            window_frames,
            step_frames,
        )
    }
}

/// Parse a replay from raw `.rep` file bytes.
///
/// Supports modern format replays (1.18+, zlib compressed).
/// Returns the full replay with header, commands, build order, and APM.
pub fn parse(data: &[u8]) -> Result<Replay> {
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

    let is_121 = fmt == Format::Modern121;

    // Section 0: Replay ID (4 bytes after decompression).
    let (section0, consumed0) = section::decompress_section(data, 0)?;
    validate_magic(&section0)?;
    let mut offset = consumed0;

    // 1.21+ inserts a 4-byte encoded length field after section 0.
    if is_121 {
        offset += 4;
    }

    // Section 1: Header (633 bytes after decompression).
    let (section1, consumed1) = section::decompress_section(data, offset)?;
    let mut hdr = header::parse_header(&section1)?;
    offset += consumed1;

    // 1.21+ inserts a size-marker section (4 bytes) between each real section.
    if is_121 {
        offset += skip_size_marker(data, offset)?;
    }

    // Section 2: Commands.
    let (section2, consumed2) = section::decompress_section(data, offset)?;
    let commands = command::parse_commands(&section2);
    offset += consumed2;

    if is_121 {
        offset += skip_size_marker(data, offset)?;
    }

    // Section 3: Map data (skip).
    let (_section3, consumed3) = section::decompress_section(data, offset)?;
    offset += consumed3;

    if is_121 {
        offset += skip_size_marker(data, offset)?;
    }

    // Section 4: Extended player names (768 bytes).
    if offset < data.len()
        && let Ok((section4, _)) = section::decompress_section(data, offset)
    {
        header::apply_extended_names(&mut hdr, &section4);
    }

    // Derive analytics.
    let build_order = analysis::extract_build_order(&commands);
    let player_apm = analysis::calculate_apm(&commands, hdr.frame_count);

    Ok(Replay {
        header: hdr,
        commands,
        build_order,
        player_apm,
    })
}

/// Skip a 1.21+ size-marker section (a mini section containing 4 bytes).
/// Returns the number of bytes consumed.
fn skip_size_marker(data: &[u8], offset: usize) -> Result<usize> {
    let (_marker, consumed) = section::decompress_section(data, offset)?;
    Ok(consumed)
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
        let result = parse(&[0u8; 30]);
        assert!(matches!(result, Err(ReplayError::LegacyFormat)));
    }
}
