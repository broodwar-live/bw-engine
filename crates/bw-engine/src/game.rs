use crate::chk_units::ChkUnit;
use crate::dat::GameData;
use crate::direction::Direction;
use crate::error::Result;
use crate::fp8::{Fp8, XY};
use crate::map::Map;
use crate::pathfind;
use crate::regions::RegionMap;
use crate::selection::SelectionState;
use crate::unit::{MoveState, UnitId, UnitState};
use crate::vision::VisionMap;

/// Maximum number of units BW supports.
pub const MAX_UNITS: usize = 1700;
/// Maximum players.
pub const MAX_PLAYERS: usize = 8;

/// Per-player resource and supply state.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub minerals: i32,
    pub gas: i32,
    pub supply_used: i32, // in half-units (Marine = 2, Zergling = 1)
    pub supply_max: i32,  // in half-units
    /// Upgrade levels indexed by upgrade_type_id.
    pub upgrade_levels: Vec<u8>,
    /// Researched tech bitset (tech_type_id < 64).
    pub researched_techs: u64,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            minerals: 0,
            gas: 0,
            supply_used: 0,
            supply_max: 0,
            upgrade_levels: vec![0; 64],
            researched_techs: 0,
        }
    }
}

impl PlayerState {
    pub fn upgrade_level(&self, upgrade_id: u8) -> u8 {
        self.upgrade_levels
            .get(upgrade_id as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn has_tech(&self, tech_id: u8) -> bool {
        if tech_id >= 64 {
            return false;
        }
        self.researched_techs & (1u64 << tech_id) != 0
    }

    fn research_tech(&mut self, tech_id: u8) {
        if tech_id < 64 {
            self.researched_techs |= 1u64 << tech_id;
        }
    }

    fn increment_upgrade(&mut self, upgrade_id: u8) {
        if let Some(level) = self.upgrade_levels.get_mut(upgrade_id as usize) {
            *level = level.saturating_add(1);
        }
    }
}

/// Commands that the engine understands.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    Select(Vec<u16>),
    SelectAdd(Vec<u16>),
    SelectRemove(Vec<u16>),
    HotkeyAssign { group: u8 },
    HotkeyRecall { group: u8 },
    Move { x: u16, y: u16 },
    Attack { target_tag: u16 },
    Stop,
    Train { unit_type: u16 },
    Build { x: u16, y: u16, unit_type: u16 },
    UnitMorph { unit_type: u16 },
    BuildingMorph { unit_type: u16 },
    Research { tech_type: u8 },
    Upgrade { upgrade_type: u8 },
}

/// The game simulation state.
pub struct Game {
    data: GameData,
    map: Map,
    region_map: RegionMap,
    current_frame: u32,
    units: Vec<Option<UnitState>>,
    selection: SelectionState,
    next_unit_index: u16,
    pub player_states: [PlayerState; MAX_PLAYERS],
    pub vision: VisionMap,
    /// Debug counter: total weapon fires.
    pub debug_fires: u32,
    pub debug_target_not_found: u32,
    pub debug_no_weapon: u32,
    pub debug_out_of_range: u32,
}

impl Game {
    pub fn new(map: Map, data: GameData) -> Self {
        let region_map = RegionMap::from_map(&map);
        let w = map.width();
        let h = map.height();
        Self {
            data,
            map,
            region_map,
            current_frame: 0,
            units: vec![None; MAX_UNITS],
            selection: SelectionState::default(),
            next_unit_index: 0,
            player_states: Default::default(),
            vision: VisionMap::new(w, h),
            debug_fires: 0,
            debug_target_not_found: 0,
            debug_no_weapon: 0,
            debug_out_of_range: 0,
        }
    }

    /// Set initial resources for a player (default: 50 minerals, 0 gas in melee).
    pub fn set_player_resources(&mut self, player_id: u8, minerals: i32, gas: i32) {
        if (player_id as usize) < MAX_PLAYERS {
            self.player_states[player_id as usize].minerals = minerals;
            self.player_states[player_id as usize].gas = gas;
        }
    }

    /// Get player state.
    pub fn player_state(&self, player_id: u8) -> Option<&PlayerState> {
        self.player_states.get(player_id as usize)
    }

    /// Load initial units from CHK UNIT section entries.
    ///
    /// Matches BW's tag assignment order by also allocating turret subunit slots.
    pub fn load_initial_units(&mut self, chk_units: &[ChkUnit]) -> Result<()> {
        for chu in chk_units {
            if self.next_unit_index as usize >= MAX_UNITS {
                break;
            }
            if chu.unit_type == 214 {
                continue; // Start Location
            }

            self.create_unit(chu.unit_type, chu.owner, chu.x as i32, chu.y as i32);
        }
        Ok(())
    }

