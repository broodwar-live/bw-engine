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
    pub turret_unit_type: u16, // 228 = none
    pub hitpoints: i32,        // fp8
    pub shield_points: u16,    // NOT fp8 — raw shield HP
    pub has_shield: bool,
    pub ground_weapon: u8, // 130 = none
    pub max_ground_hits: u8,
    pub air_weapon: u8, // 130 = none
    pub max_air_hits: u8,
    pub armor: u8,
    pub armor_upgrade: u8, // upgrade ID that increases armor
    pub unit_size: UnitSize,
    pub elevation: u8,   // >4 means air unit
    pub sight_range: u8, // in tiles (32px)
    pub build_time: u16, // frames
    pub mineral_cost: u16,
    pub gas_cost: u16,
    pub supply_cost: u8,     // in half-units (Marine = 2, Zergling = 1)
    pub supply_provided: u8, // in half-units (Supply Depot = 16, Pylon = 16)
    pub is_building: bool,
}

impl UnitType {
    /// Whether this unit type is an air unit (flyer).
    pub fn is_air(&self) -> bool {
        self.elevation >= 4
    }
}

/// Unit size class for damage type calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum UnitSize {
    /// Independent (buildings, special units) — takes full damage from all types.
    #[default]
    Independent = 0,
    /// Small (Marine, Zergling, etc.)
    Small = 1,
    /// Medium (Vulture, Hydralisk, etc.)
    Medium = 2,
    /// Large (Siege Tank, Ultralisk, etc.)
    Large = 3,
}

impl UnitSize {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Independent,
            1 => Self::Small,
            2 => Self::Medium,
            3 => Self::Large,
            _ => Self::Independent,
        }
    }
}

/// Terran_Command_Center unit type ID — turrets only created for units below this.
pub const COMMAND_CENTER_ID: u16 = 117;

/// Weapon type data parsed from weapons.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeaponType {
    pub damage_amount: u16,
    pub damage_bonus: u16,
    pub cooldown: u8,
    pub damage_factor: u8,
    pub damage_type: DamageType,
    pub damage_upgrade: u8, // upgrade ID that increases this weapon's damage
    pub max_range: u32,     // in pixels (not fp8)
    pub inner_splash: u16,  // pixels — full damage radius
    pub medium_splash: u16, // pixels — 50% damage radius
    pub outer_splash: u16,  // pixels — 25% damage radius
}

impl WeaponType {
    /// Whether this weapon deals splash (area) damage.
    pub fn is_splash(&self) -> bool {
        self.outer_splash > 0
    }
}

/// Weapon damage type for size-based modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DamageType {
    #[default]
    Independent = 0,
    Explosive = 1,
    Concussive = 2,
    Normal = 3,
    IgnoreArmor = 4,
}

impl DamageType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Independent,
            1 => Self::Explosive,
            2 => Self::Concussive,
            3 => Self::Normal,
            4 => Self::IgnoreArmor,
            _ => Self::Independent,
        }
    }

    /// Damage multiplier against a given unit size.
    /// Returns (numerator, denominator) to avoid floats.
    pub fn size_modifier(self, size: UnitSize) -> (u32, u32) {
        match self {
            DamageType::Concussive => match size {
                UnitSize::Small => (1, 1),
                UnitSize::Medium => (1, 2),
                UnitSize::Large => (1, 4),
                UnitSize::Independent => (1, 1),
            },
            DamageType::Explosive => match size {
                UnitSize::Small => (1, 2),
                UnitSize::Medium => (3, 4),
                UnitSize::Large => (1, 1),
                UnitSize::Independent => (1, 1),
            },
            DamageType::Normal | DamageType::Independent | DamageType::IgnoreArmor => (1, 1),
        }
    }
}

/// Parsed game data tables.
pub struct GameData {
    pub flingy_types: Vec<FlingyType>,
    pub unit_types: Vec<UnitType>,
    pub weapon_types: Vec<WeaponType>,
    pub tech_types: Vec<TechType>,
    pub upgrade_types: Vec<UpgradeType>,
    pub order_types: Vec<OrderType>,
    /// Fallback flingy data indexed by unit_type (for SC:R .dat compatibility).
    pub fallback_flingy: Vec<FlingyType>,
}

