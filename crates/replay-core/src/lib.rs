pub mod analysis;
pub mod command;
pub mod error;
pub mod format;
pub mod gamedata;
pub mod header;
pub mod metadata;
pub mod phases;
pub mod section;
pub mod similarity;
pub mod skill;
pub mod timeline;

use analysis::{ApmSample, BuildOrderEntry, PlayerApm};
use command::GameCommand;
use error::{ReplayError, Result};
use format::Format;
use header::Header;
use metadata::GameMetadata;
use timeline::TimelineSnapshot;

/// A fully parsed replay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Replay {
    pub header: Header,
    pub commands: Vec<GameCommand>,
    pub build_order: Vec<BuildOrderEntry>,
    pub player_apm: Vec<PlayerApm>,
    pub timeline: Vec<TimelineSnapshot>,
    /// High-level game metadata: matchup, normalized map name, winner.
    pub metadata: GameMetadata,
    /// Raw CHK map data (decompressed section 3). Can be fed to `bw_engine::Map::from_chk`.
    pub map_data: Vec<u8>,
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
/// Supports all replay formats:
/// - Legacy (pre-1.18): PKWare DCL compressed
/// - Modern (1.18–1.20): zlib compressed
/// - Modern 1.21+ (Remastered): zlib compressed with extra post-magic gap
pub fn parse(data: &[u8]) -> Result<Replay> {
    if data.len() < 30 {
        return Err(ReplayError::TooShort {
            expected: 30,
            actual: data.len(),
        });
    }

    let fmt = format::detect(data);

    // Section 0: Replay ID (4 bytes after decompression).
    let (section0, consumed0) = section::decompress_section(data, 0, fmt)?;
    validate_magic(&section0)?;
    let mut offset = consumed0;

    // 1.21+ inserts a 4-byte encoded length field after section 0.
    if fmt == Format::Modern121 {
        offset += 4;
    }

    // Section 1: Header (633 bytes).
    let (section1, consumed1) = section::decompress_section(data, offset, fmt)?;
    let mut hdr = header::parse_header(&section1)?;
    offset += consumed1;

    // All formats: skip the size-marker section between header and commands.
    offset += skip_section(data, offset, fmt)?;

    // Section 2: Commands.
    let (section2, consumed2) = section::decompress_section(data, offset, fmt)?;
    let commands = command::parse_commands(&section2);
    offset += consumed2;

    // Skip size-marker between commands and map data.
    offset += skip_section(data, offset, fmt)?;

    // Section 3: Map data (CHK format).
    let (map_data, consumed3) = section::decompress_section(data, offset, fmt)?;
    offset += consumed3;

    // Skip size-marker between map data and player names.
    if offset < data.len() {
        offset += skip_section(data, offset, fmt)?;
    }

    // Section 4: Extended player names (768 bytes).
    if offset < data.len()
        && let Ok((section4, _)) = section::decompress_section(data, offset, fmt)
    {
        header::apply_extended_names(&mut hdr, &section4);
    }

    // Derive analytics.
    let build_order = analysis::extract_build_order(&commands);
    let player_apm = analysis::calculate_apm(&commands, hdr.frame_count);

    // Build timeline from player IDs in the header.
    let player_ids: Vec<u8> = hdr.players.iter().map(|p| p.player_id).collect();
    let tl = timeline::build_timeline(&build_order, &player_ids);

    // Derive game metadata.
    let meta = metadata::extract_metadata(&hdr, &commands);

    Ok(Replay {
        header: hdr,
        commands,
        build_order,
        player_apm,
        timeline: tl,
        metadata: meta,
        map_data,
    })
}

/// Skip a section (typically a 4-byte size-marker between real sections).
fn skip_section(data: &[u8], offset: usize, fmt: Format) -> Result<usize> {
    let (_data, consumed) = section::decompress_section(data, offset, fmt)?;
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
}
