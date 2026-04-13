use crate::error::{EngineError, Result};

const FLINGY_COUNT: usize = 209;
const UNIT_TYPE_COUNT: usize = 228;
const WEAPON_COUNT: usize = 130;

/// Flingy movement parameters for one flingy type.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlingyType {
    pub top_speed: i32,
    pub acceleration: i16,
    pub halt_distance: i32,
    pub turn_rate: u8,
    pub movement_type: u8,
}

/// Static unit type data parsed from units.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnitType {
    pub flingy_id: u8,
    pub hitpoints: i32,       // fp8
    pub ground_weapon: u8,    // 255 = none
    pub max_ground_hits: u8,
    pub air_weapon: u8,       // 255 = none
    pub max_air_hits: u8,
    pub armor: u8,
    pub build_time: u16,      // frames
    pub is_building: bool,
}

/// Weapon type data parsed from weapons.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeaponType {
    pub damage_amount: u16,
    pub damage_bonus: u16,
    pub cooldown: u8,
    pub damage_factor: u8,
    pub max_range: u32,       // in pixels (not fp8)
}

/// Parsed game data tables.
pub struct GameData {
    pub flingy_types: Vec<FlingyType>,
    pub unit_types: Vec<UnitType>,
    pub weapon_types: Vec<WeaponType>,
}

