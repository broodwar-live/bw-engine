//! Erlang NIF bindings for the BW replay parser via Rustler.
//!
//! All NIF functions run on dirty CPU schedulers to avoid blocking the BEAM.
//! Returns are Elixir-native terms (maps, lists, atoms) — not serialized JSON.
//!
//! ## Elixir Usage
//!
//! ```elixir
//! defmodule BroodwarNif.ReplayParser do
//!   use Rustler, otp_app: :broodwar, crate: "replay_nif"
//!
//!   def parse_replay(_data), do: :erlang.nif_error(:not_loaded)
//!   def parse_header(_data), do: :erlang.nif_error(:not_loaded)
//!   def extract_build_order(_data), do: :erlang.nif_error(:not_loaded)
//!   def calculate_apm(_data), do: :erlang.nif_error(:not_loaded)
//!   def apm_over_time(_data, _window_secs, _step_secs), do: :erlang.nif_error(:not_loaded)
//!   def detect_phases(_data), do: :erlang.nif_error(:not_loaded)
//!   def estimate_skill(_data), do: :erlang.nif_error(:not_loaded)
//!   def compare_build_orders(_data_a, _data_b, _player_index), do: :erlang.nif_error(:not_loaded)
//!   def classify_opening(_data), do: :erlang.nif_error(:not_loaded)
//!   def normalize_player_name(_name), do: :erlang.nif_error(:not_loaded)
//! end
//! ```

use rustler::{Encoder, Env, NifResult, Term};

mod atoms {
    rustler::atoms! {
        ok,
        error,

        // Engine
        starcraft,
        brood_war,

        // Speed
        slowest,
        slower,
        slow,
        normal,
        fast,
        faster,
        fastest,

        // Race
        zerg,
        terran,
        protoss,
        unknown,

        // Player type
        inactive,
        computer,
        human,
        rescue_passive,
        computer_controlled,
        open,
        neutral,
        closed,

        // Build actions
        build,
        train,
        unit_morph,
        building_morph,
        research,
        upgrade,
        train_fighter,

        // Map keys
        header,
        build_order,
        player_apm,
        timeline,
        command_count,
        map_data,
        metadata,
        engine,
        frame_count,
        duration_secs,
        start_time,
        game_title,
        map_name,
        map_name_raw,
        map_width,
        map_height,
        game_speed,
        game_type,
        host_name,
        players,
        slot_id,
        player_id,
        player_type,
        race,
        race_code,
        team,
        name,
        color,
        custom_color,
        command_warnings,
        unknown_commands,
        truncated_commands,
        skipped_bytes,
        unknown_opcodes,
        frame,
        real_seconds,
        action,
        apm,
        eapm,
        minerals_invested,
        gas_invested,
        supply_used,
        supply_max,
        units,
        buildings,
        techs,
        upgrades,

        // Metadata
        matchup,
        code,
        mirror,
        result,
        winner,
        player_name,
        is_1v1,
        player_count,

        // Phases
        phases,
        phase,
        opening,
        early_game,
        mid_game,
        late_game,
        start_frame,
        start_seconds,
        end_frame,
        end_seconds,
        landmarks,
        first_gas,
        first_tech,
        first_tier2,
        first_tier3,
        first_expansion,

        // Skill
        efficiency,
        hotkey_assigns_per_min,
        hotkey_recalls_per_min,
        apm_consistency,
        first_action_frame,
        skill_score,
        tier,
        beginner,
        intermediate,
        advanced,
        expert,
        professional,

        // Similarity
        edit_similarity,
        lcs_similarity,
        len_a,
        len_b,

        // Classification
        tag,
        confidence,
        actions_analyzed,

        // Identity
        original,
        normalized,
        clan_tag,
        canonical_name,
        aliases,
        clan_tags,
        races,
        game_count,

        // Stats report
        total_replays,
        total_1v1,
        matchup_winrates,
        map_popularity,
        race_popularity,
        matchup_durations,
        games,
        first_race_winrate,
        second_race_winrate,
        percentage,
        avg_duration_secs,
    }
}

