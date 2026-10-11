use replay_core::analysis::{ApmSample, BuildOrderEntry, PlayerApm};
use replay_core::classify::OpeningClassification;
use replay_core::command::{CommandParseWarnings, GameCommand};
use replay_core::error::ReplayError;
use replay_core::extra::ExtraSections;
use replay_core::header::Header;
use replay_core::metadata::GameMetadata;
use replay_core::phases::PhaseAnalysis;
use replay_core::production::ProductionEntry;
use replay_core::timeline::TimelineSnapshot;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Options for [`parse_replay`]. All fields are optional (camelCase in JS).
#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ParseOptions {
    /// Include the raw CHK map data as a `Uint8Array` (`map_data`). Off by
    /// default: it is large and only needed for map rendering/simulation.
    map_data: bool,
    /// Include the full command stream (`commands`). On by default.
    commands: bool,
    /// Include the per-snapshot `timeline`. On by default.
    timeline: bool,
    /// APM curve sliding window in real seconds (default 60).
    apm_window_secs: f64,
    /// APM curve sample interval in real seconds (default 10).
    apm_step_secs: f64,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            map_data: false,
            commands: true,
            timeline: true,
            apm_window_secs: 60.0,
            apm_step_secs: 10.0,
        }
    }
}

/// Raw bytes, serialized as a `Uint8Array` rather than an array of numbers.
struct Bytes<'a>(&'a [u8]);

impl Serialize for Bytes<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(self.0)
    }
}

/// What `parseReplay` returns: the parsed replay plus the analyses computed
/// by replay-core, so pages do not need to reimplement them.
#[derive(Serialize)]
struct ReplayView<'a> {
    header: &'a Header,
    #[serde(skip_serializing_if = "Option::is_none")]
    commands: Option<&'a [GameCommand]>,
    command_warnings: &'a CommandParseWarnings,
    build_order: &'a [BuildOrderEntry],
    production: &'a [ProductionEntry],
    player_apm: &'a [PlayerApm],
    #[serde(skip_serializing_if = "Option::is_none")]
    timeline: Option<&'a [TimelineSnapshot]>,
    metadata: &'a GameMetadata,
    extra_sections: &'a ExtraSections,
    openings: Vec<OpeningClassification>,
    phases: PhaseAnalysis,
    apm_curve: Vec<ApmSample>,
    #[serde(skip_serializing_if = "Option::is_none")]
    map_data: Option<Bytes<'a>>,
}

/// Convert a parse error into a JS `Error` with a readable `message` and a
/// stable `code` property (`NOT_A_REPLAY`, `TRUNCATED`, `CORRUPT`,
/// `UNSUPPORTED`) for mapping to localized messages.
fn replay_error(e: &ReplayError) -> JsValue {
    let err = js_sys::Error::new(&e.to_string());
    err.set_name("ReplayError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = js_sys::Reflect::set(&err, &"code".into(), &e.code().into());
    err.into()
}

fn js_error(message: &str) -> JsValue {
    js_sys::Error::new(message).into()
}

/// Parse a `.rep` file and return the replay plus derived analyses.
///
/// `options` (optional): `{ mapData, commands, timeline, apmWindowSecs,
/// apmStepSecs }`, see `ParseOptions`.
///
/// Returns an object with: header, commands, command_warnings, build_order,
/// production (build order with modelled supply/larvae/dropped orders),
/// player_apm, timeline, metadata, extra_sections, openings, phases,
/// apm_curve, and `map_data` (a `Uint8Array`) only when `mapData: true`.
///
/// Throws an `Error` with a `code` property if the file cannot be parsed.
#[wasm_bindgen(js_name = "parseReplay")]
pub fn parse_replay(data: &[u8], options: JsValue) -> Result<JsValue, JsValue> {
    let opts: ParseOptions = if options.is_undefined() || options.is_null() {
        ParseOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| js_error(&format!("invalid parseReplay options: {e}")))?
    };
    let replay = replay_core::parse(data).map_err(|e| replay_error(&e))?;
    build_view(&replay, &opts)
        .serialize(&serde_wasm_bindgen::Serializer::new())
        .map_err(|e| js_error(&e.to_string()))
}

fn build_view<'a>(replay: &'a replay_core::Replay, opts: &ParseOptions) -> ReplayView<'a> {
    ReplayView {
        header: &replay.header,
        commands: opts.commands.then_some(replay.commands.as_slice()),
        command_warnings: &replay.command_warnings,
        build_order: &replay.build_order,
        production: &replay.production,
        player_apm: &replay.player_apm,
        timeline: opts.timeline.then_some(replay.timeline.as_slice()),
        metadata: &replay.metadata,
        extra_sections: &replay.extra_sections,
        openings: replay.openings(),
        phases: replay.phases(),
        apm_curve: replay.apm_over_time(opts.apm_window_secs, opts.apm_step_secs),
        map_data: opts.map_data.then_some(Bytes(&replay.map_data)),
    }
}

/// Return only the raw CHK map data of a replay, as a `Uint8Array`.
#[wasm_bindgen(js_name = "getMapData")]
pub fn get_map_data(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    replay_core::parse(data)
        .map(|r| r.map_data)
        .map_err(|e| replay_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> replay_core::Replay {
        let path = format!(
            "{}/../../tests/fixtures/larva_vs_mini.rep",
            env!("CARGO_MANIFEST_DIR")
        );
        replay_core::parse(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn test_view_default_options_exclude_map_data_include_analyses() {
        let replay = fixture();
        let view = build_view(&replay, &ParseOptions::default());
        assert!(view.map_data.is_none());
        assert!(view.commands.is_some());
        assert!(view.timeline.is_some());
        assert_eq!(view.openings.len(), 2);
        assert!(!view.phases.phases.is_empty());
        assert!(!view.apm_curve.is_empty());
        assert_eq!(view.production.len(), view.build_order.len());
    }

    #[test]
    fn test_view_options_toggle_sections() {
        let replay = fixture();
        let opts = ParseOptions {
            map_data: true,
            commands: false,
            timeline: false,
            apm_window_secs: 60.0,
            apm_step_secs: 30.0,
        };
        let view = build_view(&replay, &opts);
        assert_eq!(
            view.map_data.map(|b| b.0.len()),
            Some(replay.map_data.len())
        );
        assert!(view.commands.is_none());
        assert!(view.timeline.is_none());
        let default_curve = build_view(&replay, &ParseOptions::default()).apm_curve;
        assert!(view.apm_curve.len() * 2 < default_curve.len());
    }
}

#[cfg(feature = "map")]
mod assets;
#[cfg(feature = "sim")]
mod sim;

#[cfg(feature = "map")]
pub use assets::*;
#[cfg(feature = "sim")]
pub use sim::*;