impl GameData {
    /// Parse from raw dat file bytes.
    ///
    /// `weapons_dat` may be empty — combat data will be zeroed.
    pub fn from_dat(units_dat: &[u8], flingy_dat: &[u8]) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_types = parse_units_dat(units_dat)?;
        Ok(Self {
            flingy_types,
            unit_types,
            weapon_types: Vec::new(),
        })
    }

    /// Parse with weapon data for combat.
    pub fn from_dat_full(
        units_dat: &[u8],
        flingy_dat: &[u8],
        weapons_dat: &[u8],
    ) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_types = parse_units_dat(units_dat)?;
        let weapon_types = parse_weapons_dat(weapons_dat)?;
        Ok(Self {
            flingy_types,
            unit_types,
            weapon_types,
        })
    }

    /// Get the flingy type for a given unit type.
    pub fn flingy_for_unit(&self, unit_type: u16) -> Option<&FlingyType> {
        let ut = self.unit_types.get(unit_type as usize)?;
        self.flingy_types.get(ut.flingy_id as usize)
    }

    /// Get unit type data.
    pub fn unit_type(&self, id: u16) -> Option<&UnitType> {
        self.unit_types.get(id as usize)
    }

    /// Get weapon type data.
    pub fn weapon_type(&self, id: u8) -> Option<&WeaponType> {
        if id == 130 {
            return None; // 130 = "None" weapon
        }
        self.weapon_types.get(id as usize)
    }
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_i16_le(data: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

// ---------------------------------------------------------------------------
// flingy.dat
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// units.dat — parallel array offsets (228 entries unless noted)
// ---------------------------------------------------------------------------

const U: usize = UNIT_TYPE_COUNT; // 228
const UNITS_COUNT: usize = 106;
const BUILDINGS_COUNT: usize = 96;

const U_FLINGY: usize = 0;                                           // 228 x u8
const U_TURRET: usize = U_FLINGY + U;                                // 228 x u16
const U_SUBUNIT2: usize = U_TURRET + U * 2;                          // 228 x u16
const U_INFESTATION: usize = U_SUBUNIT2 + U * 2;                     // 96 x u16
const U_CONSTRUCTION_ANIM: usize = U_INFESTATION + BUILDINGS_COUNT * 2; // 228 x u32
const U_UNIT_DIRECTION: usize = U_CONSTRUCTION_ANIM + U * 4;         // 228 x u8
const U_HAS_SHIELD: usize = U_UNIT_DIRECTION + U;                    // 228 x u8
const U_SHIELD_POINTS: usize = U_HAS_SHIELD + U;                     // 228 x u16
const U_HITPOINTS: usize = U_SHIELD_POINTS + U * 2;                  // 228 x i32
const U_ELEVATION: usize = U_HITPOINTS + U * 4;                      // 228 x u8
const U_UNKNOWN1: usize = U_ELEVATION + U;                           // 228 x u8
const U_SUBLABEL: usize = U_UNKNOWN1 + U;                            // 228 x u8
const U_COMP_AI_IDLE: usize = U_SUBLABEL + U;                        // 228 x u8
const U_HUMAN_AI_IDLE: usize = U_COMP_AI_IDLE + U;                   // 228 x u8
const U_RETURN_IDLE: usize = U_HUMAN_AI_IDLE + U;                    // 228 x u8
const U_ATTACK_UNIT: usize = U_RETURN_IDLE + U;                      // 228 x u8
const U_ATTACK_MOVE: usize = U_ATTACK_UNIT + U;                      // 228 x u8
const U_GROUND_WEAPON: usize = U_ATTACK_MOVE + U;                    // 228 x u8
const U_MAX_GROUND_HITS: usize = U_GROUND_WEAPON + U;                // 228 x u8
const U_AIR_WEAPON: usize = U_MAX_GROUND_HITS + U;                   // 228 x u8
const U_MAX_AIR_HITS: usize = U_AIR_WEAPON + U;                      // 228 x u8
const U_AI_INTERNAL: usize = U_MAX_AIR_HITS + U;                     // 228 x u8
const U_FLAGS: usize = U_AI_INTERNAL + U;                            // 228 x u32
const U_TARGET_ACQ_RANGE: usize = U_FLAGS + U * 4;                   // 228 x u8
const U_SIGHT_RANGE: usize = U_TARGET_ACQ_RANGE + U;                 // 228 x u8
const U_ARMOR_UPGRADE: usize = U_SIGHT_RANGE + U;                    // 228 x u8
const U_UNIT_SIZE: usize = U_ARMOR_UPGRADE + U;                      // 228 x u8
const U_ARMOR: usize = U_UNIT_SIZE + U;                              // 228 x u8
const U_RIGHT_CLICK: usize = U_ARMOR + U;                            // 228 x u8
const U_READY_SOUND: usize = U_RIGHT_CLICK + U;                      // 106 x u16
const U_FIRST_WHAT: usize = U_READY_SOUND + UNITS_COUNT * 2;         // 228 x u16
const U_LAST_WHAT: usize = U_FIRST_WHAT + U * 2;                     // 228 x u16
const U_FIRST_PISSED: usize = U_LAST_WHAT + U * 2;                   // 106 x u16
const U_LAST_PISSED: usize = U_FIRST_PISSED + UNITS_COUNT * 2;       // 106 x u16
const U_FIRST_YES: usize = U_LAST_PISSED + UNITS_COUNT * 2;          // 106 x u16
const U_LAST_YES: usize = U_FIRST_YES + UNITS_COUNT * 2;             // 106 x u16
const U_PLACEMENT_SIZE: usize = U_LAST_YES + UNITS_COUNT * 2;        // 228 x i16*2
const U_ADDON_POS: usize = U_PLACEMENT_SIZE + U * 4;                 // 96 x i16*2
const U_DIMENSIONS: usize = U_ADDON_POS + BUILDINGS_COUNT * 4;       // 228 x i16*4
const U_PORTRAIT: usize = U_DIMENSIONS + U * 8;                      // 228 x u16
const U_MINERAL_COST: usize = U_PORTRAIT + U * 2;                    // 228 x u16
const U_GAS_COST: usize = U_MINERAL_COST + U * 2;                    // 228 x u16
const U_BUILD_TIME: usize = U_GAS_COST + U * 2;                      // 228 x u16

/// Minimum units.dat size to read all fields we need (through build_time).
const UNITS_DAT_MIN_SIZE: usize = U_BUILD_TIME + U * 2;

/// Building flag in units.dat flags field.
const FLAG_BUILDING: u32 = 0x0000_0001;

fn parse_units_dat(data: &[u8]) -> Result<Vec<UnitType>> {
    // Gracefully handle minimal units.dat (just flingy field).
    if data.len() < U {
        return Err(EngineError::DatTooShort {
            file: "units.dat",
            expected: U,
            actual: data.len(),
        });
    }

    let has_full = data.len() >= UNITS_DAT_MIN_SIZE;
    let mut types = Vec::with_capacity(U);

    for i in 0..U {
        let flingy_id = data[U_FLINGY + i];

        let (hitpoints, ground_weapon, max_ground_hits, air_weapon, max_air_hits, armor, build_time, is_building) =
            if has_full {
                (
                    read_i32_le(data, U_HITPOINTS + i * 4),
                    data[U_GROUND_WEAPON + i],
                    data[U_MAX_GROUND_HITS + i],
                    data[U_AIR_WEAPON + i],
                    data[U_MAX_AIR_HITS + i],
                    data[U_ARMOR + i],
                    read_u16_le(data, U_BUILD_TIME + i * 2),
                    read_u32_le(data, U_FLAGS + i * 4) & FLAG_BUILDING != 0,
                )
            } else {
                (0, 130, 0, 130, 0, 0, 0, false)
            };

        types.push(UnitType {
            flingy_id,
            hitpoints,
            ground_weapon,
            max_ground_hits,
            air_weapon,
            max_air_hits,
            armor,
            build_time,
            is_building,
        });
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// weapons.dat — parallel array offsets (130 entries)
// ---------------------------------------------------------------------------

const W: usize = WEAPON_COUNT;

const W_LABEL: usize = 0;                                 // 130 x u16
const W_FLINGY: usize = W_LABEL + W * 2;                  // 130 x u32
const W_UNUSED: usize = W_FLINGY + W * 4;                 // 130 x u8
const W_TARGET_FLAGS: usize = W_UNUSED + W;                // 130 x u16
const W_MIN_RANGE: usize = W_TARGET_FLAGS + W * 2;         // 130 x u32
const W_MAX_RANGE: usize = W_MIN_RANGE + W * 4;            // 130 x u32
const W_DAMAGE_UPGRADE: usize = W_MAX_RANGE + W * 4;       // 130 x u8
const W_DAMAGE_TYPE: usize = W_DAMAGE_UPGRADE + W;         // 130 x u8
const W_BULLET_TYPE: usize = W_DAMAGE_TYPE + W;            // 130 x u8
const W_LIFETIME: usize = W_BULLET_TYPE + W;               // 130 x u8
const W_HIT_TYPE: usize = W_LIFETIME + W;                  // 130 x u8
const W_INNER_SPLASH: usize = W_HIT_TYPE + W;              // 130 x u16
const W_MEDIUM_SPLASH: usize = W_INNER_SPLASH + W * 2;     // 130 x u16
const W_OUTER_SPLASH: usize = W_MEDIUM_SPLASH + W * 2;     // 130 x u16
const W_DAMAGE_AMOUNT: usize = W_OUTER_SPLASH + W * 2;     // 130 x u16
const W_DAMAGE_BONUS: usize = W_DAMAGE_AMOUNT + W * 2;     // 130 x u16
const W_COOLDOWN: usize = W_DAMAGE_BONUS + W * 2;          // 130 x u8
const W_BULLET_COUNT: usize = W_COOLDOWN + W;              // 130 x u8

const WEAPONS_DAT_MIN_SIZE: usize = W_BULLET_COUNT + W;

fn parse_weapons_dat(data: &[u8]) -> Result<Vec<WeaponType>> {
    if data.len() < WEAPONS_DAT_MIN_SIZE {
        return Err(EngineError::DatTooShort {
            file: "weapons.dat",
            expected: WEAPONS_DAT_MIN_SIZE,
            actual: data.len(),
        });
    }

    let mut types = Vec::with_capacity(W);
    for i in 0..W {
        types.push(WeaponType {
            damage_amount: read_u16_le(data, W_DAMAGE_AMOUNT + i * 2),
            damage_bonus: read_u16_le(data, W_DAMAGE_BONUS + i * 2),
            cooldown: data[W_COOLDOWN + i],
            damage_factor: data[W_BULLET_COUNT + i], // bullet_count acts as damage_factor
            max_range: read_u32_le(data, W_MAX_RANGE + i * 4),
        });
    }

    Ok(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_flingy_dat() -> Vec<u8> {
        let mut data = vec![0u8; FLINGY_DAT_MIN_SIZE];
        let i = 0;
        let speed: i32 = 1024;
        data[FLINGY_TOP_SPEED_OFFSET + i * 4..FLINGY_TOP_SPEED_OFFSET + i * 4 + 4]
            .copy_from_slice(&speed.to_le_bytes());
        let accel: i16 = 256;
        data[FLINGY_ACCEL_OFFSET + i * 2..FLINGY_ACCEL_OFFSET + i * 2 + 2]
            .copy_from_slice(&accel.to_le_bytes());
        data[FLINGY_TURN_RATE_OFFSET + i] = 20;
        data[FLINGY_MOVE_TYPE_OFFSET + i] = 0;
        data
    }

    pub fn build_units_dat() -> Vec<u8> {
        let mut data = vec![0u8; UNITS_DAT_MIN_SIZE];
        // Marine (unit 0): flingy 0, 40 HP (40*256=10240 in fp8), ground weapon 0, 1 hit
        data[U_FLINGY] = 0;
        let hp: i32 = 40 * 256;
        data[U_HITPOINTS..U_HITPOINTS + 4].copy_from_slice(&hp.to_le_bytes());
        data[U_GROUND_WEAPON] = 0;      // weapon 0
        data[U_MAX_GROUND_HITS] = 1;
        data[U_AIR_WEAPON] = 130;        // no air weapon
        data[U_MAX_AIR_HITS] = 0;
        data[U_ARMOR] = 0;
        let bt: u16 = 360; // ~15 seconds at fastest
        data[U_BUILD_TIME..U_BUILD_TIME + 2].copy_from_slice(&bt.to_le_bytes());
        data
    }

    pub fn build_weapons_dat() -> Vec<u8> {
        let mut data = vec![0u8; WEAPONS_DAT_MIN_SIZE];
        // Weapon 0: 6 damage, 15 cooldown, 128 range (4 tiles), 1 bullet
        let dmg: u16 = 6;
        data[W_DAMAGE_AMOUNT..W_DAMAGE_AMOUNT + 2].copy_from_slice(&dmg.to_le_bytes());
        data[W_COOLDOWN] = 15;
        let range: u32 = 128;
        data[W_MAX_RANGE..W_MAX_RANGE + 4].copy_from_slice(&range.to_le_bytes());
        data[W_BULLET_COUNT] = 1; // damage_factor
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
    }

    #[test]
    fn test_parse_flingy_dat_too_short() {
        assert!(parse_flingy_dat(&vec![0u8; 100]).is_err());
    }

    #[test]
    fn test_parse_units_dat() {
        let data = build_units_dat();
        let types = parse_units_dat(&data).unwrap();
        assert_eq!(types.len(), U);
        assert_eq!(types[0].flingy_id, 0);
        assert_eq!(types[0].hitpoints, 40 * 256);
        assert_eq!(types[0].ground_weapon, 0);
        assert_eq!(types[0].build_time, 360);
    }

    #[test]
    fn test_parse_units_dat_minimal() {
        // Only flingy field (228 bytes) — should still parse.
        let data = vec![0u8; U];
        let types = parse_units_dat(&data).unwrap();
        assert_eq!(types.len(), U);
        assert_eq!(types[0].hitpoints, 0);
        assert_eq!(types[0].ground_weapon, 130); // no weapon
    }

    #[test]
    fn test_parse_weapons_dat() {
        let data = build_weapons_dat();
        let types = parse_weapons_dat(&data).unwrap();
        assert_eq!(types.len(), W);
        assert_eq!(types[0].damage_amount, 6);
        assert_eq!(types[0].cooldown, 15);
        assert_eq!(types[0].max_range, 128);
        assert_eq!(types[0].damage_factor, 1);
    }

    #[test]
    fn test_game_data_from_dat() {
        let flingy = build_flingy_dat();
        let units = build_units_dat();
        let gd = GameData::from_dat(&units, &flingy).unwrap();
        assert_eq!(gd.unit_type(0).unwrap().hitpoints, 40 * 256);
        let ft = gd.flingy_for_unit(0).unwrap();
        assert_eq!(ft.top_speed, 1024);
    }

    #[test]
    fn test_game_data_full() {
        let flingy = build_flingy_dat();
        let units = build_units_dat();
        let weapons = build_weapons_dat();
        let gd = GameData::from_dat_full(&units, &flingy, &weapons).unwrap();
        let wt = gd.weapon_type(0).unwrap();
        assert_eq!(wt.damage_amount, 6);
        assert!(gd.weapon_type(130).is_none()); // "None" weapon
    }
}
