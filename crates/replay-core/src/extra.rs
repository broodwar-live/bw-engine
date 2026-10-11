//! Remastered (1.21+) extra sections that follow the player-names section.
//!
//! Each extra section is framed as:
//!   [4-byte ASCII tag] [u32 raw_size] [raw_size bytes]
//!
//! For the sections written by SC:R itself (`SKIN`, `LMTS`, `BFIX`, `CCLR`,
//! `GCFG`) the body is a regular compressed section envelope (see
//! [`crate::section::decompress_section`]). Third-party tools (e.g.
//! ShieldBattery's `Sbat`) may append sections with arbitrary bodies, so
//! every section is skipped by its declared `raw_size`, never by its content.

use crate::format::Format;
use crate::section;

/// Number of player slots in the custom-colour table.
const CCLR_SLOTS: usize = 12;
/// Bytes per slot in `CCLR`: four little-endian f32 (r, g, b, a) in 0..=1.
const CCLR_ENTRY_SIZE: usize = 16;

/// Data extracted from the Remastered extra sections.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExtraSections {
    /// Tags of all extra sections present, in file order (e.g. `"SKIN"`).
    pub tags: Vec<String>,
    /// Custom player colours from `CCLR` as `0xRRGGBB`, indexed by slot.
    /// `None` for slots without a colour (or if `CCLR` is absent).
    pub player_colors: Vec<Option<u32>>,
    /// True if the trailing data ended in the middle of a section header or
    /// body. Everything before that point was still processed.
    pub truncated: bool,
}

/// Walk the extra sections starting at `offset` (just past the player-names
/// section). Never fails: malformed or unknown sections are skipped.
#[must_use]
pub fn parse_extra_sections(data: &[u8], mut offset: usize, fmt: Format) -> ExtraSections {
    let mut out = ExtraSections::default();

    while offset < data.len() {
        let Some(header) = data.get(offset..offset + 8) else {
            out.truncated = true;
            break;
        };
        let tag = &header[..4];
        let raw_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let body_start = offset + 8;
        let Some(body) = body_start
            .checked_add(raw_size)
            .and_then(|end| data.get(body_start..end))
        else {
            out.truncated = true;
            break;
        };

        out.tags.push(String::from_utf8_lossy(tag).into_owned());
        if tag == b"CCLR"
            && let Ok((decompressed, _)) = section::decompress_section(body, 0, fmt)
        {
            out.player_colors = parse_cclr(&decompressed);
        }

        offset = body_start + raw_size;
    }

    out
}

/// Decode the `CCLR` table into `0xRRGGBB` values. An all-zero entry means the
/// slot has no custom colour.
fn parse_cclr(data: &[u8]) -> Vec<Option<u32>> {
    let (entries, _) = data.as_chunks::<CCLR_ENTRY_SIZE>();
    entries
        .iter()
        .take(CCLR_SLOTS)
        .map(|entry| {
            if entry.iter().all(|&b| b == 0) {
                return None;
            }
            let channel = |i: usize| -> Option<u32> {
                let bytes: [u8; 4] = entry.get(i * 4..i * 4 + 4)?.try_into().ok()?;
                let v = f32::from_le_bytes(bytes);
                if !v.is_finite() {
                    return None;
                }
                Some((v.clamp(0.0, 1.0) * 255.0).round() as u32)
            };
            Some((channel(0)? << 16) | (channel(1)? << 8) | channel(2)?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_section(payload: &[u8]) -> Vec<u8> {
        // Uncompressed single-chunk envelope: checksum, chunk count, len, data.
        let mut s = 0u32.to_le_bytes().to_vec();
        s.extend_from_slice(&1u32.to_le_bytes());
        s.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        s.extend_from_slice(payload);
        s
    }

    fn framed(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut s = tag.to_vec();
        s.extend_from_slice(&(body.len() as u32).to_le_bytes());
        s.extend_from_slice(body);
        s
    }

    fn rgba(r: f32, g: f32, b: f32) -> Vec<u8> {
        [r, g, b, 1.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    #[test]
    fn test_parse_extra_sections_reads_cclr_and_skips_unknown() {
        let mut cclr = rgba(1.0, 0.0, 0.0);
        cclr.extend(vec![0u8; CCLR_ENTRY_SIZE]); // slot 1: no colour
        cclr.extend(rgba(0.0, 0.5, 1.0));
        cclr.resize(CCLR_SLOTS * CCLR_ENTRY_SIZE, 0);

        let mut data = framed(b"SKIN", &raw_section(&[0u8; 32]));
        data.extend(framed(b"Sbat", b"arbitrary third-party bytes"));
        data.extend(framed(b"CCLR", &raw_section(&cclr)));
        data.extend(framed(b"GCFG", &raw_section(&[0u8; 25])));

        let extra = parse_extra_sections(&data, 0, Format::Modern121);
        assert_eq!(extra.tags, vec!["SKIN", "Sbat", "CCLR", "GCFG"]);
        assert!(!extra.truncated);
        assert_eq!(extra.player_colors.len(), CCLR_SLOTS);
        assert_eq!(extra.player_colors[0], Some(0xFF0000));
        assert_eq!(extra.player_colors[1], None);
        assert_eq!(extra.player_colors[2], Some(0x0080FF));
    }

    #[test]
    fn test_parse_extra_sections_truncated_body_stops_cleanly() {
        let mut data = framed(b"LMTS", &raw_section(&[1u8; 28]));
        data.extend_from_slice(b"BFIX");
        data.extend_from_slice(&1000u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);

        let extra = parse_extra_sections(&data, 0, Format::Modern121);
        assert_eq!(extra.tags, vec!["LMTS"]);
        assert!(extra.truncated);
        assert!(extra.player_colors.is_empty());
    }

    #[test]
    fn test_parse_extra_sections_empty_input_returns_default() {
        assert_eq!(
            parse_extra_sections(&[], 0, Format::Modern121),
            ExtraSections::default()
        );
    }
}
