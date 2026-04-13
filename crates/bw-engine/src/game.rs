use crate::chk_units::ChkUnit;
use crate::dat::GameData;
use crate::direction::Direction;
use crate::error::Result;
use crate::fp8::{Fp8, XY};
use crate::map::Map;
use crate::selection::SelectionState;
use crate::unit::{MoveState, UnitId, UnitState};

/// Maximum number of units BW supports.
pub const MAX_UNITS: usize = 1700;

/// Commands that the engine understands.
/// The caller (e.g. replay-wasm) translates from replay_core::Command.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    Select(Vec<u16>),
    SelectAdd(Vec<u16>),
    SelectRemove(Vec<u16>),
    HotkeyAssign { group: u8 },
    HotkeyRecall { group: u8 },
    Move { x: u16, y: u16 },
    Stop,
}

/// The game simulation state.
pub struct Game {
    data: GameData,
    map: Map,
    current_frame: u32,
    units: Vec<Option<UnitState>>,
    selection: SelectionState,
    next_unit_index: u16,
}

impl Game {
    pub fn new(map: Map, data: GameData) -> Self {
        Self {
            data,
            map,
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
            // Start Location (unit type 214) is not a real unit.
            if chu.unit_type == 214 {
                continue;
            }

            let index = self.next_unit_index;
            self.next_unit_index += 1;

            let flingy = self
                .data
                .flingy_for_unit(chu.unit_type)
                .copied()
                .unwrap_or_default();

            let id = UnitId::new(index, 0);
            let unit = UnitState {
                id,
                unit_type: chu.unit_type,
                owner: chu.owner,
                alive: true,
                exact_position: XY::from_pixels(chu.x as i32, chu.y as i32),
                pixel_x: chu.x as i32,
                pixel_y: chu.y as i32,
                velocity: XY::ZERO,
                heading: Direction::default(),
                current_speed: Fp8::ZERO,
                move_state: MoveState::AtRest,
                move_target: None,
                top_speed: flingy.top_speed,
                acceleration: flingy.acceleration,
                halt_distance: flingy.halt_distance,
                turn_rate: flingy.turn_rate,
                movement_type: flingy.movement_type,
            };
            self.units[index as usize] = Some(unit);
        }
        Ok(())
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
            EngineCommand::Stop => {
                self.issue_stop(player_id);
            }
        }
    }

    /// Advance the simulation by one frame.
    pub fn step(&mut self) {
        self.current_frame += 1;
        for slot in &mut self.units {
            if let Some(unit) = slot {
                if unit.alive {
                    unit.update_movement();
                }
            }
        }
    }

    /// Step to a target frame.
    pub fn step_to(&mut self, target_frame: u32) {
        while self.current_frame < target_frame {
            self.step();
        }
    }

    /// Current simulation frame.
    pub fn current_frame(&self) -> u32 {
        self.current_frame
    }

    /// Iterator over all alive units.
    pub fn units(&self) -> impl Iterator<Item = &UnitState> {
        self.units
            .iter()
            .filter_map(|slot| slot.as_ref().filter(|u| u.alive))
    }

    /// Get a specific unit by replay tag.
    pub fn unit_by_tag(&self, tag: u16) -> Option<&UnitState> {
        let uid = UnitId::from_tag(tag);
        self.units
            .get(uid.index() as usize)?
            .as_ref()
            .filter(|u| u.id.generation() == uid.generation() && u.alive)
    }

    /// Total count of alive units.
    pub fn unit_count(&self) -> usize {
        self.units().count()
    }

    /// Access the map.
    pub fn map(&self) -> &Map {
        &self.map
    }

    fn issue_move(&mut self, player_id: u8, x: u16, y: u16) {
        let tags: Vec<u16> = self.selection.selected_tags(player_id).to_vec();
        for tag in &tags {
            let uid = UnitId::from_tag(*tag);
            if let Some(Some(unit)) = self.units.get_mut(uid.index() as usize) {
                if unit.id.generation() == uid.generation() && unit.alive {
                    unit.move_target = Some((x, y));
                    unit.move_state = MoveState::Moving;
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
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chk_units::ChkUnit;
    use crate::dat::{FlingyType, GameData};
    use crate::tileset::{CV5_ENTRY_SIZE, VF4_ENTRY_SIZE};
    use crate::tile::MiniTile;

    fn test_game_data() -> GameData {
        // Marine flingy: speed 4.0, accel 1.0, turn 40
        let marine_flingy = FlingyType {
            top_speed: 4 * 256,
            acceleration: 256,
            halt_distance: 0,
            turn_rate: 40,
            movement_type: 0,
        };
        let mut flingy_types = vec![FlingyType::default(); 209];
        flingy_types[0] = marine_flingy;

        let mut unit_flingy = vec![0u8; 228];
        unit_flingy[0] = 0; // Marine -> flingy 0

        GameData {
            flingy_types,
            unit_flingy,
        }
    }

    fn test_map() -> Map {
        // Build a tiny 4x4 map (all walkable).
        let walkable = [MiniTile::WALKABLE; 16];
        let mut vf4 = vec![0u8; VF4_ENTRY_SIZE];
        for (j, &f) in walkable.iter().enumerate() {
            vf4[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
        }
        let mut cv5 = vec![0u8; CV5_ENTRY_SIZE];
        // mega_tile_indices[0] = 0
        cv5[20..22].copy_from_slice(&0u16.to_le_bytes());

        let mut chk = Vec::new();
        // DIM
        chk.extend_from_slice(b"DIM ");
        chk.extend_from_slice(&4u32.to_le_bytes());
        chk.extend_from_slice(&4u16.to_le_bytes());
        chk.extend_from_slice(&4u16.to_le_bytes());
        // ERA
        chk.extend_from_slice(b"ERA ");
        chk.extend_from_slice(&2u32.to_le_bytes());
        chk.extend_from_slice(&0u16.to_le_bytes());
        // MTXM
        let mtxm: Vec<u8> = vec![0u8; 4 * 4 * 2];
        chk.extend_from_slice(b"MTXM");
        chk.extend_from_slice(&(mtxm.len() as u32).to_le_bytes());
        chk.extend_from_slice(&mtxm);

        Map::from_chk(&chk, &cv5, &vf4).unwrap()
    }

    #[test]
    fn test_load_initial_units() {
        let map = test_map();
        let data = test_game_data();
        let mut game = Game::new(map, data);

        let chk_units = vec![
            ChkUnit {
                instance_id: 0,
                x: 50,
                y: 50,
                unit_type: 0, // Marine
                owner: 0,
                hp_percent: 100,
                shield_percent: 0,
                energy_percent: 0,
                resources: 0,
            },
            ChkUnit {
                instance_id: 1,
                x: 80,
                y: 80,
                unit_type: 0,
                owner: 1,
                hp_percent: 100,
                shield_percent: 0,
                energy_percent: 0,
                resources: 0,
            },
        ];

        game.load_initial_units(&chk_units).unwrap();
        assert_eq!(game.unit_count(), 2);

        let u0 = game.unit_by_tag(0).unwrap();
        assert_eq!(u0.pixel_x, 50);
        assert_eq!(u0.pixel_y, 50);
        assert_eq!(u0.owner, 0);
    }

    #[test]
    fn test_select_and_move() {
        let map = test_map();
        let data = test_game_data();
        let mut game = Game::new(map, data);

        let chk_units = vec![ChkUnit {
            instance_id: 0,
            x: 50,
            y: 50,
            unit_type: 0,
            owner: 0,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        }];
        game.load_initial_units(&chk_units).unwrap();

        // Select unit 0, then issue move.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 100, y: 50 });

        // Step 50 frames.
        for _ in 0..50 {
            game.step();
        }

        let unit = game.unit_by_tag(0).unwrap();
        assert!(unit.pixel_x > 50, "unit should have moved east: x={}", unit.pixel_x);
    }

    #[test]
    fn test_select_and_stop() {
        let map = test_map();
        let data = test_game_data();
        let mut game = Game::new(map, data);

        let chk_units = vec![ChkUnit {
            instance_id: 0,
            x: 50,
            y: 50,
            unit_type: 0,
            owner: 0,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        }];
        game.load_initial_units(&chk_units).unwrap();

        // Select, move, step a bit, then stop.
        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 200, y: 50 });
        for _ in 0..10 {
            game.step();
        }

        let x_before_stop = game.unit_by_tag(0).unwrap().pixel_x;
        game.apply_command(0, &EngineCommand::Stop);

        for _ in 0..10 {
            game.step();
        }

        let x_after_stop = game.unit_by_tag(0).unwrap().pixel_x;
        assert_eq!(x_before_stop, x_after_stop, "unit should not move after stop");
    }

    #[test]
    fn test_unit_arrives() {
        let map = test_map();
        let data = test_game_data();
        let mut game = Game::new(map, data);

        let chk_units = vec![ChkUnit {
            instance_id: 0,
            x: 50,
            y: 50,
            unit_type: 0,
            owner: 0,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        }];
        game.load_initial_units(&chk_units).unwrap();

        game.apply_command(0, &EngineCommand::Select(vec![0]));
        game.apply_command(0, &EngineCommand::Move { x: 70, y: 50 });

        for _ in 0..200 {
            game.step();
        }

        let unit = game.unit_by_tag(0).unwrap();
        assert_eq!(unit.move_state, MoveState::Arrived);
        assert_eq!(unit.pixel_x, 70);
        assert_eq!(unit.pixel_y, 50);
    }

    #[test]
    fn test_step_to() {
        let map = test_map();
        let data = test_game_data();
        let mut game = Game::new(map, data);
        game.step_to(100);
        assert_eq!(game.current_frame(), 100);
    }
}