impl GameData {
    /// Parse from raw dat file bytes (units + flingy only).
    ///
    /// Combat, tech, upgrade, and order data will be empty.
    pub fn from_dat(units_dat: &[u8], flingy_dat: &[u8]) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_types = parse_units_dat(units_dat)?;
        Ok(Self {
            flingy_types,
            unit_types,
            weapon_types: Vec::new(),
            tech_types: Vec::new(),
            upgrade_types: Vec::new(),
            order_types: Vec::new(),
            fallback_flingy: build_fallback_flingy(),
        })
    }

    /// Parse with weapon data for combat.
    pub fn from_dat_full(units_dat: &[u8], flingy_dat: &[u8], weapons_dat: &[u8]) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_types = parse_units_dat(units_dat)?;
        let weapon_types = parse_weapons_dat(weapons_dat)?;
        Ok(Self {
            flingy_types,
            unit_types,
            weapon_types,
            tech_types: Vec::new(),
            upgrade_types: Vec::new(),
            order_types: Vec::new(),
            fallback_flingy: build_fallback_flingy(),
        })
    }

    /// Parse all dat files for complete game data.
    pub fn from_dat_all(
        units_dat: &[u8],
        flingy_dat: &[u8],
        weapons_dat: &[u8],
        techdata_dat: &[u8],
        upgrades_dat: &[u8],
        orders_dat: &[u8],
    ) -> Result<Self> {
        let flingy_types = parse_flingy_dat(flingy_dat)?;
        let unit_types = parse_units_dat(units_dat)?;
        let weapon_types = parse_weapons_dat(weapons_dat)?;
        let tech_types = parse_techdata_dat(techdata_dat)?;
        let upgrade_types = parse_upgrades_dat(upgrades_dat)?;
        let order_types = parse_orders_dat(orders_dat)?;
        Ok(Self {
            flingy_types,
            unit_types,
            weapon_types,
            tech_types,
            upgrade_types,
            order_types,
            fallback_flingy: build_fallback_flingy(),
        })
    }

    /// Get the flingy type for a given unit type.
    ///
    /// If the .dat file has invalid speed data (SC:R reorganized flingy IDs),
    /// falls back to hardcoded original BW 1.16.1 values.
    pub fn flingy_for_unit(&self, unit_type: u16) -> Option<&FlingyType> {
        let ut = self.unit_types.get(unit_type as usize)?;
        let parsed = self.flingy_types.get(ut.flingy_id as usize)?;

        // SC:R's .dat files have speed=1 for many units (reorganized flingy IDs).
        // If the parsed speed looks wrong, use the fallback.
        if parsed.top_speed <= 1
            && !ut.is_building
            && let Some(fb) = self.fallback_flingy.get(unit_type as usize)
            && fb.top_speed > 1
        {
            return Some(fb);
        }
        Some(parsed)
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

    /// Get tech type data.
    pub fn tech_type(&self, id: u8) -> Option<&TechType> {
        self.tech_types.get(id as usize)
    }

    /// Get upgrade type data.
    pub fn upgrade_type(&self, id: u8) -> Option<&UpgradeType> {
        self.upgrade_types.get(id as usize)
    }

    /// Get order type data.
    pub fn order_type(&self, id: u8) -> Option<&OrderType> {
        self.order_types.get(id as usize)
    }
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_i16_le(data: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
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

const U_FLINGY: usize = 0; // 228 x u8
const U_TURRET: usize = U_FLINGY + U; // 228 x u16
const U_SUBUNIT2: usize = U_TURRET + U * 2; // 228 x u16
const U_INFESTATION: usize = U_SUBUNIT2 + U * 2; // 96 x u16
const U_CONSTRUCTION_ANIM: usize = U_INFESTATION + BUILDINGS_COUNT * 2; // 228 x u32
const U_UNIT_DIRECTION: usize = U_CONSTRUCTION_ANIM + U * 4; // 228 x u8
const U_HAS_SHIELD: usize = U_UNIT_DIRECTION + U; // 228 x u8
const U_SHIELD_POINTS: usize = U_HAS_SHIELD + U; // 228 x u16
const U_HITPOINTS: usize = U_SHIELD_POINTS + U * 2; // 228 x i32
const U_ELEVATION: usize = U_HITPOINTS + U * 4; // 228 x u8
const U_UNKNOWN1: usize = U_ELEVATION + U; // 228 x u8
const U_SUBLABEL: usize = U_UNKNOWN1 + U; // 228 x u8
const U_COMP_AI_IDLE: usize = U_SUBLABEL + U; // 228 x u8
const U_HUMAN_AI_IDLE: usize = U_COMP_AI_IDLE + U; // 228 x u8
const U_RETURN_IDLE: usize = U_HUMAN_AI_IDLE + U; // 228 x u8
const U_ATTACK_UNIT: usize = U_RETURN_IDLE + U; // 228 x u8
const U_ATTACK_MOVE: usize = U_ATTACK_UNIT + U; // 228 x u8
const U_GROUND_WEAPON: usize = U_ATTACK_MOVE + U; // 228 x u8
const U_MAX_GROUND_HITS: usize = U_GROUND_WEAPON + U; // 228 x u8
const U_AIR_WEAPON: usize = U_MAX_GROUND_HITS + U; // 228 x u8
const U_MAX_AIR_HITS: usize = U_AIR_WEAPON + U; // 228 x u8
const U_AI_INTERNAL: usize = U_MAX_AIR_HITS + U; // 228 x u8
const U_FLAGS: usize = U_AI_INTERNAL + U; // 228 x u32
const U_TARGET_ACQ_RANGE: usize = U_FLAGS + U * 4; // 228 x u8
const U_SIGHT_RANGE: usize = U_TARGET_ACQ_RANGE + U; // 228 x u8
const U_ARMOR_UPGRADE: usize = U_SIGHT_RANGE + U; // 228 x u8
const U_UNIT_SIZE: usize = U_ARMOR_UPGRADE + U; // 228 x u8
const U_ARMOR: usize = U_UNIT_SIZE + U; // 228 x u8
const U_RIGHT_CLICK: usize = U_ARMOR + U; // 228 x u8
const U_READY_SOUND: usize = U_RIGHT_CLICK + U; // 106 x u16
const U_FIRST_WHAT: usize = U_READY_SOUND + UNITS_COUNT * 2; // 228 x u16
const U_LAST_WHAT: usize = U_FIRST_WHAT + U * 2; // 228 x u16
const U_FIRST_PISSED: usize = U_LAST_WHAT + U * 2; // 106 x u16
const U_LAST_PISSED: usize = U_FIRST_PISSED + UNITS_COUNT * 2; // 106 x u16
const U_FIRST_YES: usize = U_LAST_PISSED + UNITS_COUNT * 2; // 106 x u16
const U_LAST_YES: usize = U_FIRST_YES + UNITS_COUNT * 2; // 106 x u16
const U_PLACEMENT_SIZE: usize = U_LAST_YES + UNITS_COUNT * 2; // 228 x i16*2
const U_ADDON_POS: usize = U_PLACEMENT_SIZE + U * 4; // 96 x i16*2
const U_DIMENSIONS: usize = U_ADDON_POS + BUILDINGS_COUNT * 4; // 228 x i16*4
const U_PORTRAIT: usize = U_DIMENSIONS + U * 8; // 228 x u16
const U_MINERAL_COST: usize = U_PORTRAIT + U * 2; // 228 x u16
const U_GAS_COST: usize = U_MINERAL_COST + U * 2; // 228 x u16
const U_BUILD_TIME: usize = U_GAS_COST + U * 2; // 228 x u16
const U_SUPPLY_REQUIRED: usize = U_BUILD_TIME + U * 2; // 228 x u8 (in half-units)
const U_SUPPLY_PROVIDED: usize = U_SUPPLY_REQUIRED + U; // 228 x u8 (in half-units)

/// Minimum units.dat size to read all fields we need (through supply).
const UNITS_DAT_MIN_SIZE: usize = U_SUPPLY_PROVIDED + U;

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

        if has_full {
            types.push(UnitType {
                flingy_id,
                turret_unit_type: read_u16_le(data, U_TURRET + i * 2),
                hitpoints: read_i32_le(data, U_HITPOINTS + i * 4),
                shield_points: read_u16_le(data, U_SHIELD_POINTS + i * 2),
                has_shield: data[U_HAS_SHIELD + i] != 0,
                ground_weapon: data[U_GROUND_WEAPON + i],
                max_ground_hits: data[U_MAX_GROUND_HITS + i],
                air_weapon: data[U_AIR_WEAPON + i],
                max_air_hits: data[U_MAX_AIR_HITS + i],
                armor: data[U_ARMOR + i],
                armor_upgrade: data[U_ARMOR_UPGRADE + i],
                unit_size: UnitSize::from_u8(data[U_UNIT_SIZE + i]),
                elevation: data[U_ELEVATION + i],
                sight_range: data[U_SIGHT_RANGE + i],
                build_time: read_u16_le(data, U_BUILD_TIME + i * 2),
                mineral_cost: read_u16_le(data, U_MINERAL_COST + i * 2),
                gas_cost: read_u16_le(data, U_GAS_COST + i * 2),
                supply_cost: data.get(U_SUPPLY_REQUIRED + i).copied().unwrap_or(0),
                supply_provided: data.get(U_SUPPLY_PROVIDED + i).copied().unwrap_or(0),
                is_building: read_u32_le(data, U_FLAGS + i * 4) & FLAG_BUILDING != 0,
            });
        } else {
            types.push(UnitType {
                flingy_id,
                turret_unit_type: 228,
                hitpoints: 0,
                shield_points: 0,
                has_shield: false,
                ground_weapon: 130,
                max_ground_hits: 0,
                air_weapon: 130,
                max_air_hits: 0,
                armor: 0,
                armor_upgrade: 0,
                unit_size: UnitSize::Independent,
                elevation: 0,
                sight_range: 7,
                build_time: 0,
                mineral_cost: 0,
                gas_cost: 0,
                supply_cost: 0,
                supply_provided: 0,
                is_building: false,
            });
        }
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// weapons.dat — parallel array offsets (130 entries)
// ---------------------------------------------------------------------------

const W: usize = WEAPON_COUNT;

const W_LABEL: usize = 0; // 130 x u16
const W_FLINGY: usize = W_LABEL + W * 2; // 130 x u32
const W_UNUSED: usize = W_FLINGY + W * 4; // 130 x u8
const W_TARGET_FLAGS: usize = W_UNUSED + W; // 130 x u16
const W_MIN_RANGE: usize = W_TARGET_FLAGS + W * 2; // 130 x u32
const W_MAX_RANGE: usize = W_MIN_RANGE + W * 4; // 130 x u32
const W_DAMAGE_UPGRADE: usize = W_MAX_RANGE + W * 4; // 130 x u8
const W_DAMAGE_TYPE: usize = W_DAMAGE_UPGRADE + W; // 130 x u8
const W_BULLET_TYPE: usize = W_DAMAGE_TYPE + W; // 130 x u8
const W_LIFETIME: usize = W_BULLET_TYPE + W; // 130 x u8
const W_HIT_TYPE: usize = W_LIFETIME + W; // 130 x u8
const W_INNER_SPLASH: usize = W_HIT_TYPE + W; // 130 x u16
const W_MEDIUM_SPLASH: usize = W_INNER_SPLASH + W * 2; // 130 x u16
const W_OUTER_SPLASH: usize = W_MEDIUM_SPLASH + W * 2; // 130 x u16
const W_DAMAGE_AMOUNT: usize = W_OUTER_SPLASH + W * 2; // 130 x u16
const W_DAMAGE_BONUS: usize = W_DAMAGE_AMOUNT + W * 2; // 130 x u16
const W_COOLDOWN: usize = W_DAMAGE_BONUS + W * 2; // 130 x u8
const W_BULLET_COUNT: usize = W_COOLDOWN + W; // 130 x u8

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
            damage_type: DamageType::from_u8(data[W_DAMAGE_TYPE + i]),
            damage_upgrade: data[W_DAMAGE_UPGRADE + i],
            max_range: read_u32_le(data, W_MAX_RANGE + i * 4),
            inner_splash: read_u16_le(data, W_INNER_SPLASH + i * 2),
            medium_splash: read_u16_le(data, W_MEDIUM_SPLASH + i * 2),
            outer_splash: read_u16_le(data, W_OUTER_SPLASH + i * 2),
        });
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// techdata.dat — parallel array offsets (44 entries)
// ---------------------------------------------------------------------------

