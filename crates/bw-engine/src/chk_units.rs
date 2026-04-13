use crate::chk::ChkSection;
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

    let count = data.len() / UNIT_ENTRY_SIZE;
    let mut units = Vec::with_capacity(count);

    for i in 0..count {
        let base = i * UNIT_ENTRY_SIZE;
        if base + UNIT_ENTRY_SIZE > data.len() {
            break;
        }

        let instance_id = u32::from_le_bytes(data[base..base + 4].try_into().unwrap());
        let x = u16::from_le_bytes(data[base + 4..base + 6].try_into().unwrap());
        let y = u16::from_le_bytes(data[base + 6..base + 8].try_into().unwrap());
        let unit_type = u16::from_le_bytes(data[base + 8..base + 10].try_into().unwrap());
        // bytes 10-11: link
        // bytes 12-13: valid_flags
        // bytes 14-15: valid_properties
        let owner = data[base + 16];
        let hp_percent = data[base + 17];
        let shield_percent = data[base + 18];
        let energy_percent = data[base + 19];
        let resources = u32::from_le_bytes(data[base + 20..base + 24].try_into().unwrap());
        // bytes 24-25: units_in_hangar
        // bytes 26-27: state_flags
        // bytes 28-31: unused
        // bytes 32-35: related_unit_id

        // Skip invalid entries (unit_type 0xFFFF).
        if unit_type == 0xFFFF {
            continue;
        }
        // Skip unit types >= 228.
        if unit_type >= 228 {
            continue;
        }

        units.push(ChkUnit {
            instance_id,
            x,
            y,
            unit_type,
            owner,
            hp_percent,
            shield_percent,
            energy_percent,
            resources,
        });
    }

    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chk;

    fn build_unit_entry(
        instance_id: u32,
        x: u16,
        y: u16,
        unit_type: u16,
        owner: u8,
    ) -> Vec<u8> {
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
            build_unit_entry(0, 100, 200, 0, 0),  // Marine, player 0
            build_unit_entry(1, 300, 400, 7, 1),   // SCV, player 1
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
            build_unit_entry(1, 300, 400, 0, 0),       // Valid
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
