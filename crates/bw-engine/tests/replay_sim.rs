//! End-to-end simulation tests against real replay fixtures.
//!
//! These tests validate that the full pipeline (parse replay → extract CHK →
//! build regions → process commands → simulate) works without crashing on
//! real game data. They use minimal/synthetic .dat data since real BW data
//! files are not included in the repo.

use bw_engine::chk;
use bw_engine::chk_units;
use bw_engine::dat::{FlingyType, GameData, UnitType, WeaponType};
use bw_engine::game::{EngineCommand, Game};
use bw_engine::map::Map;
use bw_engine::tile::MiniTile;
use bw_engine::tileset::{CV5_ENTRY_SIZE, VF4_ENTRY_SIZE};

fn fixture(name: &str) -> Vec<u8> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest_dir}/../../tests/fixtures/{name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// Build minimal game data with reasonable defaults for all 228 unit types.
fn synthetic_game_data() -> GameData {
    let default_flingy = FlingyType {
        top_speed: 4 * 256,  // ~4 px/frame
        acceleration: 256,
        halt_distance: 0,
        turn_rate: 40,
        movement_type: 0,
    };
    let flingy_types = vec![default_flingy; 209];

    let default_unit = UnitType {
        flingy_id: 0,
        hitpoints: 40 * 256,
        ground_weapon: 0,
        max_ground_hits: 1,
        air_weapon: 130,
        max_air_hits: 0,
        armor: 0,
        build_time: 100,
        is_building: false,
    };
    let building_unit = UnitType {
        is_building: true,
        hitpoints: 1000 * 256,
        ground_weapon: 130,
        ..default_unit
    };

    let mut unit_types = vec![default_unit; 228];
    // Mark buildings (roughly unit types 106-202 in BW).
    for i in 106..=202 {
        unit_types[i] = building_unit;
    }

    let default_weapon = WeaponType {
        damage_amount: 6,
        damage_bonus: 0,
        cooldown: 15,
        damage_factor: 1,
        max_range: 128,
    };
    let weapon_types = vec![default_weapon; 130];

    GameData {
        flingy_types,
        unit_types,
        weapon_types,
    }
}

/// Build a minimal all-walkable map from CHK dimensions.
fn synthetic_map(chk_data: &[u8]) -> Map {
    let sections = chk::parse_sections(chk_data).unwrap();
    let terrain = chk::extract_terrain(&sections).unwrap();
    let w = terrain.width;
    let h = terrain.height;

    // All-walkable VF4/CV5.
    let mut vf4 = vec![0u8; VF4_ENTRY_SIZE];
    let walkable_flags = [MiniTile::WALKABLE; 16];
    for (j, &f) in walkable_flags.iter().enumerate() {
        vf4[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
    }
    let mut cv5 = vec![0u8; CV5_ENTRY_SIZE];
    cv5[20..22].copy_from_slice(&0u16.to_le_bytes());

    // Rebuild CHK with the real dimensions but all-walkable tiles.
    let mut chk = Vec::new();
    chk.extend_from_slice(b"DIM ");
    chk.extend_from_slice(&4u32.to_le_bytes());
    chk.extend_from_slice(&w.to_le_bytes());
    chk.extend_from_slice(&h.to_le_bytes());
    chk.extend_from_slice(b"ERA ");
    chk.extend_from_slice(&2u32.to_le_bytes());
    chk.extend_from_slice(&terrain.tileset_index.to_le_bytes());
    let mtxm = vec![0u8; w as usize * h as usize * 2];
    chk.extend_from_slice(b"MTXM");
    chk.extend_from_slice(&(mtxm.len() as u32).to_le_bytes());
    chk.extend_from_slice(&mtxm);

    Map::from_chk(&chk, &cv5, &vf4).unwrap()
}

/// Translate a replay_core::Command to EngineCommand.
fn translate(cmd: &replay_core::command::Command) -> Option<EngineCommand> {
    use replay_core::command::{Command, HotkeyAction};

    match cmd {
        Command::Select { unit_tags } => Some(EngineCommand::Select(unit_tags.clone())),
        Command::SelectAdd { unit_tags } => Some(EngineCommand::SelectAdd(unit_tags.clone())),
        Command::SelectRemove { unit_tags } => {
            Some(EngineCommand::SelectRemove(unit_tags.clone()))
        }
        Command::Hotkey { action, group } => match action {
            HotkeyAction::Assign => Some(EngineCommand::HotkeyAssign { group: *group }),
            HotkeyAction::Select => Some(EngineCommand::HotkeyRecall { group: *group }),
        },
        Command::RightClick {
            x, y, target_tag, ..
        } => {
            if *target_tag == 0 || *target_tag == 0xFFFF {
                Some(EngineCommand::Move { x: *x, y: *y })
            } else {
                Some(EngineCommand::Attack {
                    target_tag: *target_tag,
                })
            }
        }
        Command::TargetedOrder {
            x, y, order, target_tag, ..
        } => {
            if *order == 0x06 {
                Some(EngineCommand::Move { x: *x, y: *y })
            } else if *target_tag != 0 && *target_tag != 0xFFFF {
                Some(EngineCommand::Attack {
                    target_tag: *target_tag,
                })
            } else {
                None
            }
        }
        Command::Stop { .. } => Some(EngineCommand::Stop),
        Command::Train { unit_type } => Some(EngineCommand::Train {
            unit_type: *unit_type,
        }),
        _ => None,
    }
}

/// Run the full simulation for a replay file and return statistics.
struct SimResult {
    map_name: String,
    map_width: u16,
    map_height: u16,
    total_frames: u32,
    total_commands: usize,
    translated_commands: usize,
    initial_units: usize,
    final_units: usize,
    peak_units: usize,
}

fn run_sim(replay_name: &str) -> SimResult {
    let rep_data = fixture(replay_name);
    let replay = replay_core::parse(&rep_data).expect("failed to parse replay");

    let map = synthetic_map(&replay.map_data);
    let data = synthetic_game_data();
    let mut game = Game::new(map, data);

    // Load initial units from CHK.
    let sections = chk::parse_sections(&replay.map_data).unwrap();
    let chk_units = chk_units::parse_chk_units(&sections).unwrap();
    game.load_initial_units(&chk_units).unwrap();
    let initial_units = game.unit_count();

    // Process all commands frame by frame.
    let mut cmd_idx = 0;
    let mut translated = 0;
    let mut peak_units = initial_units;

    let total_frames = replay.header.frame_count;
    for target_frame in 1..=total_frames {
        // Apply commands for this frame.
        while cmd_idx < replay.commands.len() && replay.commands[cmd_idx].frame <= target_frame {
            let gc = &replay.commands[cmd_idx];
            if gc.frame == target_frame {
                if let Some(cmd) = translate(&gc.command) {
                    game.apply_command(gc.player_id, &cmd);
                    translated += 1;
                }
            }
            cmd_idx += 1;
        }
        game.step();

        let count = game.unit_count();
        if count > peak_units {
            peak_units = count;
        }
    }

    SimResult {
        map_name: replay.header.map_name.clone(),
        map_width: replay.header.map_width,
        map_height: replay.header.map_height,
        total_frames,
        total_commands: replay.commands.len(),
        translated_commands: translated,
        initial_units,
        final_units: game.unit_count(),
        peak_units,
    }
}

#[test]
fn test_sim_1v1_melee() {
    let r = run_sim("1v1melee.rep");
    println!("=== 1v1melee.rep simulation ===");
    println!("  Map: {} ({}x{})", r.map_name, r.map_width, r.map_height);
    println!("  Frames: {}", r.total_frames);
    println!(
        "  Commands: {} total, {} translated",
        r.total_commands, r.translated_commands
    );
    println!(
        "  Units: {} initial → {} final (peak {})",
        r.initial_units, r.final_units, r.peak_units
    );

    assert!(r.total_frames > 0);
    assert!(r.initial_units > 0, "map should have preplaced units");
    assert!(
        r.translated_commands > 0,
        "should have some translated commands"
    );
}

#[test]
fn test_sim_larva_vs_mini() {
    let r = run_sim("larva_vs_mini.rep");
    println!("=== larva_vs_mini.rep simulation ===");
    println!("  Map: {} ({}x{})", r.map_name, r.map_width, r.map_height);
    println!("  Frames: {}", r.total_frames);
    println!(
        "  Commands: {} total, {} translated",
        r.total_commands, r.translated_commands
    );
    println!(
        "  Units: {} initial → {} final (peak {})",
        r.initial_units, r.final_units, r.peak_units
    );

    assert!(r.total_frames > 1000, "real game should be >1000 frames");
    assert!(r.initial_units > 0);
    assert!(
        r.translated_commands > 50,
        "competitive game should have many translated commands"
    );
    // Note: with synthetic data, unit tags don't match BW's assignment,
    // so Select/Train commands often target the wrong units. Production
    // only works correctly with real .dat files and matching tag assignment.
    // For now, just verify the sim completes without crashing.
}

#[test]
fn test_sim_polypoid() {
    let r = run_sim("polypoid.rep");
    println!("=== polypoid.rep simulation ===");
    println!("  Map: {} ({}x{})", r.map_name, r.map_width, r.map_height);
    println!("  Frames: {}", r.total_frames);
    println!(
        "  Commands: {} total, {} translated",
        r.total_commands, r.translated_commands
    );
    println!(
        "  Units: {} initial → {} final (peak {})",
        r.initial_units, r.final_units, r.peak_units
    );

    assert!(r.total_frames > 0);
    assert!(r.initial_units > 0);
}

#[test]
fn test_sim_legacy_centauro() {
    let r = run_sim("centauro_vs_djscan.rep");
    println!("=== centauro_vs_djscan.rep (legacy) simulation ===");
    println!("  Map: {} ({}x{})", r.map_name, r.map_width, r.map_height);
    println!("  Frames: {}", r.total_frames);
    println!(
        "  Commands: {} total, {} translated",
        r.total_commands, r.translated_commands
    );
    println!(
        "  Units: {} initial → {} final (peak {})",
        r.initial_units, r.final_units, r.peak_units
    );

    assert!(r.total_frames > 0);
    assert!(r.initial_units > 0);
    assert!(r.translated_commands > 50);
}

#[test]
fn test_sim_legacy_franky() {
    let r = run_sim("franky_vs_djscan.rep");
    println!("=== franky_vs_djscan.rep (legacy) simulation ===");
    println!("  Map: {} ({}x{})", r.map_name, r.map_width, r.map_height);
    println!("  Frames: {}", r.total_frames);
    println!(
        "  Commands: {} total, {} translated",
        r.total_commands, r.translated_commands
    );
    println!(
        "  Units: {} initial → {} final (peak {})",
        r.initial_units, r.final_units, r.peak_units
    );

    assert!(r.total_frames > 0);
    assert!(r.initial_units > 0);
}

/// Verify CHK UNIT section can be extracted from all replay fixtures.
#[test]
fn test_chk_unit_extraction() {
    for name in [
        "1v1melee.rep",
        "larva_vs_mini.rep",
        "polypoid.rep",
        "centauro_vs_djscan.rep",
        "franky_vs_djscan.rep",
    ] {
        let rep_data = fixture(name);
        let replay = replay_core::parse(&rep_data).unwrap();
        assert!(
            !replay.map_data.is_empty(),
            "{name}: map_data should not be empty"
        );

        let sections = chk::parse_sections(&replay.map_data).unwrap();
        let terrain = chk::extract_terrain(&sections).unwrap();
        assert!(terrain.width > 0 && terrain.height > 0, "{name}: invalid map dimensions");

        let units = chk_units::parse_chk_units(&sections).unwrap();
        println!(
            "{name}: map {}x{}, tileset {}, {} CHK units",
            terrain.width, terrain.height, terrain.tileset_index, units.len()
        );
        assert!(!units.is_empty(), "{name}: should have preplaced units");

        // Verify unit data is reasonable.
        for u in &units {
            assert!(
                u.unit_type < 228,
                "{name}: unit type {} out of range",
                u.unit_type
            );
            assert!(u.owner < 12, "{name}: owner {} out of range", u.owner);
            assert!(
                u.x < terrain.width * 32 + 32 && u.y < terrain.height * 32 + 32,
                "{name}: unit at ({}, {}) outside map bounds",
                u.x,
                u.y
            );
        }
    }
}
