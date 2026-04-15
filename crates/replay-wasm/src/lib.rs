use wasm_bindgen::prelude::*;

/// Parse a `.rep` file and return the full replay as a JS object.
///
/// Returns an object with: header, commands, build_order, player_apm, timeline, map_data.
#[wasm_bindgen(js_name = "parseReplay")]
pub fn parse_replay(data: &[u8]) -> Result<JsValue, JsError> {
    let replay = replay_core::parse(data).map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&replay).map_err(|e| JsError::new(&e.to_string()))
}

/// A queryable map parsed from CHK and tileset data.
#[wasm_bindgen]
pub struct GameMap {
    inner: bw_engine::Map,
}

#[wasm_bindgen]
impl GameMap {
    #[wasm_bindgen(constructor)]
    pub fn new(chk_data: &[u8], cv5_data: &[u8], vf4_data: &[u8]) -> Result<GameMap, JsError> {
        let inner = bw_engine::Map::from_chk(chk_data, cv5_data, vf4_data)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u16 {
        self.inner.width()
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u16 {
        self.inner.height()
    }

    #[wasm_bindgen(getter, js_name = "widthPx")]
    pub fn width_px(&self) -> u32 {
        self.inner.width_px()
    }

    #[wasm_bindgen(getter, js_name = "heightPx")]
    pub fn height_px(&self) -> u32 {
        self.inner.height_px()
    }

    #[wasm_bindgen(getter)]
    pub fn tileset(&self) -> String {
        self.inner.tileset().name().to_string()
    }

    #[wasm_bindgen(js_name = "isWalkable")]
    pub fn is_walkable(&self, mx: u16, my: u16) -> bool {
        self.inner.is_walkable(mx, my)
    }

    #[wasm_bindgen(js_name = "groundHeight")]
    pub fn ground_height(&self, mx: u16, my: u16) -> u8 {
        self.inner
            .ground_height(mx, my)
            .map(|h| match h {
                bw_engine::GroundHeight::Low => 0,
                bw_engine::GroundHeight::Middle => 1,
                bw_engine::GroundHeight::High => 2,
                bw_engine::GroundHeight::VeryHigh => 3,
            })
            .unwrap_or(0)
    }

    #[wasm_bindgen(js_name = "isWalkablePx")]
    pub fn is_walkable_px(&self, px: u32, py: u32) -> bool {
        self.inner.is_walkable_px(px, py)
    }

    #[wasm_bindgen(js_name = "walkabilityGrid")]
    pub fn walkability_grid(&self) -> Vec<u8> {
        self.inner
            .mini_tiles()
            .iter()
            .map(|mt| mt.is_walkable() as u8)
            .collect()
    }

    #[wasm_bindgen(js_name = "heightGrid")]
    pub fn height_grid(&self) -> Vec<u8> {
        self.inner
            .mini_tiles()
            .iter()
            .map(|mt| match mt.ground_height() {
                bw_engine::GroundHeight::Low => 0,
                bw_engine::GroundHeight::Middle => 1,
                bw_engine::GroundHeight::High => 2,
                bw_engine::GroundHeight::VeryHigh => 3,
            })
            .collect()
    }
}

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

// ---------------------------------------------------------------------------
// MPQ Archive
// ---------------------------------------------------------------------------

/// An MPQ archive reader for StarCraft game data and map files.
#[wasm_bindgen]
pub struct MpqFile {
    inner: bw_engine::MpqArchive,
}

#[wasm_bindgen]
impl MpqFile {
    /// Open an MPQ archive from raw bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<MpqFile, JsError> {
        let inner = bw_engine::MpqArchive::from_bytes(data.to_vec())
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Read a file from the archive by path (e.g., "arr\\units.dat").
    #[wasm_bindgen(js_name = "readFile")]
    pub fn read_file(&self, name: &str) -> Result<Vec<u8>, JsError> {
        self.inner
            .read_file(name)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Check if a file exists in the archive.
    pub fn contains(&self, name: &str) -> bool {
        self.inner.contains(name)
    }

    /// List files in the archive (if it has a listfile).
    #[wasm_bindgen(js_name = "listFiles")]
    pub fn list_files(&self) -> JsValue {
        match self.inner.list_files() {
            Some(files) => serde_wasm_bindgen::to_value(&files).unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        }
    }
}

// ---------------------------------------------------------------------------
// SCX/SCM Map File
// ---------------------------------------------------------------------------

/// A loaded SCX/SCM map file.
#[wasm_bindgen]
pub struct ScxMapFile {
    inner: bw_engine::ScxMap,
}

#[wasm_bindgen]
impl ScxMapFile {
    /// Open a .scx or .scm map file from raw bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<ScxMapFile, JsError> {
        let inner = bw_engine::ScxMap::from_bytes(data.to_vec())
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Map width in tiles.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u16 {
        self.inner.terrain.width
    }

    /// Map height in tiles.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u16 {
        self.inner.terrain.height
    }

    /// Tileset index (0-7).
    #[wasm_bindgen(getter, js_name = "tilesetIndex")]
    pub fn tileset_index(&self) -> u16 {
        self.inner.tileset_index()
    }

    /// Raw CHK data for use with GameMap or GameSim constructors.
    #[wasm_bindgen(js_name = "chkData")]
    pub fn chk_data(&self) -> Vec<u8> {
        self.inner.chk_data.clone()
    }

    /// Number of preplaced units on the map.
    #[wasm_bindgen(getter, js_name = "unitCount")]
    pub fn unit_count(&self) -> usize {
        self.inner.units.len()
    }
}

// ---------------------------------------------------------------------------
// String Table (TBL)
// ---------------------------------------------------------------------------

/// A string table parsed from a TBL file (e.g., stat_txt.tbl).
#[wasm_bindgen]
pub struct TblFile {
    inner: bw_engine::StringTable,
}

#[wasm_bindgen]
impl TblFile {
    /// Parse a TBL file from raw bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<TblFile, JsError> {
        let inner =
            bw_engine::StringTable::from_bytes(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Get a string by index.
    pub fn get(&self, index: usize) -> Option<String> {
        self.inner.get(index).map(|s| s.to_string())
    }

    /// Number of strings.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.inner.len()
    }
}

// ---------------------------------------------------------------------------
// GRP Sprites
// ---------------------------------------------------------------------------

/// A parsed GRP sprite file.
#[wasm_bindgen]
pub struct GrpFile {
    inner: bw_engine::Grp,
}

#[wasm_bindgen]
impl GrpFile {
    /// Parse a GRP file from raw bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<GrpFile, JsError> {
        let inner = bw_engine::Grp::from_bytes(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Max frame width.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u16 {
        self.inner.width
    }

    /// Max frame height.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u16 {
        self.inner.height
    }

    /// Number of frames.
    #[wasm_bindgen(getter, js_name = "frameCount")]
    pub fn frame_count(&self) -> usize {
        self.inner.frame_count()
    }

    /// Get a frame's pixel data as a flat Uint8Array of palette indices.
    /// Returns null if index is out of bounds.
    #[wasm_bindgen(js_name = "framePixels")]
    pub fn frame_pixels(&self, index: usize) -> Option<Vec<u8>> {
        self.inner.frames.get(index).map(|f| f.pixels.clone())
    }

    /// Get frame metadata: [x_offset, y_offset, width, height].
    #[wasm_bindgen(js_name = "frameInfo")]
    pub fn frame_info(&self, index: usize) -> Option<Vec<u8>> {
        self.inner
            .frames
            .get(index)
            .map(|f| vec![f.x_offset, f.y_offset, f.width, f.height])
    }
}

// ---------------------------------------------------------------------------
// Tileset Palette (WPE)
// ---------------------------------------------------------------------------

/// A 256-color tileset palette.
#[wasm_bindgen]
pub struct TilesetPalette {
    inner: bw_engine::Palette,
}

#[wasm_bindgen]
impl TilesetPalette {
    /// Parse from raw WPE bytes (1024 bytes).
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<TilesetPalette, JsError> {
        let inner =
            bw_engine::Palette::from_bytes(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Get a color as [r, g, b] by palette index.
    pub fn color(&self, index: u8) -> Vec<u8> {
        let c = self.inner.color(index);
        vec![c.r, c.g, c.b]
    }

    /// Get the full palette as a flat Uint8Array of 256 x [r, g, b] = 768 bytes.
    #[wasm_bindgen(js_name = "allColors")]
    pub fn all_colors(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(768);
        for i in 0..=255u8 {
            let c = self.inner.color(i);
            out.push(c.r);
            out.push(c.g);
            out.push(c.b);
        }
        out
    }

    /// Convert a palette index to an RGBA u32 (0xRRGGBBAA).
    #[wasm_bindgen(js_name = "toRgba")]
    pub fn to_rgba(&self, index: u8) -> u32 {
        self.inner.to_rgba(index)
    }
}

// ---------------------------------------------------------------------------
// Tileset VX4 + VR4
// ---------------------------------------------------------------------------

/// Megatile → mini-tile graphic references.
#[wasm_bindgen]
pub struct TilesetVx4 {
    inner: bw_engine::Vx4Data,
}

#[wasm_bindgen]
impl TilesetVx4 {
    /// Parse from raw VX4 bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<TilesetVx4, JsError> {
        let inner =
            bw_engine::Vx4Data::from_bytes(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Number of megatile entries.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.inner.len()
    }

    /// Get the 16 VR4 indices for a megatile as a Uint16Array.
    /// Each value: index into VR4 data (bit 0 = horizontal flip).
    #[wasm_bindgen(js_name = "getMegatile")]
    pub fn get_megatile(&self, index: usize) -> Option<Vec<u16>> {
        self.inner.get(index).map(|e| e.refs.to_vec())
    }
}

/// 8x8 mini-tile pixel data.
#[wasm_bindgen]
pub struct TilesetVr4 {
    inner: bw_engine::Vr4Data,
}

#[wasm_bindgen]
impl TilesetVr4 {
    /// Parse from raw VR4 bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<TilesetVr4, JsError> {
        let inner =
            bw_engine::Vr4Data::from_bytes(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Number of mini-tile entries.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.inner.len()
    }

    /// Get the 64 palette indices for a mini-tile (8x8, row-major).
    #[wasm_bindgen(js_name = "getMiniTile")]
    pub fn get_mini_tile(&self, index: usize) -> Option<Vec<u8>> {
        self.inner.get(index).map(|e| e.pixels.to_vec())
    }
}
