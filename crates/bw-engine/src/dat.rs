use crate::error::{EngineError, Result};

const FLINGY_COUNT: usize = 209;
const UNIT_TYPE_COUNT: usize = 228;

/// Flingy movement parameters for one flingy type.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlingyType {
    pub top_speed: i32,
    pub acceleration: i16,
    pub halt_distance: i32,
    pub turn_rate: u8,
    pub movement_type: u8,
}

/// Parsed game data tables.
pub struct GameData {
    pub flingy_types: Vec<FlingyType>,
    /// Maps unit_type_id (0-227) -> flingy_type_id (0-208).
    pub unit_flingy: Vec<u8>,
}

impl GameData {
    /// Parse from raw `units.dat` and `flingy.dat` file bytes.
    pub fn from_dat(units_dat: &[u8], flingy_dat: &[u8]) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_flingy = parse_units_dat_flingy(units_dat)?;
        Ok(Self {
            flingy_types,
            unit_flingy,
        })
    }

    /// Get the flingy type for a given unit type.
    pub fn flingy_for_unit(&self, unit_type: u16) -> Option<&FlingyType> {
        let flingy_id = *self.unit_flingy.get(unit_type as usize)? as usize;
        self.flingy_types.get(flingy_id)
    }
}

fn read_i16_le(data: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

/// flingy.dat layout (209 entries, parallel arrays):
///   offset    0: sprite     209 x u16 = 418 bytes
///   offset  418: top_speed  209 x i32 = 836 bytes
///   offset 1254: accel      209 x i16 = 418 bytes
///   offset 1672: halt_dist  209 x i32 = 836 bytes
///   offset 2508: turn_rate  209 x u8  = 209 bytes
///   offset 2717: unused     209 x u8  = 209 bytes
///   offset 2926: move_type  209 x u8  = 209 bytes
///   total: 3135 bytes
const FLINGY_DAT_MIN_SIZE: usize = 3135;
const FLINGY_SPRITE_OFFSET: usize = 0;
const FLINGY_TOP_SPEED_OFFSET: usize = FLINGY_SPRITE_OFFSET + FLINGY_COUNT * 2;
const FLINGY_ACCEL_OFFSET: usize = FLINGY_TOP_SPEED_OFFSET + FLINGY_COUNT * 4;
const FLINGY_HALT_OFFSET: usize = FLINGY_ACCEL_OFFSET + FLINGY_COUNT * 2;
const FLINGY_TURN_RATE_OFFSET: usize = FLINGY_HALT_OFFSET + FLINGY_COUNT * 4;
const FLINGY_UNUSED_OFFSET: usize = FLINGY_TURN_RATE_OFFSET + FLINGY_COUNT;
const FLINGY_MOVE_TYPE_OFFSET: usize = FLINGY_UNUSED_OFFSET + FLINGY_COUNT;

fn parse_flingy_dat(data: &[u8]) -> Result<Vec<FlingyType>> {
    if data.len() < FLINGY_DAT_MIN_SIZE {
        return Err(EngineError::DatTooShort {
            file: "flingy.dat",
            expected: FLINGY_DAT_MIN_SIZE,
            actual: data.len(),
        });
    }

    let mut types = Vec::with_capacity(FLINGY_COUNT);
    for i in 0..FLINGY_COUNT {
        types.push(FlingyType {
            top_speed: read_i32_le(data, FLINGY_TOP_SPEED_OFFSET + i * 4),
            acceleration: read_i16_le(data, FLINGY_ACCEL_OFFSET + i * 2),
            halt_distance: read_i32_le(data, FLINGY_HALT_OFFSET + i * 4),
            turn_rate: data[FLINGY_TURN_RATE_OFFSET + i],
            movement_type: data[FLINGY_MOVE_TYPE_OFFSET + i],
        });
    }
    Ok(types)
}

/// units.dat: we only read the flingy_id field (first 228 bytes).
fn parse_units_dat_flingy(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < UNIT_TYPE_COUNT {
        return Err(EngineError::DatTooShort {
            file: "units.dat",
            expected: UNIT_TYPE_COUNT,
            actual: data.len(),
        });
    }
    Ok(data[..UNIT_TYPE_COUNT].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_flingy_dat() -> Vec<u8> {
        let mut data = vec![0u8; FLINGY_DAT_MIN_SIZE];
        // Set flingy 0 (Marine): top_speed=4*256=1024, accel=1*256=256, halt=0, turn=20
        let i = 0;
        let speed: i32 = 1024;
        data[FLINGY_TOP_SPEED_OFFSET + i * 4..FLINGY_TOP_SPEED_OFFSET + i * 4 + 4]
            .copy_from_slice(&speed.to_le_bytes());
        let accel: i16 = 256;
        data[FLINGY_ACCEL_OFFSET + i * 2..FLINGY_ACCEL_OFFSET + i * 2 + 2]
            .copy_from_slice(&accel.to_le_bytes());
        data[FLINGY_TURN_RATE_OFFSET + i] = 20;
        data[FLINGY_MOVE_TYPE_OFFSET + i] = 0; // ground
        data
    }

    fn build_units_dat() -> Vec<u8> {
        let mut data = vec![0u8; UNIT_TYPE_COUNT];
        data[0] = 0; // Marine (unit 0) -> flingy 0
        data[7] = 0; // SCV (unit 7) -> flingy 0 (simplified)
        data
    }

    #[test]
    fn test_parse_flingy_dat() {
        let data = build_flingy_dat();
        let types = parse_flingy_dat(&data).unwrap();
        assert_eq!(types.len(), FLINGY_COUNT);
        assert_eq!(types[0].top_speed, 1024);
        assert_eq!(types[0].acceleration, 256);
        assert_eq!(types[0].turn_rate, 20);
        assert_eq!(types[0].movement_type, 0);
    }

    #[test]
    fn test_parse_flingy_dat_too_short() {
        let data = vec![0u8; 100];
        assert!(parse_flingy_dat(&data).is_err());
    }

    #[test]
    fn test_parse_units_dat_flingy() {
        let data = build_units_dat();
        let flingy_ids = parse_units_dat_flingy(&data).unwrap();
        assert_eq!(flingy_ids.len(), UNIT_TYPE_COUNT);
        assert_eq!(flingy_ids[0], 0);
    }

    #[test]
    fn test_game_data_from_dat() {
        let flingy = build_flingy_dat();
        let units = build_units_dat();
        let gd = GameData::from_dat(&units, &flingy).unwrap();
        let ft = gd.flingy_for_unit(0).unwrap();
        assert_eq!(ft.top_speed, 1024);
    }

    #[test]
    fn test_flingy_for_unknown_unit() {
        let flingy = build_flingy_dat();
        let units = build_units_dat();
        let gd = GameData::from_dat(&units, &flingy).unwrap();
        assert!(gd.flingy_for_unit(999).is_none());
    }
}