    /// Create a new unit and return its tag, or None if at capacity.
    ///
    /// Matches BW's allocation: if the unit type has a turret subunit
    /// (unit_type < 117 and turret_unit_type < 228), a slot is also
    /// allocated for the turret.
    fn create_unit(&mut self, unit_type: u16, owner: u8, x: i32, y: i32) -> Option<u16> {
        if self.next_unit_index as usize >= MAX_UNITS {
            return None;
        }
        let index = self.next_unit_index;
        self.next_unit_index += 1;

        let flingy = self
            .data
            .flingy_for_unit(unit_type)
            .copied()
            .unwrap_or_default();

        let ut = self.data.unit_type(unit_type).copied().unwrap_or_default();

        let id = UnitId::new(index, 0);
        let shield_hp = if ut.has_shield {
            ut.shield_points as i32 * 256
        } else {
            0
        };
        let unit = UnitState {
            id,
            unit_type,
            owner,
            alive: true,
            exact_position: XY::from_pixels(x, y),
            pixel_x: x,
            pixel_y: y,
            velocity: XY::ZERO,
            heading: Direction::default(),
            current_speed: Fp8::ZERO,
            move_state: MoveState::AtRest,
            move_target: None,
            waypoints: Vec::new(),
            waypoint_index: 0,
            top_speed: flingy.top_speed,
            acceleration: flingy.acceleration,
            halt_distance: flingy.halt_distance,
            turn_rate: flingy.turn_rate,
            movement_type: flingy.movement_type,
            hp: ut.hitpoints,
            max_hp: ut.hitpoints,
            shields: shield_hp,
            max_shields: shield_hp,
            armor: ut.armor,
            unit_size: ut.unit_size,
            is_air: ut.is_air(),
            ground_weapon: ut.ground_weapon,
            air_weapon: ut.air_weapon,
            weapon_cooldown: 0,
            attack_target: None,
            build_queue: Vec::new(),
            build_timer: 0,
            is_building: ut.is_building,
            under_construction: false,
            morph_timer: 0,
            morph_target: None,
            energy: energy_for_type(unit_type),
            max_energy: max_energy_for_type(unit_type),
            is_worker: is_worker_type(unit_type),
            mining_timer: 0,
            mining_target: None,
            collision_radius: collision_radius_for_type(unit_type, ut.is_building),
        };
        self.units[index as usize] = Some(unit);

        // BW allocates a turret subunit for non-building units with a turret type.
        // This consumes the next slot, matching BW's tag assignment.
        if unit_type < crate::dat::COMMAND_CENTER_ID && ut.turret_unit_type < 228 {
            self.create_turret_slot(ut.turret_unit_type, owner, x, y);
        }

        Some(id.to_tag())
    }

    /// Allocate a slot for a turret subunit (not a full unit, just reserves the index).
    fn create_turret_slot(&mut self, turret_type: u16, owner: u8, x: i32, y: i32) {
        if self.next_unit_index as usize >= MAX_UNITS {
            return;
        }
        let index = self.next_unit_index;
        self.next_unit_index += 1;

        let flingy = self
            .data
            .flingy_for_unit(turret_type)
            .copied()
            .unwrap_or_default();
        let ut = self
            .data
            .unit_type(turret_type)
            .copied()
            .unwrap_or_default();

        let id = UnitId::new(index, 0);
        let shield_hp = if ut.has_shield {
            ut.shield_points as i32 * 256
        } else {
            0
        };
        // Turrets are alive but invisible in our sim (they track the parent).
        self.units[index as usize] = Some(UnitState {
            id,
            unit_type: turret_type,
            owner,
            alive: true,
            exact_position: XY::from_pixels(x, y),
            pixel_x: x,
            pixel_y: y,
            velocity: XY::ZERO,
            heading: Direction::default(),
            current_speed: Fp8::ZERO,
            move_state: MoveState::AtRest,
            move_target: None,
            waypoints: Vec::new(),
            waypoint_index: 0,
            top_speed: flingy.top_speed,
            acceleration: flingy.acceleration,
            halt_distance: flingy.halt_distance,
            turn_rate: flingy.turn_rate,
            movement_type: flingy.movement_type,
            hp: ut.hitpoints,
            max_hp: ut.hitpoints,
            shields: shield_hp,
            max_shields: shield_hp,
            armor: ut.armor,
            unit_size: ut.unit_size,
            is_air: ut.is_air(),
            ground_weapon: ut.ground_weapon,
            air_weapon: ut.air_weapon,
            weapon_cooldown: 0,
            attack_target: None,
            build_queue: Vec::new(),
            build_timer: 0,
            is_building: false,
            under_construction: false,
            morph_timer: 0,
            morph_target: None,
            energy: 0,
            max_energy: 0,
            is_worker: false,
            mining_timer: 0,
            mining_target: None,
            collision_radius: 8,
        });
    }

    /// Create melee starting units for each player.
    ///
    /// Matches OpenBW's `create_starting_units` order (bwgame.h:21147):
    /// 1. Resource depot at start location
    /// 2. For Zerg: 2 larvae + overlord
    /// 3. 4 workers at start location
    pub fn create_melee_starting_units(
        &mut self,
        start_locations: &[(u8, i32, i32)], // (player_id, x, y)
        player_races: &[(u8, u8)],          // (player_id, race)
    ) {
        let race_map: std::collections::HashMap<u8, u8> = player_races.iter().copied().collect();
        let map_w = self.map.width_px() as i32;
        let map_h = self.map.height_px() as i32;

        for &(player_id, x, y) in start_locations {
            let race = race_map.get(&player_id).copied().unwrap_or(1);

            // 1. Resource depot.
            let (depot_type, worker_type) = match race {
                0 => (142u16, 42u16), // Hatchery, Drone
                2 => (154, 64),       // Nexus, Probe
                _ => (117, 7),        // Command Center, SCV
            };
            self.create_unit(depot_type, player_id, x, y);

            // 2. Zerg: 2 larvae + overlord.
            if race == 0 {
                self.create_unit(35, player_id, x - 32, y + 32); // Larva
                self.create_unit(35, player_id, x + 32, y + 32); // Larva
                // Overlord placed opposite side of map center from start.
                let ox = if x >= map_w / 2 { x - 64 } else { x + 64 };
                let oy = if y >= map_h / 2 { y - 64 } else { y + 64 };
                self.create_unit(43, player_id, ox, oy); // Overlord
            }

            // 3. Four workers at start location.
            for _ in 0..4 {
                self.create_unit(worker_type, player_id, x, y);
            }
        }
    }

