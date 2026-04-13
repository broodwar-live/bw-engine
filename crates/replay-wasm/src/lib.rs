use wasm_bindgen::prelude::*;

/// Parse a `.rep` file and return the full replay as a JS object.
///
/// Returns an object with: header, commands, build_order, player_apm, timeline.
/// The `map_data` field contains the raw CHK bytes as a Uint8Array.
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
    /// Parse a map from raw CHK data and tileset files.
    ///
    /// - `chk_data`: raw CHK bytes (from replay's `map_data` or a `.scm`/`.scx` file).
    /// - `cv5_data`: raw bytes of the tileset's `.cv5` file.
    /// - `vf4_data`: raw bytes of the tileset's `.vf4` file.
    #[wasm_bindgen(constructor)]
    pub fn new(chk_data: &[u8], cv5_data: &[u8], vf4_data: &[u8]) -> Result<GameMap, JsError> {
        let inner =
            bw_engine::Map::from_chk(chk_data, cv5_data, vf4_data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// Map width in 32px tiles.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u16 {
        self.inner.width()
    }

    /// Map height in 32px tiles.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u16 {
        self.inner.height()
    }

    /// Map width in pixels.
    #[wasm_bindgen(getter, js_name = "widthPx")]
    pub fn width_px(&self) -> u32 {
        self.inner.width_px()
    }

    /// Map height in pixels.
    #[wasm_bindgen(getter, js_name = "heightPx")]
    pub fn height_px(&self) -> u32 {
        self.inner.height_px()
    }

    /// Tileset name (e.g. "Badlands", "Jungle").
    #[wasm_bindgen(getter)]
    pub fn tileset(&self) -> String {
        self.inner.tileset().name().to_string()
    }

    /// Whether the mini-tile at walk-grid position (mx, my) is walkable.
    /// The walk grid is 4x the tile grid (each tile = 4x4 mini-tiles of 8px).
    #[wasm_bindgen(js_name = "isWalkable")]
    pub fn is_walkable(&self, mx: u16, my: u16) -> bool {
        self.inner.is_walkable(mx, my)
    }

    /// Ground height (0=Low, 1=Middle, 2=High, 3=VeryHigh) at walk-grid position.
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

    /// Whether the pixel position (px, py) is walkable.
    #[wasm_bindgen(js_name = "isWalkablePx")]
    pub fn is_walkable_px(&self, px: u32, py: u32) -> bool {
        self.inner.is_walkable_px(px, py)
    }

    /// Walkability grid as a flat Uint8Array (1=walkable, 0=unwalkable).
    /// Row-major, dimensions: (width*4) x (height*4) mini-tiles.
    #[wasm_bindgen(js_name = "walkabilityGrid")]
    pub fn walkability_grid(&self) -> Vec<u8> {
        self.inner
            .mini_tiles()
            .iter()
            .map(|mt| mt.is_walkable() as u8)
            .collect()
    }

    /// Height grid as a flat Uint8Array (0-3 per mini-tile).
    /// Row-major, dimensions: (width*4) x (height*4) mini-tiles.
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
