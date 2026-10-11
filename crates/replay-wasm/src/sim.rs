//! Game simulation bindings (feature `sim`). Experimental and frozen; see README.

use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Game simulation
// ---------------------------------------------------------------------------

/// A running game simulation that processes replay commands and steps frames.
#[wasm_bindgen]
pub struct GameSim {
    inner: bw_engine::Game,
    commands: Vec<replay_core::command::GameCommand>,
    command_cursor: usize,
}

#[wasm_bindgen]
impl GameSim {
    /// Create a new simulation.
    ///
    /// - `chk_data`: raw CHK bytes (from `replay.map_data`).
    /// - `cv5_data` / `vf4_data`: tileset files.
    /// - `units_dat` / `flingy_dat` / `weapons_dat`: game data files.
    /// - `replay_data`: raw `.rep` file bytes (re-parsed to extract commands + CHK units).
    ///
    /// `weapons_dat` can be empty (combat will be disabled).
    #[wasm_bindgen(constructor)]
    pub fn new(
        chk_data: &[u8],
        cv5_data: &[u8],
        vf4_data: &[u8],
        units_dat: &[u8],
        flingy_dat: &[u8],
        weapons_dat: &[u8],
        replay_data: &[u8],
    ) -> Result<GameSim, JsError> {
        let map = bw_engine::Map::from_chk(chk_data, cv5_data, vf4_data)
            .map_err(|e| JsError::new(&e.to_string()))?;
        let data = if weapons_dat.is_empty() {
            bw_engine::GameData::from_dat(units_dat, flingy_dat)
        } else {
            bw_engine::GameData::from_dat_full(units_dat, flingy_dat, weapons_dat)
        }
        .map_err(|e| JsError::new(&e.to_string()))?;
        let mut game = bw_engine::Game::new(map, data);

        // Load initial units from CHK UNIT section.
        let sections =
            bw_engine::chk::parse_sections(chk_data).map_err(|e| JsError::new(&e.to_string()))?;
        let chk_units = bw_engine::chk_units::parse_chk_units(&sections)
            .map_err(|e| JsError::new(&e.to_string()))?;
        game.load_initial_units(&chk_units)
            .map_err(|e| JsError::new(&e.to_string()))?;

        // Parse replay for commands.
        let replay = replay_core::parse(replay_data).map_err(|e| JsError::new(&e.to_string()))?;

        Ok(Self {
            inner: game,
            commands: replay.commands,
            command_cursor: 0,
        })
    }

    /// Advance the simulation by one frame, applying any commands at this frame.
    pub fn step(&mut self) {
        let next_frame = self.inner.current_frame() + 1;

        // Apply all commands for the next frame.
        while self.command_cursor < self.commands.len()
            && self.commands[self.command_cursor].frame <= next_frame
        {
            let gc = &self.commands[self.command_cursor];
            if gc.frame == next_frame
                && let Some(cmd) = translate_command(&gc.command)
            {
                self.inner.apply_command(gc.player_id, &cmd);
            }
            self.command_cursor += 1;
        }

        self.inner.step();
    }

    /// Step to a specific frame, applying all commands along the way.
    #[wasm_bindgen(js_name = "stepTo")]
    pub fn step_to(&mut self, target_frame: u32) {
        while self.inner.current_frame() < target_frame {
            self.step();
        }
    }

    /// Current simulation frame.
    #[wasm_bindgen(getter, js_name = "currentFrame")]
    pub fn current_frame(&self) -> u32 {
        self.inner.current_frame()
    }

    /// Number of alive units.
    #[wasm_bindgen(getter, js_name = "unitCount")]
    pub fn unit_count(&self) -> usize {
        self.inner.unit_count()
    }

    /// Get all alive units as a flat array: [x, y, unitType, owner, hp, maxHp] repeated.
    /// Each unit is 6 consecutive i32 values.
    #[wasm_bindgen(js_name = "unitData")]
    pub fn unit_data(&self) -> Vec<i32> {
        let mut data = Vec::new();
        for unit in self.inner.units() {
            data.push(unit.pixel_x);
            data.push(unit.pixel_y);
            data.push(unit.unit_type as i32);
            data.push(unit.owner as i32);
            data.push(unit.hp);
            data.push(unit.max_hp);
        }
        data
    }