    /// Apply a command from a player.
    pub fn apply_command(&mut self, player_id: u8, command: &EngineCommand) {
        match command {
            EngineCommand::Select(tags) => {
                self.selection.set_selection(player_id, tags);
            }
            EngineCommand::SelectAdd(tags) => {
                self.selection.add_to_selection(player_id, tags);
            }
            EngineCommand::SelectRemove(tags) => {
                self.selection.remove_from_selection(player_id, tags);
            }
            EngineCommand::HotkeyAssign { group } => {
                self.selection.assign_hotkey(player_id, *group);
            }
            EngineCommand::HotkeyRecall { group } => {
                self.selection.recall_hotkey(player_id, *group);
            }
            EngineCommand::Move { x, y } => {
                self.issue_move(player_id, *x, *y);
            }
            EngineCommand::Attack { target_tag } => {
                self.issue_attack(player_id, *target_tag);
            }
            EngineCommand::Stop => {
                self.issue_stop(player_id);
            }
            EngineCommand::Train { unit_type } => {
                self.issue_train(player_id, *unit_type);
            }
            EngineCommand::Build { x, y, unit_type } => {
                self.issue_build(player_id, *x, *y, *unit_type);
            }
            EngineCommand::UnitMorph { unit_type } => {
                self.issue_unit_morph(player_id, *unit_type);
            }
            EngineCommand::BuildingMorph { unit_type } => {
                self.issue_building_morph(player_id, *unit_type);
            }
            EngineCommand::Research { tech_type } => {
                self.issue_research(player_id, *tech_type);
            }
            EngineCommand::Upgrade { upgrade_type } => {
                self.issue_upgrade(player_id, *upgrade_type);
            }
        }
    }

    /// Advance the simulation by one frame.
    pub fn step(&mut self) {
        self.current_frame += 1;

        // Phase 1: Movement + morph/construction timers.
        for slot in &mut self.units {
            if let Some(unit) = slot
                && unit.alive
            {
                // Skip movement for units under construction or morphing.
                if !unit.under_construction && unit.morph_timer == 0 {
                    unit.update_movement();
                }
                if unit.weapon_cooldown > 0 {
                    unit.weapon_cooldown -= 1;
                }
                // Building construction timer.
                if unit.under_construction {
                    if unit.build_timer > 0 {
                        unit.build_timer -= 1;
                    }
                    if unit.build_timer == 0 {
                        unit.under_construction = false;
                    }
                }
            }
        }

        // Phase 1b: Complete morphs (separate pass to avoid borrow issues).
        self.update_morphs();

        // Phase 2: Combat — resolve attacks.
        self.update_combat();

        // Phase 3: Production — advance build timers.
        self.update_production();

        // Phase 4: Vision — update visibility (every 4 frames for perf).
        if self.current_frame.is_multiple_of(4) {
            self.update_vision();
        }

        // Phase 5: Mining — workers generate resources.
        if self.current_frame.is_multiple_of(8) {
            self.update_mining();
        }

        // Phase 6: Energy — casters regenerate energy.
        self.update_energy();

        // Phase 7: Collision — push overlapping units apart.
        if self.current_frame.is_multiple_of(4) {
            self.update_collision();
        }
    }

    /// Step to a target frame.
    pub fn step_to(&mut self, target_frame: u32) {
        while self.current_frame < target_frame {
            self.step();
        }
    }

    pub fn current_frame(&self) -> u32 {
        self.current_frame
    }

    pub fn units(&self) -> impl Iterator<Item = &UnitState> {
        self.units
            .iter()
            .filter_map(|slot| slot.as_ref().filter(|u| u.alive))
    }

    pub fn unit_by_tag(&self, tag: u16) -> Option<&UnitState> {
        let uid = UnitId::from_tag(tag);
        self.units
            .get(uid.index() as usize)?
            .as_ref()
            .filter(|u| u.alive)
    }

    pub fn unit_count(&self) -> usize {
        self.units().count()
    }

    pub fn map(&self) -> &Map {
        &self.map
    }

    // -- Command handlers --

    fn issue_move(&mut self, player_id: u8, x: u16, y: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
            {
                unit.attack_target = None;
                unit.move_target = Some((x, y));
                unit.move_state = MoveState::Moving;

                let waypoints = pathfind::find_path(
                    &self.map,
                    &self.region_map,
                    unit.pixel_x,
                    unit.pixel_y,
                    x as i32,
                    y as i32,
                )
                .unwrap_or_else(|| vec![(x as i32, y as i32)]);

                unit.waypoints = waypoints;
                unit.waypoint_index = 0;
            }
        }
    }