// ---------------------------------------------------------------------------
// NIF: parse_replay
// ---------------------------------------------------------------------------

/// Parse a full replay from raw `.rep` bytes.
///
/// Returns `{:ok, replay_map}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_replay<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let map = encode_replay(env, &replay)?;
            Ok((atoms::ok(), map).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: parse_header
// ---------------------------------------------------------------------------

/// Parse only the replay header (lightweight — no command stream or build order).
///
/// Returns `{:ok, header_map}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_header<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let map = encode_header(env, &replay.header)?;
            Ok((atoms::ok(), map).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: extract_build_order
// ---------------------------------------------------------------------------

/// Parse a replay and return only the build order.
///
/// Returns `{:ok, [build_order_entry]}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn extract_build_order<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let list = encode_build_order(env, &replay.build_order)?;
            Ok((atoms::ok(), list).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: calculate_apm
// ---------------------------------------------------------------------------

/// Parse a replay and return per-player APM/EAPM.
///
/// Returns `{:ok, [%{player_id, apm, eapm}]}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn calculate_apm<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let list = encode_player_apm(env, &replay.player_apm)?;
            Ok((atoms::ok(), list).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: apm_over_time
// ---------------------------------------------------------------------------

/// Parse a replay and return APM samples over time for graphing.
///
/// Returns `{:ok, [%{frame, real_seconds, player_id, apm, eapm}]}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn apm_over_time<'a>(
    env: Env<'a>,
    data: rustler::Binary,
    window_secs: f64,
    step_secs: f64,
) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let samples = replay.apm_over_time(window_secs, step_secs);
            let list = encode_apm_samples(env, &samples)?;
            Ok((atoms::ok(), list).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: detect_phases
// ---------------------------------------------------------------------------

/// Detect game phases from a replay.
///
/// Returns `{:ok, %{phases, landmarks}}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn detect_phases<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let analysis =
                replay_core::phases::detect_phases(&replay.build_order, replay.header.frame_count);
            let map = encode_phase_analysis(env, &analysis)?;
            Ok((atoms::ok(), map).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: estimate_skill
// ---------------------------------------------------------------------------

/// Estimate player skill from a replay.
///
/// Returns `{:ok, [skill_profile]}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn estimate_skill<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let samples = replay.apm_over_time(60.0, 10.0);
            let profiles = replay_core::skill::estimate_skill(
                &replay.commands,
                &replay.player_apm,
                &samples,
                replay.header.frame_count,
            );
            let list = encode_skill_profiles(env, &profiles)?;
            Ok((atoms::ok(), list).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: compare_build_orders
// ---------------------------------------------------------------------------

/// Compare build orders from two replays for a given player index (0 or 1).
///
/// Returns `{:ok, %{edit_similarity, lcs_similarity, len_a, len_b}}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn compare_build_orders<'a>(
    env: Env<'a>,
    data_a: rustler::Binary,
    data_b: rustler::Binary,
    player_index: u8,
) -> NifResult<Term<'a>> {
    let replay_a = replay_core::parse(data_a.as_slice());
    let replay_b = replay_core::parse(data_b.as_slice());

    match (replay_a, replay_b) {
        (Ok(a), Ok(b)) => {
            let pid_a = a
                .header
                .players
                .get(player_index as usize)
                .map(|p| p.player_id)
                .unwrap_or(0);
            let pid_b = b
                .header
                .players
                .get(player_index as usize)
                .map(|p| p.player_id)
                .unwrap_or(0);

            let seq_a =
                replay_core::similarity::BuildSequence::from_build_order(&a.build_order, pid_a);
            let seq_b =
                replay_core::similarity::BuildSequence::from_build_order(&b.build_order, pid_b);
            let result = replay_core::similarity::compare(&seq_a, &seq_b);

            let map = Term::map_from_pairs(
                env,
                &[
                    (
                        atoms::edit_similarity().encode(env),
                        result.edit_similarity.encode(env),
                    ),
                    (
                        atoms::lcs_similarity().encode(env),
                        result.lcs_similarity.encode(env),
                    ),
                    (atoms::len_a().encode(env), result.len_a.encode(env)),
                    (atoms::len_b().encode(env), result.len_b.encode(env)),
                ],
            )?;
            Ok((atoms::ok(), map).encode(env))
        }
        (Err(e), _) | (_, Err(e)) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: classify_opening
// ---------------------------------------------------------------------------

/// Classify player openings from a replay.
///
/// Returns `{:ok, [%{name, tag, confidence, race, actions_analyzed}]}` or `{:error, reason}`.
#[rustler::nif(schedule = "DirtyCpu")]
fn classify_opening<'a>(env: Env<'a>, data: rustler::Binary) -> NifResult<Term<'a>> {
    match replay_core::parse(data.as_slice()) {
        Ok(replay) => {
            let players: Vec<(u8, replay_core::header::Race)> = replay
                .header
                .players
                .iter()
                .map(|p| (p.player_id, p.race))
                .collect();
            let results = replay_core::classify::classify_all(&replay.build_order, &players);
            let list = encode_classifications(env, &results)?;
            Ok((atoms::ok(), list).encode(env))
        }
        Err(e) => Ok((atoms::error(), e.to_string()).encode(env)),
    }
}

// ---------------------------------------------------------------------------
// NIF: normalize_player_name
// ---------------------------------------------------------------------------

/// Normalize a player name (strip clan tags, whitespace, etc.).
///
/// Returns `%{original, normalized, clan_tag}`.
#[rustler::nif]
fn normalize_player_name<'a>(env: Env<'a>, name: &str) -> NifResult<Term<'a>> {
    let result = replay_core::identity::normalize_name(name);
    let clan = result
        .clan_tag
        .as_deref()
        .map(|s| s.encode(env))
        .unwrap_or_else(|| rustler::types::atom::nil().encode(env));
    let map = Term::map_from_pairs(
        env,
        &[
            (
                atoms::original().encode(env),
                result.original.as_str().encode(env),
            ),
            (
                atoms::normalized().encode(env),
                result.normalized.as_str().encode(env),
            ),
            (atoms::clan_tag().encode(env), clan),
        ],
    )?;
    Ok(map)
}

// ---------------------------------------------------------------------------
// Encoding helpers — Rust types → Elixir maps/lists
// ---------------------------------------------------------------------------

fn encode_replay<'a>(env: Env<'a>, replay: &replay_core::Replay) -> NifResult<Term<'a>> {
    let header = encode_header(env, &replay.header)?;
    let build_order = encode_build_order(env, &replay.build_order)?;
    let player_apm = encode_player_apm(env, &replay.player_apm)?;
    let timeline = encode_timeline(env, &replay.timeline)?;
    let meta = encode_metadata(env, &replay.metadata)?;
    let command_count = replay.commands.len();
    let map_data = replay.map_data.as_slice().encode(env);
    let w = &replay.command_warnings;
    let command_warnings = Term::map_from_pairs(
        env,
        &[
            (
                atoms::unknown_commands().encode(env),
                w.unknown_commands.encode(env),
            ),
            (
                atoms::truncated_commands().encode(env),
                w.truncated_commands.encode(env),
            ),
            (
                atoms::skipped_bytes().encode(env),
                w.skipped_bytes.encode(env),
            ),
            (
                atoms::unknown_opcodes().encode(env),
                w.unknown_opcodes.encode(env),
            ),
        ],
    )?;

    Term::map_from_pairs(
        env,
        &[
            (atoms::header().encode(env), header),
            (atoms::build_order().encode(env), build_order),
            (atoms::player_apm().encode(env), player_apm),
            (atoms::timeline().encode(env), timeline),
            (atoms::metadata().encode(env), meta),
            (
                atoms::command_count().encode(env),
                command_count.encode(env),
            ),
            (atoms::command_warnings().encode(env), command_warnings),
            (atoms::map_data().encode(env), map_data),
        ],
    )
}

