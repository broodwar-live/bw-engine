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
    HotkeyAssign {
        group: u8,
    },
    HotkeyRecall {
        group: u8,
    },
    Move {
        x: u16,
        y: u16,
    },
    Attack {
        target_tag: u16,
    },
    Stop,
    Train {
        unit_type: u16,
    },
    Build {
        x: u16,
        y: u16,
        unit_type: u16,
    },
    UnitMorph {
        unit_type: u16,
    },
    BuildingMorph {
        unit_type: u16,
    },
    Research {
        tech_type: u8,
    },
    Upgrade {
        upgrade_type: u8,
    },
    /// Cast a spell at a target position. tech_type identifies the spell.
    CastSpell {
        tech_type: u8,
        x: u16,
        y: u16,
    },
    /// Set rally point for selected buildings.
    SetRally {
        x: u16,
        y: u16,
    },
    /// Load selected units into a transport.
    Load {
        transport_tag: u16,
    },
    /// Unload all units from a transport.
    UnloadAll,
    /// Stim Pack (Marines/Firebats): +50% speed and fire rate, costs 10 HP.
    Stim,
    /// Burrow selected units.
    Burrow,
    /// Unburrow selected units.
    Unburrow,
    /// Cloak selected units.
    Cloak,
    /// Decloak selected units.
    Decloak,
}

/// Spatial hash grid for efficient collision detection.
struct SpatialGrid {
    cell_size: i32,
    width: i32,
    cells: Vec<Vec<usize>>,
}

impl SpatialGrid {
    fn new(map_width_px: i32, map_height_px: i32, cell_size: i32) -> Self {
        let width = (map_width_px / cell_size) + 1;
        let height = (map_height_px / cell_size) + 1;
        Self {
            cell_size,
            width,
            cells: vec![Vec::new(); (width * height) as usize],
        }
    }

    fn insert(&mut self, index: usize, x: i32, y: i32) {
        let cx = (x / self.cell_size).max(0);
        let cy = (y / self.cell_size).max(0);
        let ci = (cy * self.width + cx) as usize;
        if ci < self.cells.len() {
            self.cells[ci].push(index);
        }
    }

