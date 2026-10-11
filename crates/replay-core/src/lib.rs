pub mod analysis;
pub mod classify;
pub mod command;
pub mod error;
pub mod extra;
pub mod format;
pub mod gamedata;
pub mod header;
pub mod identity;
pub mod metadata;
pub mod phases;
pub mod production;
pub mod section;
pub mod similarity;
pub mod skill;
pub mod stats;
pub mod text;
pub mod timeline;

use analysis::{ApmSample, BuildOrderEntry, PlayerApm};
use command::{CommandParseWarnings, GameCommand};
use error::{ReplayError, Result};
use format::Format;
use header::Header;
use metadata::GameMetadata;
use production::ProductionEntry;
use timeline::TimelineSnapshot;

/// A fully parsed replay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Replay {
    pub header: Header,
    pub commands: Vec<GameCommand>,
    /// Problems encountered while decoding the command stream (unknown or
    /// truncated commands). Empty/zero for a cleanly parsed replay.
    pub command_warnings: CommandParseWarnings,
    pub build_order: Vec<BuildOrderEntry>,
    /// The build order annotated by the production model: supply count at
    /// each order, larvae per morph, and orders that could not have been
    /// carried out (queue full, no larva, duplicates). One entry per
    /// `build_order` entry.
    pub production: Vec<ProductionEntry>,
    pub player_apm: Vec<PlayerApm>,
    pub timeline: Vec<TimelineSnapshot>,
    /// High-level game metadata: matchup, normalized map name, winner.
    pub metadata: GameMetadata,
    /// Remastered extra sections (`SKIN`, `LMTS`, `BFIX`, `CCLR`, `GCFG`, ...).
    /// Empty for pre-Remastered replays.
    pub extra_sections: extra::ExtraSections,
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
    check_replay_id(data)?;

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
    let (commands, command_warnings) = command::parse_commands_with_warnings(&section2);
    offset += consumed2;

    // Skip size-marker between commands and map data.
    offset += skip_section(data, offset, fmt)?;

    // Section 3: Map data (CHK format).
    let (map_data, consumed3) = section::decompress_section(data, offset, fmt)?;
    offset += consumed3;

    // Section 4: Extended player names (12 x 96 bytes), present in 1.18+.
    // Unlike commands and map data it has no size-marker section before it.
    let mut extra_sections = extra::ExtraSections::default();
    if fmt != Format::Legacy
        && offset < data.len()
        && let Ok((section4, consumed4)) = section::decompress_section(data, offset, fmt)
    {
        header::apply_extended_names(&mut hdr, &section4);
        offset += consumed4;

        // Remastered extra sections (tagged, length-prefixed). Optional.
        extra_sections = extra::parse_extra_sections(data, offset, fmt);
        header::apply_custom_colors(&mut hdr, &extra_sections.player_colors);
    }

    // Derive analytics.
    let build_order = analysis::extract_build_order(&commands);
    let player_apm = analysis::calculate_apm(&commands, hdr.frame_count);

    // Production model (supply, larvae, queues) and the timeline built on it.
    let players = modelled_players(&hdr);
    let model = production::model_production(&commands, &players);
    let tl = timeline::build_timeline(&model, &players);

    // Derive game metadata.
    let meta = metadata::extract_metadata(&hdr, &commands);

    Ok(Replay {
        header: hdr,
        commands,
        command_warnings,
        build_order,
        production: model.entries,
        player_apm,
        timeline: tl,
        metadata: meta,
        extra_sections,
        map_data,
    })
}

/// Players to model: human/computer players that issue commands, with their
/// race. Computer players share player id 255, so they are skipped.
fn modelled_players(hdr: &Header) -> Vec<(u8, header::Race)> {
    let mut out: Vec<(u8, header::Race)> = Vec::new();
    for p in &hdr.players {
        if p.player_id != 255 && !out.iter().any(|(id, _)| *id == p.player_id) {
            out.push((p.player_id, p.race));
        }
    }
    out
}

/// Skip a section (typically a 4-byte size-marker between real sections).
fn skip_section(data: &[u8], offset: usize, fmt: Format) -> Result<usize> {
    let (_data, consumed) = section::decompress_section(data, offset, fmt)?;
    Ok(consumed)
}

/// Offset of the raw replay identifier: section 0 is a single 4-byte chunk,
/// which is too small to be compressed, so the magic is stored verbatim after
/// the checksum, chunk count and chunk length fields.
const RAW_MAGIC_OFFSET: usize = 12;
/// Smallest input that can hold section 0 plus the start of the header
/// section's envelope.
const MIN_REPLAY_LEN: usize = 30;

/// Cheap up-front check that `data` is a replay at all, so arbitrary files are
/// rejected with [`ReplayError::NotAReplay`] instead of a confusing section
/// decoding error.
fn check_replay_id(data: &[u8]) -> Result<()> {
    let Some(magic) = data.get(RAW_MAGIC_OFFSET..RAW_MAGIC_OFFSET + 4) else {
        return Err(ReplayError::NotAReplay("file is too small"));
    };
    if magic != format::MAGIC_MODERN && magic != format::MAGIC_LEGACY {
        return Err(ReplayError::NotAReplay("missing replay identifier"));
    }
    if data.len() < MIN_REPLAY_LEN {
        return Err(ReplayError::Truncated {
            offset: data.len(),
            len: data.len(),
        });
    }
    Ok(())
}

fn validate_magic(section0: &[u8]) -> Result<()> {
    match section0.get(..4) {
        Some(m) if m == format::MAGIC_MODERN || m == format::MAGIC_LEGACY => Ok(()),
        _ => Err(ReplayError::NotAReplay("missing replay identifier")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).unwrap()
    }

    #[test]
    fn test_parse_tiny_input_returns_not_a_replay() {
        let result = parse(&[0u8; 10]);
        assert!(matches!(result, Err(ReplayError::NotAReplay(_))));
    }

    #[test]
    fn test_parse_text_file_returns_not_a_replay_with_sane_message() {
        // Used to fail with "expected at least 1631854720 bytes": the text was
        // read as a section envelope.
        let text = b"This is a plain text file, definitely not a StarCraft replay.\n";
        let err = parse(text).unwrap_err();
        assert!(matches!(err, ReplayError::NotAReplay(_)));
        assert_eq!(err.code(), "NOT_A_REPLAY");
        assert_eq!(
            err.to_string(),
            "not a StarCraft: Brood War replay (missing replay identifier)"
        );
    }

    #[test]
    fn test_parse_truncated_fixture_returns_truncated() {
        let data = fixture("larva_vs_mini.rep");
        for cut in [40, 2_000, data.len() / 2] {
            let err = parse(&data[..cut]).unwrap_err();
            assert!(
                matches!(err, ReplayError::Truncated { len, .. } if len == cut)
                    || err.code() == "CORRUPT",
                "cut at {cut}: {err}"
            );
            let msg = err.to_string();
            assert!(msg.len() < 120, "message too long: {msg}");
        }
        let err = parse(&data[..40]).unwrap_err();
        assert_eq!(err.code(), "TRUNCATED");
    }

    #[test]
    fn test_parse_magic_only_returns_truncated() {
        let mut data = vec![0u8; 16];
        data[12..16].copy_from_slice(format::MAGIC_MODERN);
        let err = parse(&data).unwrap_err();
        assert!(matches!(err, ReplayError::Truncated { .. }), "{err}");
    }
}