fn encode_header<'a>(env: Env<'a>, header: &replay_core::header::Header) -> NifResult<Term<'a>> {
    use replay_core::header::*;

    let engine_atom = match header.engine {
        Engine::StarCraft => atoms::starcraft().encode(env),
        Engine::BroodWar => atoms::brood_war().encode(env),
    };

    let speed = encode_speed(env, &header.game_speed);
    let game_type_str = format!("{:?}", header.game_type);
    let players_list: Vec<Term<'a>> = header
        .players
        .iter()
        .map(|p| encode_player(env, p))
        .collect::<NifResult<_>>()?;

    Term::map_from_pairs(
        env,
        &[
            (atoms::engine().encode(env), engine_atom),
            (
                atoms::frame_count().encode(env),
                header.frame_count.encode(env),
            ),
            (
                atoms::duration_secs().encode(env),
                header.duration_secs().encode(env),
            ),
            (
                atoms::start_time().encode(env),
                header.start_time.encode(env),
            ),
            (
                atoms::game_title().encode(env),
                header.game_title.as_str().encode(env),
            ),
            (
                atoms::map_name().encode(env),
                header.map_name.as_str().encode(env),
            ),
            (atoms::map_width().encode(env), header.map_width.encode(env)),
            (
                atoms::map_height().encode(env),
                header.map_height.encode(env),
            ),
            (atoms::game_speed().encode(env), speed),
            (atoms::game_type().encode(env), game_type_str.encode(env)),
            (
                atoms::host_name().encode(env),
                header.host_name.as_str().encode(env),
            ),
            (atoms::players().encode(env), players_list.encode(env)),
        ],
    )
}