    fn query_neighbors(&self, x: i32, y: i32) -> impl Iterator<Item = usize> + '_ {
        let cx = (x / self.cell_size).max(0);
        let cy = (y / self.cell_size).max(0);
        (-1..=1i32)
            .flat_map(move |dy| (-1..=1i32).map(move |dx| (cx + dx, cy + dy)))
            .filter(move |&(nx, ny)| nx >= 0 && ny >= 0)
            .filter_map(move |(nx, ny)| {
                let ci = (ny * self.width + nx) as usize;
                self.cells.get(ci)
            })
            .flat_map(|cell| cell.iter().copied())
    }
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
            rally_x: x,
            rally_y: y,
            cargo: Vec::new(),
            loaded_in: None,
            irradiate_timer: 0,
            plague_timer: 0,
            ensnare_timer: 0,
            stim_timer: 0,
            burrowed: false,
            cloaked: false,
            base_top_speed: flingy.top_speed,
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
            rally_x: x,
            rally_y: y,
            cargo: Vec::new(),
            loaded_in: None,
            irradiate_timer: 0,
            plague_timer: 0,
            ensnare_timer: 0,
            stim_timer: 0,
            burrowed: false,
            cloaked: false,
            base_top_speed: flingy.top_speed,
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
            EngineCommand::CastSpell { tech_type, x, y } => {
                self.issue_spell(player_id, *tech_type, *x as i32, *y as i32);
            }
            EngineCommand::SetRally { x, y } => {
                self.issue_set_rally(player_id, *x as i32, *y as i32);
            }
            EngineCommand::Load { transport_tag } => {
                self.issue_load(player_id, *transport_tag);
            }
            EngineCommand::UnloadAll => {
                self.issue_unload_all(player_id);
            }
            EngineCommand::Stim => {
                self.issue_stim(player_id);
            }
            EngineCommand::Burrow => {
                self.issue_burrow(player_id, true);
            }
            EngineCommand::Unburrow => {
                self.issue_burrow(player_id, false);
            }
            EngineCommand::Cloak => {
                self.issue_cloak(player_id, true);
            }
            EngineCommand::Decloak => {
                self.issue_cloak(player_id, false);
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

        // Phase 7: Status effects (Irradiate, Plague, Ensnare, Stim).
        self.update_status_effects();

        // Phase 8: Shield regeneration (Protoss).
        if self.current_frame.is_multiple_of(8) {
            self.update_shield_regen();
        }

        // Phase 8: Collision — push overlapping units apart (spatial hash).
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
            unit.base_top_speed = flingy.top_speed;
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

    // -- Spell casting --

    fn issue_spell(&mut self, player_id: u8, tech_type: u8, x: i32, y: i32) {
        let energy_cost = spell_energy_cost(tech_type);
        if energy_cost == 0 {
            return;
        }

        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        let caster_idx = tags.iter().find_map(|tag| {
            let uid = UnitId::from_tag(*tag);
            let idx = uid.index() as usize;
            self.units
                .get(idx)?
                .as_ref()
                .filter(|u| u.alive && u.energy >= energy_cost)?;
            Some(idx)
        });

        let Some(idx) = caster_idx else { return };

        if let Some(Some(unit)) = self.units.get_mut(idx) {
            unit.energy -= energy_cost;
        }

        match tech_type {
            19 => {
                // Psionic Storm: 112 damage over 48px radius.
                self.apply_area_damage(x, y, 48, 112 * 256, player_id, true);
            }
            2 => {
                // EMP: drain all shields + energy in 64px radius.
                let radius_sq = 64i64 * 64;
                for slot in &mut self.units {
                    if let Some(unit) = slot
                        && unit.alive
                    {
                        let dx = (unit.pixel_x - x) as i64;
                        let dy = (unit.pixel_y - y) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            unit.shields = 0;
                            unit.energy = 0;
                        }
                    }
                }
            }
            7 => {
                // Irradiate: apply DoT to nearest enemy unit at target location.
                // ~250 damage over ~750 frames (37.5 seconds).
                let radius_sq = 32i64 * 32;
                for slot in &mut self.units {
                    if let Some(unit) = slot
                        && unit.alive
                        && unit.owner != player_id
                        && unit.irradiate_timer == 0
                    {
                        let dx = (unit.pixel_x - x) as i64;
                        let dy = (unit.pixel_y - y) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            unit.irradiate_timer = 750;
                            break; // Single target.
                        }
                    }
                }
            }
            15 => {
                // Plague: DoT in 64px radius, ~300 damage over ~600 frames. Can't kill.
                let radius_sq = 64i64 * 64;
                for slot in &mut self.units {
                    if let Some(unit) = slot
                        && unit.alive
                        && unit.owner != player_id
                    {
                        let dx = (unit.pixel_x - x) as i64;
                        let dy = (unit.pixel_y - y) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            unit.plague_timer = 600;
                        }
                    }
                }
            }
            17 => {
                // Ensnare: slow units in 64px radius for ~450 frames.
                let radius_sq = 64i64 * 64;
                for slot in &mut self.units {
                    if let Some(unit) = slot
                        && unit.alive
                        && unit.ensnare_timer == 0
                    {
                        let dx = (unit.pixel_x - x) as i64;
                        let dy = (unit.pixel_y - y) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            unit.ensnare_timer = 450;
                            unit.top_speed = unit.base_top_speed / 2;
                        }
                    }
                }
            }
            _ => {
                // Generic area damage for other spells.
                let (damage, radius, affects_shields) = spell_effect(tech_type);
                if damage > 0 && radius > 0 {
                    self.apply_area_damage(x, y, radius, damage, player_id, affects_shields);
                }
            }
        }
    }

    fn apply_area_damage(
        &mut self,
        x: i32,
        y: i32,
        radius: i32,
        damage_fp8: i32,
        caster_owner: u8,
        affects_shields: bool,
    ) {
        let radius_sq = radius as i64 * radius as i64;
        for slot in &mut self.units {
            if let Some(unit) = slot
                && unit.alive
                && unit.owner != caster_owner
            {
                let dx = (unit.pixel_x - x) as i64;
                let dy = (unit.pixel_y - y) as i64;
                if dx * dx + dy * dy <= radius_sq {
                    if affects_shields && unit.shields > 0 {
                        let shield_dmg = damage_fp8.min(unit.shields);
                        unit.shields -= shield_dmg;
                        let remaining = damage_fp8 - shield_dmg;
                        if remaining > 0 {
                            unit.hp -= remaining;
                        }
                    } else {
                        unit.hp -= damage_fp8;
                    }
                    if unit.hp <= 0 {
                        unit.alive = false;
                    }
                }
            }
        }
    }

    // -- Rally points --

    fn issue_set_rally(&mut self, player_id: u8, x: i32, y: i32) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
                && unit.is_building
            {
                unit.rally_x = x;
                unit.rally_y = y;
            }
        }
    }

    // -- Transport --

    fn issue_load(&mut self, player_id: u8, transport_tag: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        let transport_uid = UnitId::from_tag(transport_tag);
        let ti = transport_uid.index() as usize;

        // Check transport is valid and has cargo space (max 8).
        let can_load = self
            .units
            .get(ti)
            .and_then(|s| s.as_ref())
            .is_some_and(|u| {
                u.alive
                    && u.owner == player_id
                    && u.cargo.len() < 8
                    && is_transport_type(u.unit_type)
            });

        if !can_load {
            return;
        }

        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            let idx = uid.index() as usize;
            if idx == ti {
                continue;
            }
            let can_board = self
                .units
                .get(idx)
                .and_then(|s| s.as_ref())
                .is_some_and(|u| u.alive && !u.is_building && !u.is_air && u.loaded_in.is_none());
            if !can_board {
                continue;
            }

            // Check cargo space again.
            let space = self.units[ti].as_ref().map(|u| u.cargo.len()).unwrap_or(8);
            if space >= 8 {
                break;
            }

            if let Some(Some(transport)) = self.units.get_mut(ti) {
                transport.cargo.push(*tag);
            }
            if let Some(Some(unit)) = self.units.get_mut(idx) {
                unit.loaded_in = Some(transport_tag);
                unit.move_state = MoveState::AtRest;
                unit.velocity = crate::fp8::XY::ZERO;
            }
        }
    }

    fn issue_unload_all(&mut self, player_id: u8) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            let ti = uid.index() as usize;

            let cargo = self
                .units
                .get(ti)
                .and_then(|s| s.as_ref())
                .filter(|u| u.alive)
                .map(|u| (u.cargo.clone(), u.pixel_x, u.pixel_y))
                .unwrap_or_default();

            if cargo.0.is_empty() {
                continue;
            }

            let (loaded_tags, tx, ty) = cargo;

            // Clear cargo.
            if let Some(Some(transport)) = self.units.get_mut(ti) {
                transport.cargo.clear();
            }

            // Unload each unit near the transport.
            for (i, loaded_tag) in loaded_tags.iter().enumerate() {
                let loaded_uid = UnitId::from_tag(*loaded_tag);
                let li = loaded_uid.index() as usize;
                if let Some(Some(unit)) = self.units.get_mut(li) {
                    unit.loaded_in = None;
                    unit.pixel_x = tx + (i as i32 % 4) * 16 - 24;
                    unit.pixel_y = ty + (i as i32 / 4) * 16 - 8;
                    unit.exact_position = crate::fp8::XY::from_pixels(unit.pixel_x, unit.pixel_y);
                }
            }
        }
    }

    // -- Stim / Burrow / Cloak --

    fn issue_stim(&mut self, player_id: u8) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
                && matches!(unit.unit_type, 0 | 32) // Marine (0) or Firebat (32)
                && unit.stim_timer == 0
                && unit.hp > 10 * 256
            {
                unit.hp -= 10 * 256; // Costs 10 HP.
                unit.stim_timer = 270; // ~11 seconds at fastest.
                unit.top_speed = unit.base_top_speed * 3 / 2; // +50% speed.
                // Fire rate boost is handled in update_combat via cooldown reduction.
            }
        }
    }

    fn issue_burrow(&mut self, player_id: u8, burrow: bool) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
                && !unit.is_building
                && !unit.is_air
            {
                unit.burrowed = burrow;
                if burrow {
                    unit.move_state = MoveState::AtRest;
                    unit.velocity = crate::fp8::XY::ZERO;
                    unit.current_speed = crate::fp8::Fp8::ZERO;
                }
            }
        }
    }

    fn issue_cloak(&mut self, player_id: u8, cloak: bool) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize)
                && unit.alive
            {
                unit.cloaked = cloak;
            }
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

                // Terrain height miss chance: 47% miss when attacking uphill.
                let attacker_height = self.map.ground_height_px(
                    attacker.pixel_x.max(0) as u32,
                    attacker.pixel_y.max(0) as u32,
                );
                let target_height = self
                    .map
                    .ground_height_px(tx.max(0) as u32, ty.max(0) as u32);
                let height_miss = target_height > attacker_height
                    && (self
                        .current_frame
                        .wrapping_mul(1103515245)
                        .wrapping_add(12345)
                        % 100)
                        < 47;

                // Copy attacker data before mutable borrows.
                let attacker_owner = attacker.owner;
                let attacker_unit_type = attacker.unit_type;
                let weapon_copy = *weapon;

                if height_miss {
                    if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                        let cd = weapon_copy.cooldown as u16;
                        unit.weapon_cooldown = if unit.stim_timer > 0 { cd / 2 } else { cd };
                    }
                } else {
                    let upgrade_bonus = weapon_copy.damage_bonus as i32
                        * self.player_states[attacker_owner as usize]
                            .upgrade_level(weapon_copy.damage_upgrade)
                            as i32;
                    let base_damage = weapon_copy.damage_amount as i32
                        * weapon_copy.damage_factor.max(1) as i32
                        + upgrade_bonus;

                    let max_hits = if target_is_air {
                        self.data
                            .unit_type(attacker_unit_type)
                            .map(|ut| ut.max_air_hits)
                            .unwrap_or(1)
                    } else {
                        self.data
                            .unit_type(attacker_unit_type)
                            .map(|ut| ut.max_ground_hits)
                            .unwrap_or(1)
                    }
                    .max(1);

                    let (num, den) = weapon_copy.damage_type.size_modifier(target_size);
                    let modified_damage = base_damage * num as i32 / den as i32;

                    let armor_upgrade_id = self
                        .data
                        .unit_type(self.units[ti].as_ref().map(|u| u.unit_type).unwrap_or(0))
                        .map(|ut| ut.armor_upgrade)
                        .unwrap_or(0);
                    let target_owner_idx = self.units[ti]
                        .as_ref()
                        .map(|u| u.owner as usize)
                        .unwrap_or(0);
                    let armor_total = target_armor as i32
                        + self.player_states[target_owner_idx].upgrade_level(armor_upgrade_id)
                            as i32;
                    let per_hit = if weapon_copy.damage_type == crate::dat::DamageType::IgnoreArmor
                    {
                        modified_damage.max(1)
                    } else {
                        (modified_damage - armor_total).max(1)
                    };
                    let total_dmg = per_hit * max_hits as i32 * 256;

                    apply_damage_to_unit(&mut self.units, ti, total_dmg);

                    // Splash.
                    if weapon_copy.is_splash() {
                        let outer = weapon_copy.outer_splash as i64;
                        let medium = weapon_copy.medium_splash as i64;
                        let inner = weapon_copy.inner_splash as i64;
                        for (si, slot) in self.units.iter_mut().enumerate() {
                            if si == ti {
                                continue;
                            }
                            if let Some(su) = slot
                                && su.alive
                                && su.owner != attacker_owner
                            {
                                let sdx = (su.pixel_x - tx) as i64;
                                let sdy = (su.pixel_y - ty) as i64;
                                let sd = sdx * sdx + sdy * sdy;
                                let splash_dmg = if sd <= inner * inner {
                                    total_dmg
                                } else if sd <= medium * medium {
                                    total_dmg / 2
                                } else if sd <= outer * outer {
                                    total_dmg / 4
                                } else {
                                    0
                                };
                                if splash_dmg > 0 {
                                    if su.shields > 0 {
                                        let s = splash_dmg.min(su.shields);
                                        su.shields -= s;
                                        if splash_dmg > s {
                                            su.hp -= splash_dmg - s;
                                        }
                                    } else {
                                        su.hp -= splash_dmg;
                                    }
                                    if su.hp <= 0 {
                                        su.alive = false;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                        let cd = weapon_copy.cooldown as u16;
                        unit.weapon_cooldown = if unit.stim_timer > 0 { cd / 2 } else { cd };
                    }
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
        // Collect units to spawn: (unit_type, owner, spawn_x, spawn_y, rally_x, rally_y).
        let mut spawns: Vec<(u16, u8, i32, i32, i32, i32)> = Vec::new();

        for slot in &mut self.units {
            let Some(unit) = slot else { continue };
            if !unit.alive || unit.build_queue.is_empty() {
                continue;
            }

            if unit.build_timer == 0 {
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
                    let trained_type = unit.build_queue.remove(0);
                    let spawn_x = unit.pixel_x + 32;
                    let spawn_y = unit.pixel_y + 32;
                    spawns.push((
                        trained_type,
                        unit.owner,
                        spawn_x,
                        spawn_y,
                        unit.rally_x,
                        unit.rally_y,
                    ));
                }
            }
        }

        for (unit_type, owner, x, y, rally_x, rally_y) in spawns {
            if let Some(tag) = self.create_unit(unit_type, owner, x, y) {
                // Move spawned unit to rally point if set.
                let uid = UnitId::from_tag(tag);
                let idx = uid.index() as usize;
                let has_rally = rally_x != x || rally_y != y;
                if has_rally
                    && let Some(Some(unit)) = self.units.get_mut(idx)
                    && !unit.is_building
                {
                    unit.move_target = Some((rally_x as u16, rally_y as u16));
                    unit.move_state = MoveState::Moving;
                    unit.waypoints = vec![(rally_x, rally_y)];
                    unit.waypoint_index = 0;
                }
            }
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

    // -- Status effects --

    fn update_status_effects(&mut self) {
        for slot in &mut self.units {
            let Some(unit) = slot else { continue };
            if !unit.alive {
                continue;
            }

            // Irradiate: ~0.33 damage per frame (250 / 750 = 0.33).
            if unit.irradiate_timer > 0 {
                unit.irradiate_timer -= 1;
                let dmg = 85; // ~0.33 * 256 fp8
                unit.hp -= dmg;
                if unit.hp <= 0 {
                    unit.alive = false;
                }
            }

            // Plague: ~0.5 damage per frame (300 / 600 = 0.5). Can't kill — floor at 1 HP.
            if unit.plague_timer > 0 {
                unit.plague_timer -= 1;
                let dmg = 128; // ~0.5 * 256 fp8
                if unit.hp > 256 {
                    // Don't let plague kill — minimum 1 HP (256 fp8).
                    unit.hp = (unit.hp - dmg).max(256);
                }
            }

            // Ensnare: decrement timer, restore speed when expired.
            if unit.ensnare_timer > 0 {
                unit.ensnare_timer -= 1;
                if unit.ensnare_timer == 0 {
                    unit.top_speed = unit.base_top_speed;
                }
            }

            // Stim: decrement timer, restore speed when expired.
            // While active, weapon cooldown is halved (handled in combat via cooldown reset).
            if unit.stim_timer > 0 {
                unit.stim_timer -= 1;
                if unit.stim_timer == 0 {
                    unit.top_speed = unit.base_top_speed;
                }
            }
        }
    }

    // -- Shield regeneration --

    fn update_shield_regen(&mut self) {
        // Protoss shields regenerate: +7 fp8 per 8 frames ≈ ~0.2 shield/sec.
        for slot in &mut self.units {
            if let Some(unit) = slot
                && unit.alive
                && unit.max_shields > 0
                && unit.shields < unit.max_shields
            {
                unit.shields = (unit.shields + 7).min(unit.max_shields);
            }
        }
    }

    // -- Collision --

    fn update_collision(&mut self) {
        // Spatial hash grid for O(n) collision instead of O(n²).
        let map_w = self.map.width_px() as i32;
        let map_h = self.map.height_px() as i32;
        let mut grid = SpatialGrid::new(map_w, map_h, 32);

        // Insert all ground units into grid.
        for (i, slot) in self.units.iter().enumerate() {
            if let Some(u) = slot
                && u.alive
                && !u.is_air
            {
                grid.insert(i, u.pixel_x, u.pixel_y);
            }
        }

        // Collect push operations to avoid borrow conflicts.
        let mut pushes: Vec<(usize, i32, i32)> = Vec::new();

        for i in 0..self.units.len() {
            let (ax, ay, ar) = match &self.units[i] {
                Some(u) if u.alive && !u.is_air && !u.is_building => {
                    (u.pixel_x, u.pixel_y, u.collision_radius as i32)
                }
                _ => continue,
            };

            for j in grid.query_neighbors(ax, ay) {
                if j <= i {
                    continue;
                }
                let (bx, by, br, b_building) = match &self.units[j] {
                    Some(u) if u.alive && !u.is_air => (
                        u.pixel_x,
                        u.pixel_y,
                        u.collision_radius as i32,
                        u.is_building,
                    ),
                    _ => continue,
                };

                let dx = ax - bx;
                let dy = ay - by;
                let min_dist = ar + br;
                let dist_sq = dx as i64 * dx as i64 + dy as i64 * dy as i64;
                let min_dist_sq = min_dist as i64 * min_dist as i64;

                if dist_sq < min_dist_sq && dist_sq > 0 {
                    let dist = (dist_sq as f64).sqrt() as i32;
                    let overlap = min_dist - dist;
                    let push = (overlap / 2).max(1);
                    if dist > 0 {
                        let nx = dx * push / dist;
                        let ny = dy * push / dist;
                        if b_building {
                            pushes.push((i, nx * 2, ny * 2));
                        } else {
                            pushes.push((i, nx, ny));
                            pushes.push((j, -nx, -ny));
                        }
                    }
                }
            }
        }

        for (idx, dx, dy) in pushes {
            if let Some(Some(u)) = self.units.get_mut(idx) {
                u.pixel_x += dx;
                u.pixel_y += dy;
                u.exact_position = crate::fp8::XY::from_pixels(u.pixel_x, u.pixel_y);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit type classification helpers
// ---------------------------------------------------------------------------

fn apply_damage_to_unit(units: &mut [Option<UnitState>], idx: usize, damage_fp8: i32) {
    if let Some(Some(target)) = units.get_mut(idx) {
        if target.shields > 0 {
            let shield_dmg = damage_fp8.min(target.shields);
            target.shields -= shield_dmg;
            let remaining = damage_fp8 - shield_dmg;
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
}

fn is_transport_type(unit_type: u16) -> bool {
    matches!(unit_type, 11 | 69 | 42) // Dropship, Shuttle, Overlord
}

/// Energy cost for a spell (in fp8). Returns 0 if not a known spell.
fn spell_energy_cost(tech_type: u8) -> i32 {
    match tech_type {
        19 => 75 * 256,  // Psionic Storm
        2 => 100 * 256,  // EMP
        7 => 75 * 256,   // Irradiate
        15 => 150 * 256, // Plague
        17 => 75 * 256,  // Ensnare
        14 => 100 * 256, // Dark Swarm
        22 => 100 * 256, // Stasis Field
        6 => 100 * 256,  // Defensive Matrix
        _ => 0,
    }
}

/// Spell effect: (damage_fp8, radius_pixels, affects_shields).
fn spell_effect(tech_type: u8) -> (i32, i32, bool) {
    match tech_type {
        19 => (112 * 256, 48, true),  // Psionic Storm: 112 damage over area
        2 => (0, 64, false),          // EMP: drains shields (handled separately — 0 HP damage)
        7 => (250 * 256, 0, false),   // Irradiate: 250 damage to single target (no AoE here)
        15 => (300 * 256, 64, false), // Plague: 300 damage over area (ignores shields)
        17 => (0, 64, false),         // Ensnare: no damage (slow effect)
        _ => (0, 0, false),
    }
}

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
            inner_splash: 0,
            medium_splash: 0,
            outer_splash: 0,
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

    // -- Spell tests --

    #[test]
    fn test_spell_deducts_energy() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0), // "caster" (we'll give it energy)
            make_chk_unit(1, 80, 50, 0, 1), // target
        ])
        .unwrap();
        // Give unit 0 caster energy.
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.energy = 200 * 256;
            u.max_energy = 200 * 256;
        }

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(
            0,
            &EngineCommand::CastSpell {
                tech_type: 19, // Psionic Storm
                x: 80,
                y: 50,
            },
        );

        let caster = game.unit_by_tag(0).unwrap();
        assert!(
            caster.energy < 200 * 256,
            "energy should be deducted after spell"
        );
    }

    // -- Rally tests --

    #[test]
    fn test_rally_point() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)]) // Barracks
            .unwrap();
        game.set_player_resources(0, 500, 500);

        // Set rally point.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::SetRally { x: 200, y: 100 });

        // Check rally was set.
        let barracks = game.unit_by_tag(0).unwrap();
        assert_eq!(barracks.rally_x, 200);
        assert_eq!(barracks.rally_y, 100);
    }

    // -- Transport tests --

    #[test]
    fn test_load_and_unload() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0), // Marine (will be loaded)
            make_chk_unit(1, 55, 50, 0, 0), // Marine (will be loaded)
        ])
        .unwrap();

        // Manually create a transport-like unit at index 2.
        let tag = game.create_unit(11, 0, 60, 50); // Dropship = 11
        assert!(tag.is_some());

        // Select marines, load into transport.
        game.apply_command(0, &EngineCommand::Select(vec![0, 1]));
        game.apply_command(
            0,
            &EngineCommand::Load {
                transport_tag: tag.unwrap(),
            },
        );

        let transport = game.unit_by_tag(tag.unwrap()).unwrap();
        assert!(
            !transport.cargo.is_empty(),
            "transport should have loaded units"
        );

        // Unload.
        game.apply_command(0, &EngineCommand::Select(vec![tag.unwrap()]));
        game.apply_command(0, &EngineCommand::UnloadAll);

        let transport = game.unit_by_tag(tag.unwrap()).unwrap();
        assert!(
            transport.cargo.is_empty(),
            "cargo should be empty after unload"
        );
    }

    // -- Shield regen test --

    #[test]
    fn test_shield_regen() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        // Give unit shields.
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.shields = 10 * 256; // 10 shields
            u.max_shields = 50 * 256; // 50 max
        }

        // Step many frames.
        for _ in 0..200 {
            game.step();
        }

        let unit = game.unit_by_tag(0).unwrap();
        assert!(
            unit.shields > 10 * 256,
            "shields should have regenerated: {}",
            unit.shields
        );
    }

    // -- Energy regen test --

    #[test]
    fn test_energy_regen() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.energy = 10 * 256;
            u.max_energy = 200 * 256;
        }

        for _ in 0..100 {
            game.step();
        }

        let unit = game.unit_by_tag(0).unwrap();
        assert!(
            unit.energy > 10 * 256,
            "energy should have regenerated: {}",
            unit.energy
        );
    }

    // -- Mining test --

    #[test]
    fn test_worker_mining_generates_income() {
        let mut game = Game::new(test_map(), test_game_data());
        // Use unit type 0 but manually mark as worker with no weapon.
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)])
            .unwrap();
        if let Some(Some(u)) = game.units.get_mut(0) {
            u.is_worker = true;
            u.ground_weapon = 130; // No weapon — prevents auto-attack.
        }
        game.set_player_resources(0, 0, 0);

        // Step enough for mining cycles (~75 frames per trip, checked every 8).
        for _ in 0..700 {
            game.step();
        }

        let ps = game.player_state(0).unwrap();
        assert!(
            ps.minerals > 0,
            "worker should have mined minerals: got {}",
            ps.minerals
        );
    }

    // -- Collision test --

    #[test]
    fn test_units_dont_overlap() {
        let mut game = Game::new(test_map(), test_game_data());
        // Place two friendly units very close (same owner = no combat, but close enough to collide).
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0),
            make_chk_unit(1, 52, 50, 0, 0),
        ])
        .unwrap();

        // Step — collision should push them apart.
        for _ in 0..20 {
            game.step();
        }

        let u0 = game.unit_by_tag(0).unwrap();
        let u1 = game.unit_by_tag(1).unwrap();
        let dist = ((u0.pixel_x - u1.pixel_x).pow(2) + (u0.pixel_y - u1.pixel_y).pow(2)) as f64;
        assert!(dist.sqrt() > 1.0, "units should have been pushed apart");
    }
}
