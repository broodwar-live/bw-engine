use wasm_bindgen::prelude::*;

/// Parse a `.rep` file and return the full replay as a JS object.
///
/// Returns an object with: header, commands, build_order, player_apm, timeline, map_data.
#[wasm_bindgen(js_name = "parseReplay")]
pub fn parse_replay(data: &[u8]) -> Result<JsValue, JsError> {
    let replay = replay_core::parse(data).map_err(|e| JsError::new(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&replay).map_err(|e| JsError::new(&e.to_string()))
}

#[cfg(feature = "map")]
mod assets;
#[cfg(feature = "sim")]
mod sim;

#[cfg(feature = "map")]
pub use assets::*;
#[cfg(feature = "sim")]
pub use sim::*;