const TECH_COUNT: usize = 44;
const T: usize = TECH_COUNT;

const T_MINERAL_COST: usize = 0; // 44 x u16
const T_GAS_COST: usize = T_MINERAL_COST + T * 2; // 44 x u16
const T_RESEARCH_TIME: usize = T_GAS_COST + T * 2; // 44 x u16
const T_ENERGY_COST: usize = T_RESEARCH_TIME + T * 2; // 44 x u16
const T_UNKNOWN: usize = T_ENERGY_COST + T * 2; // 44 x u32
const T_ICON: usize = T_UNKNOWN + T * 4; // 44 x u16
const T_LABEL: usize = T_ICON + T * 2; // 44 x u16
const T_RACE: usize = T_LABEL + T * 2; // 44 x u8
const T_UNUSED: usize = T_RACE + T; // 44 x u8
const T_BROOD_WAR: usize = T_UNUSED + T; // 44 x u8

const TECHDATA_DAT_MIN_SIZE: usize = T_BROOD_WAR + T;

/// Tech research data parsed from techdata.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct TechType {
    pub mineral_cost: u16,
    pub gas_cost: u16,
    pub research_time: u16,
    pub energy_cost: u16,
    pub label: u16,
    pub race: u8,
    pub brood_war: bool,
}

