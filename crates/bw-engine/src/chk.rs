use crate::error::{EngineError, Result};

/// A parsed CHK section: 4-byte tag + data slice.
#[derive(Debug)]
pub struct ChkSection<'a> {
    pub tag: [u8; 4],
    pub data: &'a [u8],
}

impl ChkSection<'_> {
    #[must_use]
    pub fn tag_str(&self) -> &str {
        std::str::from_utf8(&self.tag).unwrap_or("????")
    }
}

fn read_tag(data: &[u8], off: usize) -> Option<[u8; 4]> {
    data.get(off..off.checked_add(4)?)?.try_into().ok()
}

/// Read a little-endian u16 at `off`, or `None` if out of bounds.
pub(crate) fn read_u16(data: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(off..off.checked_add(2)?)?.try_into().ok()?,
    ))
}

/// Read a little-endian u32 at `off`, or `None` if out of bounds.
pub(crate) fn read_u32(data: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(off..off.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// Parse all tag-length-value sections from raw CHK bytes.
///
/// Format: repeating `[4-byte ASCII tag][u32 LE length][length bytes data]`.
/// Sections may appear multiple times; per CHK spec the last occurrence wins.
pub fn parse_sections(data: &[u8]) -> Result<Vec<ChkSection<'_>>> {
    let mut sections = Vec::new();
    let mut pos = 0;

    while let (Some(tag), Some(len)) = (read_tag(data, pos), read_u32(data, pos + 4)) {
        pos += 8;
        let end = pos.saturating_add(len as usize).min(data.len());
        // A truncated final section keeps whatever is available (some maps do this).
        sections.push(ChkSection {
            tag,
            data: &data[pos..end],
        });
        pos = end;
    }

    Ok(sections)
}

/// Extracted terrain-relevant data from a CHK file.
#[derive(Debug)]
pub struct ChkTerrain {
    pub width: u16,
    pub height: u16,
    pub tileset_index: u16,
    pub tile_ids: Vec<u16>,
}

/// Extract terrain data (DIM, ERA, MTXM) from parsed CHK sections.
///
/// Uses the last occurrence of each section per CHK spec.
pub fn extract_terrain(sections: &[ChkSection<'_>]) -> Result<ChkTerrain> {
    let mut dim: Option<(u16, u16)> = None;
    let mut era: Option<u16> = None;
    let mut mtxm: Option<&[u8]> = None;

    for section in sections {
        match &section.tag {
            b"DIM " => dim = Some(parse_dim(section.data)?),
            b"ERA " => era = Some(parse_era(section.data)?),
            b"MTXM" => mtxm = Some(section.data),
            _ => {}
        }
    }

    let (width, height) = dim.ok_or_else(|| EngineError::MissingSection {
        tag: "DIM ".to_string(),
    })?;
    let tileset_index = era.ok_or_else(|| EngineError::MissingSection {
        tag: "ERA ".to_string(),
    })?;
    let mtxm_data = mtxm.ok_or_else(|| EngineError::MissingSection {
        tag: "MTXM".to_string(),
    })?;

    let expected_count = width as usize * height as usize;
    let tile_ids = parse_mtxm(mtxm_data, expected_count);

    Ok(ChkTerrain {
        width,
        height,
        tileset_index,
        tile_ids,
    })
}

fn parse_dim(data: &[u8]) -> Result<(u16, u16)> {
    if data.len() < 4 {
        return Err(EngineError::InvalidSection {
            tag: "DIM ".to_string(),
            reason: format!("expected 4 bytes, got {}", data.len()),
        });
    }
    let width = u16::from_le_bytes([data[0], data[1]]);
    let height = u16::from_le_bytes([data[2], data[3]]);
    Ok((width, height))
}

fn parse_era(data: &[u8]) -> Result<u16> {
    if data.len() < 2 {
        return Err(EngineError::InvalidSection {
            tag: "ERA ".to_string(),
            reason: format!("expected 2 bytes, got {}", data.len()),
        });
    }
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

/// Parse MTXM tile IDs, zero-filling if data is shorter than expected.
/// Handles odd trailing byte per OpenBW (bwgame.h:21336-21339).
fn parse_mtxm(data: &[u8], expected_count: usize) -> Vec<u16> {
    let mut tile_ids = vec![0u16; expected_count];
    let mut pos = 0;

    for tile in &mut tile_ids {
        if pos + 2 <= data.len() {
            *tile = u16::from_le_bytes([data[pos], data[pos + 1]]);
            pos += 2;
        } else if pos + 1 == data.len() {
            // Odd trailing byte: only update the low byte.
            *tile = data[pos] as u16;
            break;
        } else {
            break; // Remaining tiles stay zero-filled.
        }
    }

    tile_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_section(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(tag);
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(data);
        buf
    }

    fn build_chk(sections: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (tag, data) in sections {
            buf.extend_from_slice(&build_section(tag, data));
        }
        buf
    }

    #[test]
    fn test_parse_sections() {
        let chk = build_chk(&[(b"DIM ", &[4, 0, 4, 0]), (b"ERA ", &[0, 0])]);
        let sections = parse_sections(&chk).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].tag_str(), "DIM ");
        assert_eq!(sections[1].tag_str(), "ERA ");
    }

    #[test]
    fn test_parse_sections_truncated() {
        // DIM section claims 4 bytes but only 2 available
        let mut chk = Vec::new();
        chk.extend_from_slice(b"DIM ");
        chk.extend_from_slice(&4u32.to_le_bytes());
        chk.extend_from_slice(&[4, 0]); // Only 2 bytes instead of 4
        let sections = parse_sections(&chk).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].data.len(), 2); // Gets what's available
    }

    #[test]
    fn test_extract_terrain() {
        // 2x2 map, tileset 4 (Jungle), 4 tile IDs
        let mut mtxm = Vec::new();
        for id in [0x0010u16, 0x0020, 0x0030, 0x0040] {
            mtxm.extend_from_slice(&id.to_le_bytes());
        }
        let chk = build_chk(&[
            (b"DIM ", &[2, 0, 2, 0]),
            (b"ERA ", &[4, 0]),
            (b"MTXM", &mtxm),
        ]);
        let sections = parse_sections(&chk).unwrap();
        let terrain = extract_terrain(&sections).unwrap();
        assert_eq!(terrain.width, 2);
        assert_eq!(terrain.height, 2);
        assert_eq!(terrain.tileset_index, 4);
        assert_eq!(terrain.tile_ids, vec![0x0010, 0x0020, 0x0030, 0x0040]);
    }

    #[test]
    fn test_last_section_wins() {
        // Two DIM sections — second should win
        let chk = build_chk(&[
            (b"DIM ", &[2, 0, 2, 0]),
            (b"ERA ", &[0, 0]),
            (b"MTXM", &[0, 0, 0, 0, 0, 0, 0, 0]),
            (b"DIM ", &[4, 0, 4, 0]),
        ]);
        let sections = parse_sections(&chk).unwrap();
        let terrain = extract_terrain(&sections).unwrap();
        assert_eq!(terrain.width, 4);
        assert_eq!(terrain.height, 4);
    }

    #[test]
    fn test_missing_dim_section() {
        let chk = build_chk(&[(b"ERA ", &[0, 0]), (b"MTXM", &[0, 0])]);
        let sections = parse_sections(&chk).unwrap();
        let result = extract_terrain(&sections);
        assert!(matches!(result, Err(EngineError::MissingSection { .. })));
    }

    #[test]
    fn test_mtxm_short_zero_fills() {
        // 2x2 map but MTXM only has 2 tile IDs
        let chk = build_chk(&[
            (b"DIM ", &[2, 0, 2, 0]),
            (b"ERA ", &[0, 0]),
            (b"MTXM", &[0x10, 0x00, 0x20, 0x00]),
        ]);
        let sections = parse_sections(&chk).unwrap();
        let terrain = extract_terrain(&sections).unwrap();
        assert_eq!(terrain.tile_ids.len(), 4);
        assert_eq!(terrain.tile_ids[0], 0x0010);
        assert_eq!(terrain.tile_ids[1], 0x0020);
        assert_eq!(terrain.tile_ids[2], 0); // zero-filled
        assert_eq!(terrain.tile_ids[3], 0); // zero-filled
    }

    #[test]
    fn test_mtxm_odd_trailing_byte() {
        // 2x1 map, MTXM has 3 bytes (1 full u16 + 1 odd byte)
        let chk = build_chk(&[
            (b"DIM ", &[2, 0, 1, 0]),
            (b"ERA ", &[0, 0]),
            (b"MTXM", &[0x10, 0x00, 0x42]),
        ]);
        let sections = parse_sections(&chk).unwrap();
        let terrain = extract_terrain(&sections).unwrap();
        assert_eq!(terrain.tile_ids.len(), 2);
        assert_eq!(terrain.tile_ids[0], 0x0010);
        assert_eq!(terrain.tile_ids[1], 0x0042); // low byte only
    }
}
