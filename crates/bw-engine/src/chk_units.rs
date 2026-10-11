use crate::chk::{ChkSection, read_u16, read_u32};
use crate::error::Result;

const UNIT_ENTRY_SIZE: usize = 36;

/// A unit placed on the map (from CHK UNIT section).
#[derive(Debug, Clone)]
pub struct ChkUnit {
    pub instance_id: u32,
    pub x: u16,
    pub y: u16,
    pub unit_type: u16,
    pub owner: u8,
    pub hp_percent: u8,
    pub shield_percent: u8,
    pub energy_percent: u8,
    pub resources: u32,
}

/// Parse initial unit placements from CHK sections.
/// Uses the last UNIT section per CHK spec.
pub fn parse_chk_units(sections: &[ChkSection<'_>]) -> Result<Vec<ChkUnit>> {
    let mut unit_data: Option<&[u8]> = None;
    for section in sections {
        if section.tag == *b"UNIT" {
            unit_data = Some(section.data);
        }
    }

    let Some(data) = unit_data else {
        return Ok(Vec::new()); // No UNIT section — no preplaced units.
    };

    let (entries, _) = data.as_chunks::<UNIT_ENTRY_SIZE>();
    let units = entries
        .iter()
        .filter_map(|e| parse_unit_entry(e))
        // Skip invalid entries (unit_type 0xFFFF) and out-of-range types.
        .filter(|u| u.unit_type != 0xFFFF && u.unit_type < 228)
        .collect();

    Ok(units)
}

/// Decode one 36-byte CHK `UNIT` entry.
///
/// Layout: u32 instance id, u16 x, u16 y, u16 unit type, u16 link,
/// u16 valid flags, u16 valid properties, u8 owner, u8 hp%, u8 shield%,
/// u8 energy%, u32 resources, u16 hangar count, u16 state flags,
/// u32 unused, u32 related unit id.
fn parse_unit_entry(e: &[u8]) -> Option<ChkUnit> {
    Some(ChkUnit {
        instance_id: read_u32(e, 0)?,
        x: read_u16(e, 4)?,
        y: read_u16(e, 6)?,
        unit_type: read_u16(e, 8)?,
        owner: *e.get(16)?,
        hp_percent: *e.get(17)?,
        shield_percent: *e.get(18)?,
        energy_percent: *e.get(19)?,
        resources: read_u32(e, 20)?,
    })
}

/// Start Location unit type ID.
const START_LOCATION: u16 = 214;

/// Extract start locations from CHK sections.
/// Returns `(owner, x, y)` for each start location.
pub fn parse_start_locations(sections: &[ChkSection<'_>]) -> Vec<(u8, u16, u16)> {
    let mut unit_data: Option<&[u8]> = None;
    for section in sections {
        if section.tag == *b"UNIT" {
            unit_data = Some(section.data);
        }
    }

    let Some(data) = unit_data else {
        return Vec::new();
    };

    let (entries, _) = data.as_chunks::<UNIT_ENTRY_SIZE>();
    entries
        .iter()
        .filter_map(|e| parse_unit_entry(e))
        .filter(|u| u.unit_type == START_LOCATION)
        .map(|u| (u.owner, u.x, u.y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chk;

    fn build_unit_entry(instance_id: u32, x: u16, y: u16, unit_type: u16, owner: u8) -> Vec<u8> {
        let mut entry = vec![0u8; UNIT_ENTRY_SIZE];
        entry[0..4].copy_from_slice(&instance_id.to_le_bytes());
        entry[4..6].copy_from_slice(&x.to_le_bytes());
        entry[6..8].copy_from_slice(&y.to_le_bytes());
        entry[8..10].copy_from_slice(&unit_type.to_le_bytes());
        entry[16] = owner;
        entry[17] = 100; // hp_percent
        entry[18] = 100; // shield_percent
        entry[19] = 100; // energy_percent
        entry
    }

    fn build_chk_with_units(unit_entries: &[Vec<u8>]) -> Vec<u8> {
        let mut unit_data = Vec::new();
        for entry in unit_entries {
            unit_data.extend_from_slice(entry);
        }
        let mut chk = Vec::new();
        chk.extend_from_slice(b"UNIT");
        chk.extend_from_slice(&(unit_data.len() as u32).to_le_bytes());
        chk.extend_from_slice(&unit_data);
        chk
    }

    #[test]
    fn test_parse_chk_units() {
        let entries = vec![
            build_unit_entry(0, 100, 200, 0, 0), // Marine, player 0
            build_unit_entry(1, 300, 400, 7, 1), // SCV, player 1
        ];
        let chk = build_chk_with_units(&entries);
        let sections = chk::parse_sections(&chk).unwrap();
        let units = parse_chk_units(&sections).unwrap();

        assert_eq!(units.len(), 2);
        assert_eq!(units[0].x, 100);
        assert_eq!(units[0].y, 200);
        assert_eq!(units[0].unit_type, 0);
        assert_eq!(units[0].owner, 0);
        assert_eq!(units[1].x, 300);
        assert_eq!(units[1].unit_type, 7);
        assert_eq!(units[1].owner, 1);
    }

    #[test]
    fn test_skip_invalid_unit_type() {
        let entries = vec![
            build_unit_entry(0, 100, 200, 0xFFFF, 0), // Invalid
            build_unit_entry(1, 300, 400, 0, 0),      // Valid
        ];
        let chk = build_chk_with_units(&entries);
        let sections = chk::parse_sections(&chk).unwrap();
        let units = parse_chk_units(&sections).unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].unit_type, 0);
    }

    #[test]
    fn test_no_unit_section() {
        let chk = Vec::new();
        let sections = chk::parse_sections(&chk).unwrap();
        let units = parse_chk_units(&sections).unwrap();
        assert!(units.is_empty());
    }
}