fn encode_player<'a>(env: Env<'a>, player: &replay_core::header::Player) -> NifResult<Term<'a>> {
    use replay_core::header::*;

    let race_atom = match player.race {
        Race::Zerg => atoms::zerg().encode(env),
        Race::Terran => atoms::terran().encode(env),
        Race::Protoss => atoms::protoss().encode(env),
        Race::Unknown(_) => atoms::unknown().encode(env),
    };

    let type_atom = match player.player_type {
        PlayerType::Human => atoms::human().encode(env),
        PlayerType::Computer => atoms::computer().encode(env),
        PlayerType::Inactive => atoms::inactive().encode(env),
        PlayerType::RescuePassive => atoms::rescue_passive().encode(env),
        PlayerType::ComputerControlled => atoms::computer_controlled().encode(env),
        PlayerType::Open => atoms::open().encode(env),
        PlayerType::Neutral => atoms::neutral().encode(env),
        PlayerType::Closed => atoms::closed().encode(env),
        PlayerType::Unknown(_) => atoms::unknown().encode(env),
    };

    Term::map_from_pairs(
        env,
        &[
            (atoms::slot_id().encode(env), player.slot_id.encode(env)),
            (atoms::player_id().encode(env), player.player_id.encode(env)),
            (atoms::player_type().encode(env), type_atom),
            (atoms::race().encode(env), race_atom),
            (
                atoms::race_code().encode(env),
                player.race.code().encode(env),
            ),
            (atoms::team().encode(env), player.team.encode(env)),
            (atoms::name().encode(env), player.name.as_str().encode(env)),
            (atoms::color().encode(env), player.color.encode(env)),
            (
                atoms::custom_color().encode(env),
                player
                    .custom_color
                    .map(|c| c.encode(env))
                    .unwrap_or_else(|| rustler::types::atom::nil().encode(env)),
            ),
        ],
    )
}

fn encode_speed<'a>(env: Env<'a>, speed: &replay_core::header::Speed) -> Term<'a> {
    use replay_core::header::Speed;
    match speed {
        Speed::Slowest => atoms::slowest().encode(env),
        Speed::Slower => atoms::slower().encode(env),
        Speed::Slow => atoms::slow().encode(env),
        Speed::Normal => atoms::normal().encode(env),
        Speed::Fast => atoms::fast().encode(env),
        Speed::Faster => atoms::faster().encode(env),
        Speed::Fastest => atoms::fastest().encode(env),
        Speed::Unknown(v) => v.encode(env),
    }
}