    /// Get the visibility grid for a player.
    /// Flat Uint8Array: 0=fog, 1=explored, 2=visible. Row-major, tile dimensions.
    #[wasm_bindgen(js_name = "visibilityGrid")]
    pub fn visibility_grid(&self, player: u8) -> Vec<u8> {
        self.inner.visibility_grid(player)
    }

    /// Get player resources as [minerals, gas, supplyUsed, supplyMax] x 8 players.
    #[wasm_bindgen(js_name = "playerData")]
    pub fn player_data(&self) -> Vec<i32> {
        let mut data = Vec::new();
        for i in 0..8u8 {
            if let Some(ps) = self.inner.player_state(i) {
                data.push(ps.minerals);
                data.push(ps.gas);
                data.push(ps.supply_used);
                data.push(ps.supply_max);
            } else {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
        data
    }
}

/// Translate a replay_core::Command into a bw_engine::EngineCommand.
fn translate_command(cmd: &replay_core::command::Command) -> Option<bw_engine::EngineCommand> {
    use replay_core::command::{Command, HotkeyAction};

    match cmd {
        Command::Select { unit_tags } => Some(bw_engine::EngineCommand::Select(unit_tags.clone())),
        Command::SelectAdd { unit_tags } => {
            Some(bw_engine::EngineCommand::SelectAdd(unit_tags.clone()))
        }
        Command::SelectRemove { unit_tags } => {
            Some(bw_engine::EngineCommand::SelectRemove(unit_tags.clone()))
        }
        Command::Hotkey { action, group } => match action {
            HotkeyAction::Assign => Some(bw_engine::EngineCommand::HotkeyAssign { group: *group }),
            HotkeyAction::Select => Some(bw_engine::EngineCommand::HotkeyRecall { group: *group }),
        },
        Command::RightClick {
            x, y, target_tag, ..
        } => {
            if *target_tag == 0 || *target_tag == 0xFFFF {
                Some(bw_engine::EngineCommand::Move { x: *x, y: *y })
            } else {
                Some(bw_engine::EngineCommand::Attack {
                    target_tag: *target_tag,
                })
            }
        }
        Command::TargetedOrder {
            x,
            y,
            order,
            target_tag,
            ..
        } => {
            if *order == 0x06 {
                // Move order.
                Some(bw_engine::EngineCommand::Move { x: *x, y: *y })
            } else if *order == 0x0A && *target_tag != 0 && *target_tag != 0xFFFF {
                // AttackUnit order.
                Some(bw_engine::EngineCommand::Attack {
                    target_tag: *target_tag,
                })
            } else {
                None
            }
        }
        Command::Stop { .. } => Some(bw_engine::EngineCommand::Stop),
        Command::Train { unit_type } => Some(bw_engine::EngineCommand::Train {
            unit_type: *unit_type,
        }),
        Command::Build {
            x, y, unit_type, ..
        } => Some(bw_engine::EngineCommand::Build {
            x: *x,
            y: *y,
            unit_type: *unit_type,
        }),
        Command::UnitMorph { unit_type } => Some(bw_engine::EngineCommand::UnitMorph {
            unit_type: *unit_type,
        }),
        Command::BuildingMorph { unit_type } => Some(bw_engine::EngineCommand::BuildingMorph {
            unit_type: *unit_type,
        }),
        Command::Research { tech_type } => Some(bw_engine::EngineCommand::Research {
            tech_type: *tech_type,
        }),
        Command::Upgrade { upgrade_type } => Some(bw_engine::EngineCommand::Upgrade {
            upgrade_type: *upgrade_type,
        }),
        Command::Stim => Some(bw_engine::EngineCommand::Stim),
        Command::Burrow { .. } => Some(bw_engine::EngineCommand::Burrow),
        Command::Unburrow { .. } => Some(bw_engine::EngineCommand::Unburrow),
        Command::Cloak { .. } => Some(bw_engine::EngineCommand::Cloak),
        Command::Decloak { .. } => Some(bw_engine::EngineCommand::Decloak),
        _ => None,
    }
}