    fn issue_attack(&mut self, player_id: u8, target_tag: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
            {
                unit.attack_target = Some(target_tag);
                // Movement toward target is handled in update_combat.
            }
        }
    }

    fn issue_stop(&mut self, player_id: u8) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
            {
                unit.move_target = None;
                unit.move_state = MoveState::AtRest;
                unit.velocity = XY::ZERO;
                unit.current_speed = Fp8::ZERO;
                unit.waypoints.clear();
                unit.waypoint_index = 0;
                unit.attack_target = None;
            }
        }
    }

    fn issue_train(&mut self, player_id: u8, unit_type: u16) {
        // Check resource/supply costs.
        let ut = self.data.unit_type(unit_type).copied().unwrap_or_default();
        let ps = &self.player_states[player_id as usize];
        if ps.minerals < ut.mineral_cost as i32 || ps.gas < ut.gas_cost as i32 {
            return; // Not enough resources.
        }

        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
                && unit.is_building
                && unit.build_queue.len() < 5
            {
                // Deduct resources on queue (first unit only).
                if unit.build_queue.is_empty() {
                    let ps = &mut self.player_states[player_id as usize];
                    ps.minerals -= ut.mineral_cost as i32;
                    ps.gas -= ut.gas_cost as i32;
                }
                unit.build_queue.push(unit_type);
                return; // Only queue in one building.
            }
        }
    }

    fn issue_build(&mut self, player_id: u8, x: u16, y: u16, unit_type: u16) {
        let ut = self.data.unit_type(unit_type).copied().unwrap_or_default();
        let ps = &mut self.player_states[player_id as usize];

        // Deduct resources.
        if ps.minerals < ut.mineral_cost as i32 || ps.gas < ut.gas_cost as i32 {
            return;
        }
        ps.minerals -= ut.mineral_cost as i32;
        ps.gas -= ut.gas_cost as i32;

        let px = x as i32 * 32 + 16;
        let py = y as i32 * 32 + 16;

        if let Some(tag) = self.create_unit(unit_type, player_id, px, py) {
            // Mark building as under construction with a build timer.
            let uid = UnitId::from_tag(tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if ut.build_time > 0 {
                    unit.under_construction = true;
                    unit.build_timer = ut.build_time;
                }
                // Track supply provided by buildings.
                self.add_supply(player_id, unit_type);
            }
        }
    }

    fn issue_unit_morph(&mut self, player_id: u8, unit_type: u16) {
        let ut = self.data.unit_type(unit_type).copied().unwrap_or_default();
        let ps = &mut self.player_states[player_id as usize];

        if ps.minerals < ut.mineral_cost as i32 || ps.gas < ut.gas_cost as i32 {
            return;
        }
        ps.minerals -= ut.mineral_cost as i32;
        ps.gas -= ut.gas_cost as i32;

        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        if let Some(&tag) = tags.first() {
            let uid = UnitId::from_tag(tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
            {
                if ut.build_time > 0 {
                    // Start morph timer — don't change type until complete.
                    unit.morph_timer = ut.build_time;
                    unit.morph_target = Some(unit_type);
                } else {
                    self.apply_morph_to_unit(uid.index() as usize, unit_type);
                }
            }
        }
    }

    fn issue_building_morph(&mut self, player_id: u8, unit_type: u16) {
        self.issue_unit_morph(player_id, unit_type);
    }

    fn issue_research(&mut self, player_id: u8, tech_type: u8) {
        // Deduct cost from tech data if available.
        if let Some(tech) = self.data.tech_type(tech_type) {
            let ps = &mut self.player_states[player_id as usize];
            if ps.minerals < tech.mineral_cost as i32 || ps.gas < tech.gas_cost as i32 {
                return;
            }
            ps.minerals -= tech.mineral_cost as i32;
            ps.gas -= tech.gas_cost as i32;
        }
        // Mark tech as researched immediately (simplified — no research timer).
        self.player_states[player_id as usize].research_tech(tech_type);
    }

    fn issue_upgrade(&mut self, player_id: u8, upgrade_type: u8) {
        if let Some(upg) = self.data.upgrade_type(upgrade_type) {
            let ps = &mut self.player_states[player_id as usize];
            let next_level = ps.upgrade_level(upgrade_type) + 1;
            let (min_cost, gas_cost) = upg.cost_at_level(next_level);
            if ps.minerals < min_cost as i32 || ps.gas < gas_cost as i32 {
                return;
            }
            ps.minerals -= min_cost as i32;
            ps.gas -= gas_cost as i32;
        }
        self.player_states[player_id as usize].increment_upgrade(upgrade_type);
    }

    /// Apply a completed morph: change unit type and stats.
    fn apply_morph_to_unit(&mut self, unit_index: usize, unit_type: u16) {
        let flingy = self
            .data
            .flingy_for_unit(unit_type)
            .copied()
            .unwrap_or_default();
        let ut = self.data.unit_type(unit_type).copied().unwrap_or_default();

        if let Some(Some(unit)) = self.units.get_mut(unit_index) {
            unit.unit_type = unit_type;
            unit.top_speed = flingy.top_speed;
            unit.acceleration = flingy.acceleration;
            unit.turn_rate = flingy.turn_rate;
            unit.movement_type = flingy.movement_type;
            unit.hp = ut.hitpoints;
            unit.max_hp = ut.hitpoints;
            unit.armor = ut.armor;
            unit.unit_size = ut.unit_size;
            unit.is_air = ut.is_air();
            unit.ground_weapon = ut.ground_weapon;
            unit.air_weapon = ut.air_weapon;
            let shield_hp = if ut.has_shield {
                ut.shield_points as i32 * 256
            } else {
                0
            };
            unit.shields = shield_hp;
            unit.max_shields = shield_hp;
            unit.morph_timer = 0;
            unit.morph_target = None;
            unit.energy = energy_for_type(unit_type);
            unit.max_energy = max_energy_for_type(unit_type);
            unit.is_worker = is_worker_type(unit_type);
            unit.collision_radius = collision_radius_for_type(unit_type, ut.is_building);
        }
    }

    /// Track supply provided by a unit type, using units.dat data.
    fn add_supply(&mut self, player_id: u8, unit_type: u16) {
        let provided = self
            .data
            .unit_type(unit_type)
            .map(|ut| ut.supply_provided as i32)
            .unwrap_or(0);
        if provided > 0 && (player_id as usize) < MAX_PLAYERS {
            self.player_states[player_id as usize].supply_max += provided;
        }
    }

    // -- Simulation phases --

    fn update_combat(&mut self) {
        // Collect attack actions: (attacker_index, target_tag).
        let mut attacks: Vec<(usize, u16)> = Vec::new();

        for (i, slot) in self.units.iter().enumerate() {
            let Some(unit) = slot else { continue };
            if !unit.alive || unit.owner >= 8 || unit.under_construction || unit.morph_timer > 0 {
                continue;
            }

            if let Some(target_tag) = unit.attack_target {
                let target_uid = UnitId::from_tag(target_tag);
                let target_owner = self
                    .units
                    .get(target_uid.index() as usize)
                    .and_then(|s| s.as_ref())
                    .map(|u| u.owner);
                if target_owner != Some(unit.owner) {
                    attacks.push((i, target_tag));
                    continue;
                }
            }

            // Auto-attack: find nearest enemy within acquisition range.
            // Check both ground and air weapons.
            let has_ground = unit.ground_weapon < 130;
            let has_air = unit.air_weapon < 130;
            if !has_ground && !has_air {
                continue;
            }

            let acq_range = self
                .data
                .unit_type(unit.unit_type)
                .map(|ut| ut.sight_range as i32 * 32)
                .unwrap_or(0);
            if acq_range == 0 {
                continue;
            }

            let mut best_dist = i64::MAX;
            let mut best_tag: Option<u16> = None;
            for other_slot in self.units.iter() {
                let Some(other) = other_slot else { continue };
                if !other.alive || other.owner == unit.owner || other.owner >= 8 {
                    continue;
                }
                // Check if we have a weapon that can hit this target.
                if other.is_air && !has_air {
                    continue;
                }
                if !other.is_air && !has_ground {
                    continue;
                }
                let dx = (unit.pixel_x - other.pixel_x) as i64;
                let dy = (unit.pixel_y - other.pixel_y) as i64;
                let dist = dx * dx + dy * dy;
                let acq = acq_range as i64;
                if dist < acq * acq && dist < best_dist {
                    best_dist = dist;
                    best_tag = Some(other.id.to_tag());
                }
            }
            if let Some(tag) = best_tag {
                attacks.push((i, tag));
            }
        }

        for (attacker_idx, target_tag) in attacks {
            let target_uid = UnitId::from_tag(target_tag);
            let ti = target_uid.index() as usize;

            let target_info = self
                .units
                .get(ti)
                .and_then(|s| s.as_ref().filter(|u| u.alive))
                .map(|u| (u.pixel_x, u.pixel_y, u.is_air, u.unit_size, u.armor));

            let Some((tx, ty, target_is_air, target_size, target_armor)) = target_info else {
                self.debug_target_not_found += 1;
                if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                    unit.attack_target = None;
                }
                continue;
            };

            // Select the right weapon based on target type (air vs ground).
            let attacker = self.units[attacker_idx].as_ref().unwrap();
            let weapon_id = if target_is_air {
                attacker.air_weapon
            } else {
                attacker.ground_weapon
            };
            let Some(weapon) = self.data.weapon_type(weapon_id) else {
                self.debug_no_weapon += 1;
                continue;
            };

            let dx = (attacker.pixel_x - tx).abs();
            let dy = (attacker.pixel_y - ty).abs();
            let dist_sq = (dx as u64) * (dx as u64) + (dy as u64) * (dy as u64);
            let range = weapon.max_range as u64;
            let range_sq = range * range;

            if dist_sq <= range_sq && attacker.weapon_cooldown == 0 {
                self.debug_fires += 1;

                // Base damage = amount * factor + upgrade bonus.
                let attacker_owner = attacker.owner;
                let upgrade_bonus = weapon.damage_bonus as i32
                    * self.player_states[attacker_owner as usize]
                        .upgrade_level(weapon.damage_upgrade) as i32;
                let base_damage = weapon.damage_amount as i32 * weapon.damage_factor.max(1) as i32
                    + upgrade_bonus;

                // Apply damage type modifier vs unit size.
                let (num, den) = weapon.damage_type.size_modifier(target_size);
                let modified_damage = base_damage * num as i32 / den as i32;

                // Subtract armor (with upgrade bonus).
                let armor_upgrade_id = self
                    .data
                    .unit_type(self.units[ti].as_ref().map(|u| u.unit_type).unwrap_or(0))
                    .map(|ut| ut.armor_upgrade)
                    .unwrap_or(0);
                let armor_total = target_armor as i32
                    + self.units[ti]
                        .as_ref()
                        .map(|u| {
                            self.player_states[u.owner as usize].upgrade_level(armor_upgrade_id)
                                as i32
                        })
                        .unwrap_or(0);
                let effective_damage = if weapon.damage_type == crate::dat::DamageType::IgnoreArmor
                {
                    modified_damage.max(1)
                } else {
                    (modified_damage - armor_total).max(1)
                };

                let damage_fp8 = effective_damage * 256;

                // Apply damage: shields first (shields ignore damage type), then HP.
                if let Some(Some(target)) = self.units.get_mut(ti) {
                    if target.shields > 0 {
                        // Shields absorb damage first (at full damage, ignoring armor/type).
                        let shield_damage = damage_fp8.min(target.shields);
                        target.shields -= shield_damage;
                        let remaining = damage_fp8 - shield_damage;
                        if remaining > 0 {
                            target.hp -= remaining;
                        }
                    } else {
                        target.hp -= damage_fp8;
                    }
                    if target.hp <= 0 {
                        target.alive = false;
                    }
                }

                // Reset cooldown.
                if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                    unit.weapon_cooldown = weapon.cooldown as u16;
                }
            } else if dist_sq > range_sq {
                self.debug_out_of_range += 1;
                let attacker = self.units[attacker_idx].as_mut().unwrap();
                if attacker.move_state != MoveState::Moving
                    || attacker.move_target != Some((tx as u16, ty as u16))
                {
                    attacker.move_target = Some((tx as u16, ty as u16));
                    attacker.move_state = MoveState::Moving;
                    let waypoints = pathfind::find_path(
                        &self.map,
                        &self.region_map,
                        attacker.pixel_x,
                        attacker.pixel_y,
                        tx,
                        ty,
                    )
                    .unwrap_or_else(|| vec![(tx, ty)]);
                    attacker.waypoints = waypoints;
                    attacker.waypoint_index = 0;
                }
            }
        }
    }

    /// Process morph timers — complete morphs when timer reaches zero.
    fn update_morphs(&mut self) {
        let mut completed: Vec<(usize, u16)> = Vec::new();
        for (i, slot) in self.units.iter_mut().enumerate() {
            if let Some(unit) = slot
                && unit.alive
                && unit.morph_timer > 0
            {
                unit.morph_timer -= 1;
                if unit.morph_timer == 0
                    && let Some(target) = unit.morph_target
                {
                    completed.push((i, target));
                }
            }
        }
        for (idx, unit_type) in completed {
            self.apply_morph_to_unit(idx, unit_type);
        }
    }

    fn update_production(&mut self) {
        // Collect units to spawn: (unit_type, owner, x, y).
        let mut spawns: Vec<(u16, u8, i32, i32)> = Vec::new();

        for slot in &mut self.units {
            let Some(unit) = slot else { continue };
            if !unit.alive || unit.build_queue.is_empty() {
                continue;
            }

            if unit.build_timer == 0 {
                // Start building the first item in queue.
                let training_type = unit.build_queue[0];
                let build_time = self
                    .data
                    .unit_type(training_type)
                    .map(|ut| ut.build_time)
                    .unwrap_or(1);
                unit.build_timer = build_time.max(1);
            } else {
                unit.build_timer -= 1;
                if unit.build_timer == 0 {
                    // Training complete — spawn unit.
                    let trained_type = unit.build_queue.remove(0);
                    // Spawn near the building (offset by 32px).
                    let spawn_x = unit.pixel_x + 32;
                    let spawn_y = unit.pixel_y + 32;
                    spawns.push((trained_type, unit.owner, spawn_x, spawn_y));
                }
            }
        }

        for (unit_type, owner, x, y) in spawns {
            self.create_unit(unit_type, owner, x, y);
        }
    }

    fn update_vision(&mut self) {
        self.vision.clear_visible();
        for slot in &self.units {
            if let Some(unit) = slot
                && unit.alive
                && unit.owner < 8
            {
                let sight = self
                    .data
                    .unit_type(unit.unit_type)
                    .map(|ut| ut.sight_range)
                    .unwrap_or(7);
                self.vision
                    .reveal(unit.pixel_x, unit.pixel_y, sight, unit.owner);
            }
        }
    }

    /// Get the visibility grid for a player.
    /// 0 = fog, 1 = explored, 2 = visible.
    pub fn visibility_grid(&self, player: u8) -> Vec<u8> {
        self.vision.visibility_grid(player)
    }

    // -- Mining --

    fn update_mining(&mut self) {
        // Simplified mining: workers near mineral patches generate income every ~75 frames
        // (~3 seconds at fastest speed, roughly one mining trip).
        for slot in &mut self.units {
            if let Some(unit) = slot
                && unit.alive
                && unit.is_worker
                && unit.owner < 8
                && unit.move_state == MoveState::AtRest
                && unit.attack_target.is_none()
            {
                if unit.mining_timer > 0 {
                    unit.mining_timer -= 1;
                    if unit.mining_timer == 0 {
                        // Deliver resources.
                        let minerals = if unit.mining_target.is_some() { 0 } else { 8 };
                        let gas = if unit.mining_target.is_some() { 8 } else { 0 };
                        self.player_states[unit.owner as usize].minerals += minerals;
                        self.player_states[unit.owner as usize].gas += gas;
                        unit.mining_timer = 75; // Start next trip.
                    }
                } else {
                    // Start mining if idle worker.
                    unit.mining_timer = 75;
                }
            }
        }
    }

    // -- Energy --

    fn update_energy(&mut self) {
        // Casters regenerate energy: +8 energy per 256 frames (fp8 units).
        // That's roughly 1 energy per 32 frames ≈ 0.75 energy/sec at fastest.
        for slot in &mut self.units {
            if let Some(unit) = slot
                && unit.alive
                && unit.max_energy > 0
                && unit.energy < unit.max_energy
            {
                unit.energy = (unit.energy + 8).min(unit.max_energy);
            }
        }
    }

    // -- Collision --

    fn update_collision(&mut self) {
        // Simple separation: push overlapping units apart.
        // Only check ground units (air units don't collide).
        // Run every 4 frames for performance.
        let count = self.units.len();
        for i in 0..count {
            let (ax, ay, ar, a_alive, a_air, a_building) = {
                match &self.units[i] {
                    Some(u) if u.alive && !u.is_air && !u.is_building => (
                        u.pixel_x,
                        u.pixel_y,
                        u.collision_radius as i32,
                        true,
                        u.is_air,
                        u.is_building,
                    ),
                    _ => continue,
                }
            };
            if !a_alive || a_air || a_building {
                continue;
            }

            for j in (i + 1)..count {
                let (bx, by, br, b_alive, b_air, b_building) = {
                    match &self.units[j] {
                        Some(u) if u.alive && !u.is_air => (
                            u.pixel_x,
                            u.pixel_y,
                            u.collision_radius as i32,
                            true,
                            u.is_air,
                            u.is_building,
                        ),
                        _ => continue,
                    }
                };
                if !b_alive || b_air {
                    continue;
                }

                let dx = ax - bx;
                let dy = ay - by;
                let min_dist = ar + br;
                let dist_sq = dx as i64 * dx as i64 + dy as i64 * dy as i64;
                let min_dist_sq = min_dist as i64 * min_dist as i64;

                if dist_sq < min_dist_sq && dist_sq > 0 {
                    // Push apart. Buildings don't move.
                    let dist = (dist_sq as f64).sqrt() as i32;
                    let overlap = min_dist - dist;
                    let push = (overlap / 2).max(1);

                    if dist > 0 {
                        let nx = dx * push / dist;
                        let ny = dy * push / dist;

                        if !b_building {
                            if let Some(Some(ua)) = self.units.get_mut(i) {
                                ua.pixel_x += nx;
                                ua.pixel_y += ny;
                                ua.exact_position =
                                    crate::fp8::XY::from_pixels(ua.pixel_x, ua.pixel_y);
                            }
                            if let Some(Some(ub)) = self.units.get_mut(j) {
                                ub.pixel_x -= nx;
                                ub.pixel_y -= ny;
                                ub.exact_position =
                                    crate::fp8::XY::from_pixels(ub.pixel_x, ub.pixel_y);
                            }
                        } else {
                            // Only push unit A away from building B.
                            if let Some(Some(ua)) = self.units.get_mut(i) {
                                ua.pixel_x += nx * 2;
                                ua.pixel_y += ny * 2;
                                ua.exact_position =
                                    crate::fp8::XY::from_pixels(ua.pixel_x, ua.pixel_y);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit type classification helpers
// ---------------------------------------------------------------------------

fn is_worker_type(unit_type: u16) -> bool {
    matches!(unit_type, 7 | 41 | 64) // SCV, Drone, Probe
}

fn energy_for_type(unit_type: u16) -> i32 {
    // Starting energy: 50 for most casters (50 * 256 = 12800 in fp8).
    match unit_type {
        9 => 50 * 256,  // Science Vessel
        45 => 50 * 256, // Queen
        46 => 50 * 256, // Defiler
        67 => 50 * 256, // High Templar
        63 => 50 * 256, // Dark Archon
        34 => 50 * 256, // Medic
        71 => 50 * 256, // Arbiter
        _ => 0,
    }
}

fn max_energy_for_type(unit_type: u16) -> i32 {
    // Max energy: 200 for most casters (200 * 256 = 51200 in fp8).
    match unit_type {
        9 | 45 | 46 | 67 | 63 | 34 | 71 => 200 * 256,
        _ => 0,
    }
}

fn collision_radius_for_type(unit_type: u16, is_building: bool) -> u8 {
    if is_building {
        return 16; // Buildings have larger collision.
    }
    match unit_type {
        39 => 16, // Ultralisk — large
        12 => 16, // Battlecruiser
        72 => 16, // Carrier
        _ => 8,   // Default: 8px radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chk_units::ChkUnit;
    use crate::dat::{FlingyType, GameData, UnitType, WeaponType};
    use crate::tile::MiniTile;
    use crate::tileset::{CV5_ENTRY_SIZE, VF4_ENTRY_SIZE};

    fn test_game_data() -> GameData {
        let marine_flingy = FlingyType {
            top_speed: 4 * 256,
            acceleration: 256,
            halt_distance: 0,
            turn_rate: 40,
            movement_type: 0,
        };
        let mut flingy_types = vec![FlingyType::default(); 209];
        flingy_types[0] = marine_flingy;

        let marine_ut = UnitType {
            flingy_id: 0,
            turret_unit_type: 228,
            hitpoints: 40 * 256,
            shield_points: 0,
            has_shield: false,
            ground_weapon: 0,
            max_ground_hits: 1,
            air_weapon: 130,
            max_air_hits: 0,
            armor: 0,
            armor_upgrade: 0,
            unit_size: crate::dat::UnitSize::Small,
            elevation: 0,
            sight_range: 7,
            build_time: 30,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 2,
            supply_provided: 0,
            is_building: false,
        };
        let barracks_ut = UnitType {
            flingy_id: 0,
            turret_unit_type: 228,
            hitpoints: 1000 * 256,
            shield_points: 0,
            has_shield: false,
            ground_weapon: 130,
            max_ground_hits: 0,
            air_weapon: 130,
            max_air_hits: 0,
            armor: 1,
            armor_upgrade: 0,
            unit_size: crate::dat::UnitSize::Large,
            elevation: 0,
            sight_range: 7,
            build_time: 0,
            mineral_cost: 150,
            gas_cost: 0,
            supply_cost: 0,
            supply_provided: 0,
            is_building: true,
        };
        let mut unit_types = vec![UnitType::default(); 228];
        unit_types[0] = marine_ut; // Marine = unit type 0
        unit_types[122] = barracks_ut; // Barracks = unit type 122

        let marine_weapon = WeaponType {
            damage_amount: 6,
            damage_bonus: 0,
            cooldown: 15,
            damage_factor: 1,
            damage_type: crate::dat::DamageType::Normal,
            damage_upgrade: 7, // Infantry Weapons
            max_range: 128,    // 4 tiles
        };
        let mut weapon_types = vec![WeaponType::default(); 130];
        weapon_types[0] = marine_weapon;

        GameData {
            flingy_types,
            unit_types,
            weapon_types,
            tech_types: Vec::new(),
            upgrade_types: Vec::new(),
            order_types: Vec::new(),
            fallback_flingy: Vec::new(),
        }
    }

    fn test_map() -> Map {
        let walkable = [MiniTile::WALKABLE; 16];
        let mut vf4 = vec![0u8; VF4_ENTRY_SIZE];
        for (j, &f) in walkable.iter().enumerate() {
            vf4[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
        }
        let mut cv5 = vec![0u8; CV5_ENTRY_SIZE];
        cv5[20..22].copy_from_slice(&0u16.to_le_bytes());

        let mut chk = Vec::new();
        chk.extend_from_slice(b"DIM ");
        chk.extend_from_slice(&4u32.to_le_bytes());
        chk.extend_from_slice(&8u16.to_le_bytes());
        chk.extend_from_slice(&8u16.to_le_bytes());
        chk.extend_from_slice(b"ERA ");
        chk.extend_from_slice(&2u32.to_le_bytes());
        chk.extend_from_slice(&0u16.to_le_bytes());
        let mtxm: Vec<u8> = vec![0u8; 8 * 8 * 2];
        chk.extend_from_slice(b"MTXM");
        chk.extend_from_slice(&(mtxm.len() as u32).to_le_bytes());
        chk.extend_from_slice(&mtxm);

        Map::from_chk(&chk, &cv5, &vf4).unwrap()
    }

    fn make_chk_unit(id: u32, x: u16, y: u16, unit_type: u16, owner: u8) -> ChkUnit {
        ChkUnit {
            instance_id: id,
            x,
            y,
            unit_type,
            owner,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        }
    }

    #[test]
    fn test_load_initial_units() {
        let mut game = Game::new(test_map(), test_game_data());
        let units = vec![
            make_chk_unit(0, 50, 50, 0, 0),
            make_chk_unit(1, 80, 80, 0, 1),
        ];
        game.load_initial_units(&units).unwrap();
        assert_eq!(game.unit_count(), 2);
        let u0 = game.unit_by_tag(0).unwrap();
        assert_eq!(u0.pixel_x, 50);
        assert_eq!(u0.hp, 40 * 256);
    }

    #[test]
    fn test_select_and_move() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 100, y: 50 });
        for _ in 0..50 {
            game.step();
        }
        assert!(game.unit_by_tag(0).unwrap().pixel_x > 50);
    }

    #[test]
    fn test_select_and_stop() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 200, y: 50 });
        for _ in 0..10 {
            game.step();
        }
        let x = game.unit_by_tag(0).unwrap().pixel_x;
        game.apply_command(0, &EngineCommand::Stop);
        for _ in 0..10 {
            game.step();
        }
        assert_eq!(game.unit_by_tag(0).unwrap().pixel_x, x);
    }

    #[test]
    fn test_unit_arrives() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 70, y: 50 });
        for _ in 0..200 {
            game.step();
        }
        let u = game.unit_by_tag(0).unwrap();
        assert_eq!(u.move_state, MoveState::Arrived);
        assert_eq!(u.pixel_x, 70);
    }

    #[test]
    fn test_step_to() {
        let mut game = Game::new(test_map(), test_game_data());
        game.step_to(100);
        assert_eq!(game.current_frame(), 100);
    }

    // -- Combat tests --

    #[test]
    fn test_attack_kills_target() {
        let mut game = Game::new(test_map(), test_game_data());
        // Two marines: attacker has extra HP to survive mutual auto-attack.
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0),  // attacker
            make_chk_unit(1, 100, 50, 0, 1), // target
        ])
        .unwrap();
        // Give attacker extra HP to survive the mutual combat.
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.hp = 200 * 256;
            u.max_hp = 200 * 256;
        }

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Attack { target_tag: 1 });

        for _ in 0..200 {
            game.step();
            if game.unit_by_tag(1).is_none() {
                break;
            }
        }

        assert!(game.unit_by_tag(1).is_none(), "target should be dead");
        assert!(game.unit_by_tag(0).is_some(), "attacker should be alive");
    }

    #[test]
    fn test_attack_clears_on_target_death() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0),
            make_chk_unit(1, 100, 50, 0, 1),
        ])
        .unwrap();
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.hp = 200 * 256;
            u.max_hp = 200 * 256;
        }

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Attack { target_tag: 1 });

        for _ in 0..300 {
            game.step();
        }

        // After target dies, attacker's attack_target should be cleared.
        let attacker = game.unit_by_tag(0).unwrap();
        assert!(attacker.attack_target.is_none());
    }

    #[test]
    fn test_attack_moves_into_range() {
        let mut game = Game::new(test_map(), test_game_data());
        // Two marines far apart (200px > 128px range).
        game.load_initial_units(&[
            make_chk_unit(0, 20, 50, 0, 0),
            make_chk_unit(1, 200, 50, 0, 1),
        ])
        .unwrap();

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Attack { target_tag: 1 });

        // Step a few frames — attacker should start moving toward target.
        for _ in 0..20 {
            game.step();
        }

        let attacker = game.unit_by_tag(0).unwrap();
        assert!(attacker.pixel_x > 20, "attacker should move toward target");
    }

    // -- Production tests --

    #[test]
    fn test_train_produces_unit() {
        let mut game = Game::new(test_map(), test_game_data());
        // A barracks (unit type 122) owned by player 0.
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)])
            .unwrap();
        game.set_player_resources(0, 500, 500);
        let initial_count = game.unit_count();

        // Select barracks, train a marine.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Train { unit_type: 0 });

        // Build time is 30 frames in test data.
        for _ in 0..50 {
            game.step();
        }

        assert_eq!(
            game.unit_count(),
            initial_count + 1,
            "should have spawned a marine"
        );
    }

    #[test]
    fn test_train_queue() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)])
            .unwrap();
        game.set_player_resources(0, 500, 500);

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Train { unit_type: 0 });
        game.apply_command(0, &EngineCommand::Train { unit_type: 0 });

        // Step enough for both to train (30 + 30 = 60 frames + startup).
        for _ in 0..80 {
            game.step();
        }

        // Should have barracks + 2 marines = 3 units.
        assert_eq!(game.unit_count(), 3);
    }

    #[test]
    fn test_train_insufficient_resources() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)])
            .unwrap();
        // No resources set — training should be rejected.

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Train { unit_type: 0 });

        for _ in 0..50 {
            game.step();
        }

        // No marine should have been produced.
        assert_eq!(game.unit_count(), 1);
    }

    #[test]
    fn test_shields_absorb_damage() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0),  // attacker
            make_chk_unit(1, 100, 50, 0, 1), // target
        ])
        .unwrap();

        // Give target shields.
        if let Some(Some(u)) = game.units.get_mut(1) {
            u.shields = 20 * 256;
            u.max_shields = 20 * 256;
            u.hp = 200 * 256; // lots of HP so it survives
            u.max_hp = 200 * 256;
        }
        // Give attacker lots of HP.
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.hp = 200 * 256;
            u.max_hp = 200 * 256;
        }

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Attack { target_tag: 1 });

        // Step a few times to land some hits.
        for _ in 0..50 {
            game.step();
        }

        let target = game.unit_by_tag(1).unwrap();
        // Shields should have been reduced.
        assert!(target.shields < 20 * 256, "shields should be damaged");
    }
}