fn encode_build_order<'a>(
    env: Env<'a>,
    entries: &[replay_core::analysis::BuildOrderEntry],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = entries
        .iter()
        .map(|entry| {
            let action_term = encode_build_action(env, &entry.action);
            Term::map_from_pairs(
                env,
                &[
                    (atoms::frame().encode(env), entry.frame.encode(env)),
                    (
                        atoms::real_seconds().encode(env),
                        entry.real_seconds.encode(env),
                    ),
                    (atoms::player_id().encode(env), entry.player_id.encode(env)),
                    (atoms::action().encode(env), action_term),
                    (atoms::name().encode(env), entry.action.name().encode(env)),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

fn encode_build_action<'a>(env: Env<'a>, action: &replay_core::analysis::BuildAction) -> Term<'a> {
    use replay_core::analysis::BuildAction;
    match action {
        BuildAction::Build(id) => (atoms::build(), *id).encode(env),
        BuildAction::Train(id) => (atoms::train(), *id).encode(env),
        BuildAction::UnitMorph(id) => (atoms::unit_morph(), *id).encode(env),
        BuildAction::BuildingMorph(id) => (atoms::building_morph(), *id).encode(env),
        BuildAction::Research(id) => (atoms::research(), *id as u16).encode(env),
        BuildAction::Upgrade(id) => (atoms::upgrade(), *id as u16).encode(env),
        BuildAction::TrainFighter => atoms::train_fighter().encode(env),
    }
}

fn encode_player_apm<'a>(
    env: Env<'a>,
    apms: &[replay_core::analysis::PlayerApm],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = apms
        .iter()
        .map(|a| {
            Term::map_from_pairs(
                env,
                &[
                    (atoms::player_id().encode(env), a.player_id.encode(env)),
                    (atoms::apm().encode(env), a.apm.encode(env)),
                    (atoms::eapm().encode(env), a.eapm.encode(env)),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

fn encode_apm_samples<'a>(
    env: Env<'a>,
    samples: &[replay_core::analysis::ApmSample],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = samples
        .iter()
        .map(|s| {
            Term::map_from_pairs(
                env,
                &[
                    (atoms::frame().encode(env), s.frame.encode(env)),
                    (
                        atoms::real_seconds().encode(env),
                        s.real_seconds.encode(env),
                    ),
                    (atoms::player_id().encode(env), s.player_id.encode(env)),
                    (atoms::apm().encode(env), s.apm.encode(env)),
                    (atoms::eapm().encode(env), s.eapm.encode(env)),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

fn encode_timeline<'a>(
    env: Env<'a>,
    snapshots: &[replay_core::timeline::TimelineSnapshot],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = snapshots
        .iter()
        .map(|snap| {
            let player_states: Vec<Term<'a>> = snap
                .players
                .iter()
                .map(|ps| encode_player_state(env, ps))
                .collect::<NifResult<_>>()?;

            Term::map_from_pairs(
                env,
                &[
                    (atoms::frame().encode(env), snap.frame.encode(env)),
                    (
                        atoms::real_seconds().encode(env),
                        snap.real_seconds.encode(env),
                    ),
                    (atoms::players().encode(env), player_states.encode(env)),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

fn encode_player_state<'a>(
    env: Env<'a>,
    state: &replay_core::timeline::PlayerState,
) -> NifResult<Term<'a>> {
    // Convert BTreeMaps to lists of {key, value} tuples for Elixir.
    let units_list: Vec<(u16, u32)> = state.units.iter().map(|(&k, &v)| (k, v)).collect();
    let buildings_list: Vec<(u16, u32)> = state.buildings.iter().map(|(&k, &v)| (k, v)).collect();
    let upgrades_list: Vec<(u8, u8)> = state.upgrades.iter().map(|(&k, &v)| (k, v)).collect();

    Term::map_from_pairs(
        env,
        &[
            (atoms::player_id().encode(env), state.player_id.encode(env)),
            (
                atoms::minerals_invested().encode(env),
                state.minerals_invested.encode(env),
            ),
            (
                atoms::gas_invested().encode(env),
                state.gas_invested.encode(env),
            ),
            (
                atoms::supply_used().encode(env),
                state.supply_used.encode(env),
            ),
            (
                atoms::supply_max().encode(env),
                state.supply_max.encode(env),
            ),
            (atoms::units().encode(env), units_list.encode(env)),
            (atoms::buildings().encode(env), buildings_list.encode(env)),
            (atoms::techs().encode(env), state.techs.encode(env)),
            (atoms::upgrades().encode(env), upgrades_list.encode(env)),
        ],
    )
}

// ---------------------------------------------------------------------------
// Metadata encoding
// ---------------------------------------------------------------------------

fn encode_metadata<'a>(
    env: Env<'a>,
    meta: &replay_core::metadata::GameMetadata,
) -> NifResult<Term<'a>> {
    use replay_core::metadata::GameResult;

    let matchup_term = match &meta.matchup {
        Some(m) => Term::map_from_pairs(
            env,
            &[
                (atoms::code().encode(env), m.code.as_str().encode(env)),
                (atoms::mirror().encode(env), m.mirror.encode(env)),
            ],
        )?,
        None => rustler::types::atom::nil().encode(env),
    };

    let result_term = match &meta.result {
        GameResult::Winner {
            player_id,
            player_name,
        } => Term::map_from_pairs(
            env,
            &[
                (atoms::result().encode(env), atoms::winner().encode(env)),
                (atoms::player_id().encode(env), player_id.encode(env)),
                (
                    atoms::player_name().encode(env),
                    player_name.as_str().encode(env),
                ),
            ],
        )?,
        GameResult::Unknown => atoms::unknown().encode(env),
    };

    Term::map_from_pairs(
        env,
        &[
            (atoms::matchup().encode(env), matchup_term),
            (
                atoms::map_name().encode(env),
                meta.map_name.as_str().encode(env),
            ),
            (
                atoms::map_name_raw().encode(env),
                meta.map_name_raw.as_str().encode(env),
            ),
            (atoms::result().encode(env), result_term),
            (
                atoms::duration_secs().encode(env),
                meta.duration_secs.encode(env),
            ),
            (atoms::is_1v1().encode(env), meta.is_1v1.encode(env)),
            (
                atoms::player_count().encode(env),
                meta.player_count.encode(env),
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// Phase encoding
// ---------------------------------------------------------------------------

fn encode_phase_analysis<'a>(
    env: Env<'a>,
    analysis: &replay_core::phases::PhaseAnalysis,
) -> NifResult<Term<'a>> {
    let phases_list: Vec<Term<'a>> = analysis
        .phases
        .iter()
        .map(|p| {
            let phase_atom = match p.phase {
                replay_core::phases::Phase::Opening => atoms::opening().encode(env),
                replay_core::phases::Phase::EarlyGame => atoms::early_game().encode(env),
                replay_core::phases::Phase::MidGame => atoms::mid_game().encode(env),
                replay_core::phases::Phase::LateGame => atoms::late_game().encode(env),
            };
            let end_f = p
                .end_frame
                .map(|f| f.encode(env))
                .unwrap_or_else(|| rustler::types::atom::nil().encode(env));
            let end_s = p
                .end_seconds
                .map(|s| s.encode(env))
                .unwrap_or_else(|| rustler::types::atom::nil().encode(env));
            Term::map_from_pairs(
                env,
                &[
                    (atoms::phase().encode(env), phase_atom),
                    (atoms::start_frame().encode(env), p.start_frame.encode(env)),
                    (
                        atoms::start_seconds().encode(env),
                        p.start_seconds.encode(env),
                    ),
                    (atoms::end_frame().encode(env), end_f),
                    (atoms::end_seconds().encode(env), end_s),
                ],
            )
        })
        .collect::<NifResult<_>>()?;

    let lm = &analysis.landmarks;
    let nil = || rustler::types::atom::nil().encode(env);
    let opt_u32 = |v: Option<u32>| v.map(|f| f.encode(env)).unwrap_or_else(nil);

    let landmarks_map = Term::map_from_pairs(
        env,
        &[
            (atoms::first_gas().encode(env), opt_u32(lm.first_gas)),
            (atoms::first_tech().encode(env), opt_u32(lm.first_tech)),
            (atoms::first_tier2().encode(env), opt_u32(lm.first_tier2)),
            (atoms::first_tier3().encode(env), opt_u32(lm.first_tier3)),
            (
                atoms::first_expansion().encode(env),
                opt_u32(lm.first_expansion),
            ),
        ],
    )?;

    Term::map_from_pairs(
        env,
        &[
            (atoms::phases().encode(env), phases_list.encode(env)),
            (atoms::landmarks().encode(env), landmarks_map),
        ],
    )
}

// ---------------------------------------------------------------------------
// Skill encoding
// ---------------------------------------------------------------------------

fn encode_skill_profiles<'a>(
    env: Env<'a>,
    profiles: &[replay_core::skill::SkillProfile],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = profiles
        .iter()
        .map(|p| {
            let tier_atom = match p.tier {
                replay_core::skill::SkillTier::Beginner => atoms::beginner().encode(env),
                replay_core::skill::SkillTier::Intermediate => atoms::intermediate().encode(env),
                replay_core::skill::SkillTier::Advanced => atoms::advanced().encode(env),
                replay_core::skill::SkillTier::Expert => atoms::expert().encode(env),
                replay_core::skill::SkillTier::Professional => atoms::professional().encode(env),
            };
            let first_action = p
                .first_action_frame
                .map(|f| f.encode(env))
                .unwrap_or_else(|| rustler::types::atom::nil().encode(env));

            Term::map_from_pairs(
                env,
                &[
                    (atoms::player_id().encode(env), p.player_id.encode(env)),
                    (atoms::apm().encode(env), p.apm.encode(env)),
                    (atoms::eapm().encode(env), p.eapm.encode(env)),
                    (atoms::efficiency().encode(env), p.efficiency.encode(env)),
                    (
                        atoms::hotkey_assigns_per_min().encode(env),
                        p.hotkey_assigns_per_min.encode(env),
                    ),
                    (
                        atoms::hotkey_recalls_per_min().encode(env),
                        p.hotkey_recalls_per_min.encode(env),
                    ),
                    (
                        atoms::apm_consistency().encode(env),
                        p.apm_consistency.encode(env),
                    ),
                    (atoms::first_action_frame().encode(env), first_action),
                    (atoms::skill_score().encode(env), p.skill_score.encode(env)),
                    (atoms::tier().encode(env), tier_atom),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

// ---------------------------------------------------------------------------
// Classification encoding
// ---------------------------------------------------------------------------

fn encode_classifications<'a>(
    env: Env<'a>,
    results: &[replay_core::classify::OpeningClassification],
) -> NifResult<Term<'a>> {
    let list: Vec<Term<'a>> = results
        .iter()
        .map(|c| {
            Term::map_from_pairs(
                env,
                &[
                    (atoms::name().encode(env), c.name.as_str().encode(env)),
                    (atoms::tag().encode(env), c.tag.as_str().encode(env)),
                    (atoms::confidence().encode(env), c.confidence.encode(env)),
                    (atoms::race().encode(env), c.race.as_str().encode(env)),
                    (
                        atoms::actions_analyzed().encode(env),
                        c.actions_analyzed.encode(env),
                    ),
                ],
            )
        })
        .collect::<NifResult<_>>()?;
    Ok(list.encode(env))
}

// ---------------------------------------------------------------------------
// Module init
// ---------------------------------------------------------------------------

rustler::init!("Elixir.BroodwarNif.ReplayParser");