fn parse_techdata_dat(data: &[u8]) -> Result<Vec<TechType>> {
    if data.len() < TECHDATA_DAT_MIN_SIZE {
        return Err(EngineError::DatTooShort {
            file: "techdata.dat",
            expected: TECHDATA_DAT_MIN_SIZE,
            actual: data.len(),
        });
    }

    let mut types = Vec::with_capacity(T);
    for i in 0..T {
        types.push(TechType {
            mineral_cost: read_u16_le(data, T_MINERAL_COST + i * 2),
            gas_cost: read_u16_le(data, T_GAS_COST + i * 2),
            research_time: read_u16_le(data, T_RESEARCH_TIME + i * 2),
            energy_cost: read_u16_le(data, T_ENERGY_COST + i * 2),
            label: read_u16_le(data, T_LABEL + i * 2),
            race: data[T_RACE + i],
            brood_war: data[T_BROOD_WAR + i] != 0,
        });
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// upgrades.dat — parallel array offsets (61 entries)
// ---------------------------------------------------------------------------

const UPGRADE_COUNT: usize = 61;
const UG: usize = UPGRADE_COUNT;

const UG_MINERAL_BASE: usize = 0; // 61 x u16
const UG_MINERAL_FACTOR: usize = UG_MINERAL_BASE + UG * 2; // 61 x u16
const UG_GAS_BASE: usize = UG_MINERAL_FACTOR + UG * 2; // 61 x u16
const UG_GAS_FACTOR: usize = UG_GAS_BASE + UG * 2; // 61 x u16
const UG_TIME_BASE: usize = UG_GAS_FACTOR + UG * 2; // 61 x u16
const UG_TIME_FACTOR: usize = UG_TIME_BASE + UG * 2; // 61 x u16
const UG_UNKNOWN: usize = UG_TIME_FACTOR + UG * 2; // 61 x u16
const UG_ICON: usize = UG_UNKNOWN + UG * 2; // 61 x u16
const UG_LABEL: usize = UG_ICON + UG * 2; // 61 x u16
const UG_RACE: usize = UG_LABEL + UG * 2; // 61 x u8
const UG_MAX_REPEATS: usize = UG_RACE + UG; // 61 x u8
const UG_BROOD_WAR: usize = UG_MAX_REPEATS + UG; // 61 x u8

const UPGRADES_DAT_MIN_SIZE: usize = UG_BROOD_WAR + UG;

/// Upgrade data parsed from upgrades.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct UpgradeType {
    pub mineral_base: u16,
    pub mineral_factor: u16,
    pub gas_base: u16,
    pub gas_factor: u16,
    pub time_base: u16,
    pub time_factor: u16,
    pub label: u16,
    pub race: u8,
    pub max_repeats: u8,
    pub brood_war: bool,
}

impl UpgradeType {
    /// Cost at a given level (1-indexed).
    pub fn cost_at_level(&self, level: u8) -> (u16, u16) {
        let l = level.saturating_sub(1) as u16;
        (
            self.mineral_base + self.mineral_factor * l,
            self.gas_base + self.gas_factor * l,
        )
    }

    /// Research time at a given level (1-indexed), in frames.
    pub fn time_at_level(&self, level: u8) -> u16 {
        let l = level.saturating_sub(1) as u16;
        self.time_base + self.time_factor * l
    }
}

fn parse_upgrades_dat(data: &[u8]) -> Result<Vec<UpgradeType>> {
    if data.len() < UPGRADES_DAT_MIN_SIZE {
        return Err(EngineError::DatTooShort {
            file: "upgrades.dat",
            expected: UPGRADES_DAT_MIN_SIZE,
            actual: data.len(),
        });
    }

    let mut types = Vec::with_capacity(UG);
    for i in 0..UG {
        types.push(UpgradeType {
            mineral_base: read_u16_le(data, UG_MINERAL_BASE + i * 2),
            mineral_factor: read_u16_le(data, UG_MINERAL_FACTOR + i * 2),
            gas_base: read_u16_le(data, UG_GAS_BASE + i * 2),
            gas_factor: read_u16_le(data, UG_GAS_FACTOR + i * 2),
            time_base: read_u16_le(data, UG_TIME_BASE + i * 2),
            time_factor: read_u16_le(data, UG_TIME_FACTOR + i * 2),
            label: read_u16_le(data, UG_LABEL + i * 2),
            race: data[UG_RACE + i],
            max_repeats: data[UG_MAX_REPEATS + i],
            brood_war: data[UG_BROOD_WAR + i] != 0,
        });
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// orders.dat — parallel array offsets (189 entries)
// ---------------------------------------------------------------------------

const ORDER_COUNT: usize = 189;
const O: usize = ORDER_COUNT;

const O_LABEL: usize = 0; // 189 x u16
const O_USE_WEAPON: usize = O_LABEL + O * 2; // 189 x u8
const O_SECONDARY: usize = O_USE_WEAPON + O; // 189 x u8 (unused)
const O_NON_SUBUNIT: usize = O_SECONDARY + O; // 189 x u8 (unused)
const O_INTERRUPTABLE: usize = O_NON_SUBUNIT + O; // 189 x u8
const O_STOP_MOVING: usize = O_INTERRUPTABLE + O; // 189 x u8
const O_CAN_BE_QUEUED: usize = O_STOP_MOVING + O; // 189 x u8
const O_KEEP_TARGET: usize = O_CAN_BE_QUEUED + O; // 189 x u8
const O_TERRAIN_CLIP: usize = O_KEEP_TARGET + O; // 189 x u8
const O_FLEEING: usize = O_TERRAIN_CLIP + O; // 189 x u8
const O_REQUIRES_MOVE: usize = O_FLEEING + O; // 189 x u8
const O_ORDER_WEAPON: usize = O_REQUIRES_MOVE + O; // 189 x u8
const O_ANIMATION: usize = O_ORDER_WEAPON + O; // 189 x u8
const O_HIGHLIGHT: usize = O_ANIMATION + O; // 189 x u16
const O_UNKNOWN: usize = O_HIGHLIGHT + O * 2; // 189 x u16
const O_TARGETING: usize = O_UNKNOWN + O * 2; // 189 x u8

const ORDERS_DAT_MIN_SIZE: usize = O_TARGETING + O;

/// Order data parsed from orders.dat.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrderType {
    pub label: u16,
    pub use_weapon_targeting: bool,
    pub interruptable: bool,
    pub can_be_queued: bool,
    pub keep_target: bool,
    pub requires_move: bool,
    pub order_weapon: u8,
}

fn parse_orders_dat(data: &[u8]) -> Result<Vec<OrderType>> {
    if data.len() < ORDERS_DAT_MIN_SIZE {
        return Err(EngineError::DatTooShort {
            file: "orders.dat",
            expected: ORDERS_DAT_MIN_SIZE,
            actual: data.len(),
        });
    }

    let mut types = Vec::with_capacity(O);
    for i in 0..O {
        types.push(OrderType {
            label: read_u16_le(data, O_LABEL + i * 2),
            use_weapon_targeting: data[O_USE_WEAPON + i] != 0,
            interruptable: data[O_INTERRUPTABLE + i] != 0,
            can_be_queued: data[O_CAN_BE_QUEUED + i] != 0,
            keep_target: data[O_KEEP_TARGET + i] != 0,
            requires_move: data[O_REQUIRES_MOVE + i] != 0,
            order_weapon: data[O_ORDER_WEAPON + i],
        });
    }

    Ok(types)
}

/// Build fallback flingy data from original BW 1.16.1 values.
/// Indexed by unit_type_id (0-227). Covers all common competitive units.
fn build_fallback_flingy() -> Vec<FlingyType> {
    let mut table = vec![FlingyType::default(); UNIT_TYPE_COUNT];

    // (unit_type, top_speed, acceleration, halt_distance, turn_rate, movement_type)
    let entries: &[(usize, i32, i16, i32, u8, u8)] = &[
        // Terran
        (0, 1024, 17, 1, 40, 0),      // Marine
        (1, 1024, 17, 1, 40, 0),      // Ghost
        (2, 1707, 100, 40960, 40, 0), // Vulture
        (3, 853, 17, 1, 20, 0),       // Goliath
        (5, 853, 17, 1, 20, 0),       // Siege Tank (Tank)
        (7, 1280, 67, 30720, 40, 0),  // SCV
        (8, 1707, 67, 40960, 40, 2),  // Wraith
        (9, 1280, 50, 40960, 40, 2),  // Science Vessel
        (11, 1400, 17, 40960, 40, 2), // Dropship
        (12, 640, 27, 40960, 20, 2),  // Battlecruiser
        (30, 853, 17, 1, 20, 0),      // Siege Tank (Siege)
        (32, 1024, 17, 1, 40, 0),     // Firebat
        (34, 1024, 17, 1, 40, 0),     // Medic
        (58, 1707, 67, 40960, 40, 2), // Valkyrie
        // Zerg
        (35, 1, 1, 1, 1, 0),          // Larva
        (37, 1397, 67, 20480, 27, 0), // Zergling
        (38, 1024, 17, 1, 27, 0),     // Hydralisk
        (39, 1397, 67, 20480, 40, 0), // Ultralisk
        (41, 1280, 67, 30720, 40, 0), // Drone
        (42, 1067, 17, 40960, 40, 2), // Overlord (before speed upgrade)
        (43, 1707, 67, 40960, 40, 2), // Mutalisk
        (44, 640, 27, 40960, 20, 2),  // Guardian
        (45, 1280, 50, 40960, 40, 2), // Queen
        (46, 853, 27, 1, 27, 0),      // Defiler
        (47, 2133, 67, 1, 40, 2),     // Scourge
        (50, 1024, 17, 1, 40, 0),     // Infested Terran
        (62, 853, 27, 40960, 27, 2),  // Devourer
        (103, 1024, 17, 1, 40, 0),    // Lurker
        // Protoss
        (60, 1707, 67, 40960, 40, 2), // Corsair
        (61, 1024, 17, 1, 40, 0),     // Dark Templar
        (63, 853, 27, 1, 40, 2),      // Dark Archon
        (64, 1280, 67, 30720, 40, 0), // Probe
        (65, 608, 17, 1, 40, 0),      // Zealot
        (66, 853, 27, 20480, 40, 0),  // Dragoon
        (67, 608, 17, 1, 40, 0),      // High Templar
        (68, 1024, 17, 1, 40, 2),     // Archon
        (69, 1400, 17, 40960, 40, 2), // Shuttle
        (70, 1280, 67, 40960, 40, 2), // Scout
        (71, 853, 27, 40960, 40, 2),  // Arbiter
        (72, 640, 27, 40960, 20, 2),  // Carrier
        (73, 2560, 133, 1, 40, 2),    // Interceptor
        (83, 427, 17, 1, 20, 0),      // Reaver
        (84, 1280, 17, 40960, 40, 2), // Observer
    ];

    for &(uid, speed, accel, halt, turn, mt) in entries {
        if uid < UNIT_TYPE_COUNT {
            table[uid] = FlingyType {
                top_speed: speed,
                acceleration: accel,
                halt_distance: halt,
                turn_rate: turn,
                movement_type: mt,
            };
        }
    }

    table
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
        data[U_GROUND_WEAPON] = 0; // weapon 0
        data[U_MAX_GROUND_HITS] = 1;
        data[U_AIR_WEAPON] = 130; // no air weapon
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
        assert!(parse_flingy_dat(&[0u8; 100]).is_err());
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

    fn build_techdata_dat() -> Vec<u8> {
        let mut data = vec![0u8; TECHDATA_DAT_MIN_SIZE];
        // Tech 0 (Stim): 100 minerals, 100 gas, 1200 frames
        let cost: u16 = 100;
        data[T_MINERAL_COST..T_MINERAL_COST + 2].copy_from_slice(&cost.to_le_bytes());
        data[T_GAS_COST..T_GAS_COST + 2].copy_from_slice(&cost.to_le_bytes());
        let time: u16 = 1200;
        data[T_RESEARCH_TIME..T_RESEARCH_TIME + 2].copy_from_slice(&time.to_le_bytes());
        let energy: u16 = 0;
        data[T_ENERGY_COST..T_ENERGY_COST + 2].copy_from_slice(&energy.to_le_bytes());
        data[T_RACE] = 1; // Terran
        data
    }

    fn build_upgrades_dat() -> Vec<u8> {
        let mut data = vec![0u8; UPGRADES_DAT_MIN_SIZE];
        // Upgrade 0 (Infantry Armor): base 100 min, factor 75
        let base: u16 = 100;
        data[UG_MINERAL_BASE..UG_MINERAL_BASE + 2].copy_from_slice(&base.to_le_bytes());
        let factor: u16 = 75;
        data[UG_MINERAL_FACTOR..UG_MINERAL_FACTOR + 2].copy_from_slice(&factor.to_le_bytes());
        data[UG_GAS_BASE..UG_GAS_BASE + 2].copy_from_slice(&base.to_le_bytes());
        data[UG_GAS_FACTOR..UG_GAS_FACTOR + 2].copy_from_slice(&factor.to_le_bytes());
        data[UG_MAX_REPEATS] = 3;
        data[UG_RACE] = 1; // Terran
        data
    }

    fn build_orders_dat() -> Vec<u8> {
        vec![0u8; ORDERS_DAT_MIN_SIZE]
    }

    #[test]
    fn test_parse_techdata_dat() {
        let data = build_techdata_dat();
        let types = parse_techdata_dat(&data).unwrap();
        assert_eq!(types.len(), TECH_COUNT);
        assert_eq!(types[0].mineral_cost, 100);
        assert_eq!(types[0].gas_cost, 100);
        assert_eq!(types[0].research_time, 1200);
        assert_eq!(types[0].race, 1);
    }

    #[test]
    fn test_parse_techdata_dat_too_short() {
        assert!(parse_techdata_dat(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_parse_upgrades_dat() {
        let data = build_upgrades_dat();
        let types = parse_upgrades_dat(&data).unwrap();
        assert_eq!(types.len(), UPGRADE_COUNT);
        assert_eq!(types[0].mineral_base, 100);
        assert_eq!(types[0].mineral_factor, 75);
        assert_eq!(types[0].max_repeats, 3);
    }

    #[test]
    fn test_upgrade_cost_at_level() {
        let data = build_upgrades_dat();
        let types = parse_upgrades_dat(&data).unwrap();
        // Level 1: base only
        assert_eq!(types[0].cost_at_level(1), (100, 100));
        // Level 2: base + factor
        assert_eq!(types[0].cost_at_level(2), (175, 175));
        // Level 3: base + 2*factor
        assert_eq!(types[0].cost_at_level(3), (250, 250));
    }

    #[test]
    fn test_parse_upgrades_dat_too_short() {
        assert!(parse_upgrades_dat(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_parse_orders_dat() {
        let data = build_orders_dat();
        let types = parse_orders_dat(&data).unwrap();
        assert_eq!(types.len(), ORDER_COUNT);
    }

    #[test]
    fn test_parse_orders_dat_too_short() {
        assert!(parse_orders_dat(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_game_data_all() {
        let flingy = build_flingy_dat();
        let units = build_units_dat();
        let weapons = build_weapons_dat();
        let tech = build_techdata_dat();
        let upgrades = build_upgrades_dat();
        let orders = build_orders_dat();
        let gd =
            GameData::from_dat_all(&units, &flingy, &weapons, &tech, &upgrades, &orders).unwrap();
        assert_eq!(gd.tech_type(0).unwrap().mineral_cost, 100);
        assert_eq!(gd.upgrade_type(0).unwrap().mineral_base, 100);
        assert!(gd.order_type(0).is_some());
    }
}
