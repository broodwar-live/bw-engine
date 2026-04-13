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

/// Maximum number of units BW supports.
pub const MAX_UNITS: usize = 1700;

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
}

impl Game {
    pub fn new(map: Map, data: GameData) -> Self {
        let region_map = RegionMap::from_map(&map);
        Self {
            data,
            map,
            region_map,
            current_frame: 0,
            units: vec![None; MAX_UNITS],
            selection: SelectionState::default(),
            next_unit_index: 0,
        }
    }

    /// Load initial units from CHK UNIT section entries.
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
            armor: ut.armor,
            ground_weapon: ut.ground_weapon,
            air_weapon: ut.air_weapon,
            weapon_cooldown: 0,
            attack_target: None,
            build_queue: Vec::new(),
            build_timer: 0,
            is_building: ut.is_building,
        };
        self.units[index as usize] = Some(unit);
        Some(id.to_tag())
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
        }
    }

    /// Advance the simulation by one frame.
    pub fn step(&mut self) {
        self.current_frame += 1;

        // Phase 1: Movement.
        for slot in &mut self.units {
            if let Some(unit) = slot {
                if unit.alive {
                    unit.update_movement();
                    if unit.weapon_cooldown > 0 {
                        unit.weapon_cooldown -= 1;
                    }
                }
            }
        }

        // Phase 2: Combat — resolve attacks.
        self.update_combat();

        // Phase 3: Production — advance build timers.
        self.update_production();
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
            .filter(|u| u.id.generation() == uid.generation() && u.alive)
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
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if unit.id.generation() == uid.generation() && unit.alive {
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
    }

    fn issue_attack(&mut self, player_id: u8, target_tag: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if unit.id.generation() == uid.generation() && unit.alive {
                    unit.attack_target = Some(target_tag);
                    // Movement toward target is handled in update_combat.
                }
            }
        }
    }

    fn issue_stop(&mut self, player_id: u8) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if unit.id.generation() == uid.generation() && unit.alive {
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
    }

    fn issue_train(&mut self, player_id: u8, unit_type: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if unit.id.generation() == uid.generation() && unit.alive && unit.is_building {
                    if unit.build_queue.len() < 5 {
                        unit.build_queue.push(unit_type);
                    }
                }
            }
        }
    }

    // -- Simulation phases --

    fn update_combat(&mut self) {
        // Collect attack actions: (attacker_index, target_tag).
        let mut attacks: Vec<(usize, u16)> = Vec::new();

        for (i, slot) in self.units.iter().enumerate() {
            if let Some(unit) = slot {
                if !unit.alive {
                    continue;
                }
                if let Some(target_tag) = unit.attack_target {
                    attacks.push((i, target_tag));
                }
            }
        }

        for (attacker_idx, target_tag) in attacks {
            let target_uid = UnitId::from_tag(target_tag);
            let ti = target_uid.index() as usize;

            // Get target position and alive status.
            let target_info = self.units.get(ti).and_then(|s| {
                s.as_ref().filter(|u| {
                    u.id.generation() == target_uid.generation() && u.alive
                })
            }).map(|u| (u.pixel_x, u.pixel_y));

            let Some((tx, ty)) = target_info else {
                // Target dead or invalid — clear attack.
                if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                    unit.attack_target = None;
                }
                continue;
            };

            // Check range and fire.
            let attacker = self.units[attacker_idx].as_ref().unwrap();
            let weapon_id = attacker.ground_weapon; // simplified: use ground weapon
            let Some(weapon) = self.data.weapon_type(weapon_id) else {
                continue; // No weapon.
            };

            let dx = (attacker.pixel_x - tx).abs();
            let dy = (attacker.pixel_y - ty).abs();
            let dist_sq = (dx as u64) * (dx as u64) + (dy as u64) * (dy as u64);
            let range = weapon.max_range as u64;
            let range_sq = range * range;

            if dist_sq <= range_sq && attacker.weapon_cooldown == 0 {
                // Fire! Apply damage to target.
                let damage = weapon.damage_amount as i32 * weapon.damage_factor.max(1) as i32;
                let target_armor = self.units[ti].as_ref().map(|u| u.armor).unwrap_or(0);
                let effective_damage = (damage - target_armor as i32).max(1);
                // Damage in fp8 (HP is stored in fp8).
                let damage_fp8 = effective_damage * 256;

                if let Some(Some(target)) = self.units.get_mut(ti) {
                    target.hp -= damage_fp8;
                    if target.hp <= 0 {
                        target.alive = false;
                    }
                }

                // Reset cooldown.
                if let Some(Some(unit)) = self.units.get_mut(attacker_idx) {
                    unit.weapon_cooldown = weapon.cooldown as u16;
                }
            } else if dist_sq > range_sq {
                // Out of range — move toward target.
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
            hitpoints: 40 * 256,
            ground_weapon: 0,
            max_ground_hits: 1,
            air_weapon: 130,
            max_air_hits: 0,
            armor: 0,
            build_time: 30, // 30 frames for testing
            is_building: false,
        };
        let barracks_ut = UnitType {
            flingy_id: 0,
            hitpoints: 1000 * 256,
            ground_weapon: 130, // no weapon
            max_ground_hits: 0,
            air_weapon: 130,
            max_air_hits: 0,
            armor: 1,
            build_time: 0,
            is_building: true,
        };
        let mut unit_types = vec![UnitType::default(); 228];
        unit_types[0] = marine_ut;     // Marine = unit type 0
        unit_types[122] = barracks_ut; // Barracks = unit type 122

        let marine_weapon = WeaponType {
            damage_amount: 6,
            damage_bonus: 0,
            cooldown: 15,
            damage_factor: 1,
            max_range: 128, // 4 tiles
        };
        let mut weapon_types = vec![WeaponType::default(); 130];
        weapon_types[0] = marine_weapon;

        GameData {
            flingy_types,
            unit_types,
            weapon_types,
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
        let units = vec![make_chk_unit(0, 50, 50, 0, 0), make_chk_unit(1, 80, 80, 0, 1)];
        game.load_initial_units(&units).unwrap();
        assert_eq!(game.unit_count(), 2);
        let u0 = game.unit_by_tag(0).unwrap();
        assert_eq!(u0.pixel_x, 50);
        assert_eq!(u0.hp, 40 * 256);
    }

    #[test]
    fn test_select_and_move() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)]).unwrap();
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
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)]).unwrap();
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
        game.load_initial_units(&[make_chk_unit(0, 50, 50, 0, 0)]).unwrap();
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
        // Two marines close together (within range=128px).
        game.load_initial_units(&[
            make_chk_unit(0, 50, 50, 0, 0),  // attacker
            make_chk_unit(1, 100, 50, 0, 1),  // target (50px away, within 128px range)
        ])
        .unwrap();

        // Player 0 selects marine 0 and attacks marine 1.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Attack { target_tag: 1 });

        // Step enough frames for the target to die.
        // Marine: 40 HP, weapon does 6 dmg every 15 frames → ~7 hits = ~105 frames.
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
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)]).unwrap();
        let initial_count = game.unit_count();

        // Select barracks, train a marine.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Train { unit_type: 0 });

        // Build time is 30 frames in test data.
        for _ in 0..50 {
            game.step();
        }

        assert_eq!(game.unit_count(), initial_count + 1, "should have spawned a marine");
    }

    #[test]
    fn test_train_queue() {
        let mut game = Game::new(test_map(), test_game_data());
        game.load_initial_units(&[make_chk_unit(0, 100, 100, 122, 0)]).unwrap();

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
}
