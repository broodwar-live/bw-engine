//! Validation tests using real BW data files extracted from SC:R CASC.
//!
//! These tests are ignored by default (require /tmp/bw-data/).
//! Run with: cargo test -p bw-engine --test real_data -- --ignored --nocapture

use bw_engine::chk;
use bw_engine::chk_units;
use bw_engine::dat::GameData;
use bw_engine::game::{EngineCommand, Game};
use bw_engine::map::Map;

fn bw_data(name: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("/tmp/bw-data/{name}")).ok()
}

fn fixture(name: &str) -> Vec<u8> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest_dir}/../../tests/fixtures/{name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

fn tileset_files(index: u16) -> Option<(Vec<u8>, Vec<u8>)> {
    let name = match index % 8 {
        0 => "badlands",
        1 => "platform",
        2 => "install",
        3 => "ashworld",
        4 => "jungle",
        5 => "desert",
        6 => "ice",
        7 => "twilight",
        _ => unreachable!(),
    };
    let cv5 = bw_data(&format!("{name}.cv5"))?;
    let vf4 = bw_data(&format!("{name}.vf4"))?;
    Some((cv5, vf4))
}

fn translate(cmd: &replay_core::command::Command) -> Option<EngineCommand> {
    use replay_core::command::{Command, HotkeyAction};
    match cmd {
        Command::Select { unit_tags } => Some(EngineCommand::Select(unit_tags.clone())),
        Command::SelectAdd { unit_tags } => Some(EngineCommand::SelectAdd(unit_tags.clone())),
        Command::SelectRemove { unit_tags } => Some(EngineCommand::SelectRemove(unit_tags.clone())),
        Command::Hotkey { action, group } => match action {
            HotkeyAction::Assign => Some(EngineCommand::HotkeyAssign { group: *group }),
            HotkeyAction::Select => Some(EngineCommand::HotkeyRecall { group: *group }),
        },
        Command::RightClick { x, y, target_tag, .. } => {
            if *target_tag == 0 || *target_tag == 0xFFFF {
                Some(EngineCommand::Move { x: *x, y: *y })
            } else {
                Some(EngineCommand::Attack { target_tag: *target_tag })
            }
        }
        Command::TargetedOrder { x, y, order, target_tag, .. } => {
            if *order == 0x06 {
                Some(EngineCommand::Move { x: *x, y: *y })
            } else if *target_tag != 0 && *target_tag != 0xFFFF {
                Some(EngineCommand::Attack { target_tag: *target_tag })
            } else {
                None
            }
        }
        Command::Stop { .. } => Some(EngineCommand::Stop),
        Command::Train { unit_type } => Some(EngineCommand::Train { unit_type: *unit_type }),
        Command::Build { x, y, unit_type, .. } => Some(EngineCommand::Build { x: *x, y: *y, unit_type: *unit_type }),
        Command::UnitMorph { unit_type } => Some(EngineCommand::UnitMorph { unit_type: *unit_type }),
        Command::BuildingMorph { unit_type } => Some(EngineCommand::BuildingMorph { unit_type: *unit_type }),
        Command::Research { tech_type } => Some(EngineCommand::Research { tech_type: *tech_type }),
        Command::Upgrade { upgrade_type } => Some(EngineCommand::Upgrade { upgrade_type: *upgrade_type }),
        _ => None,
    }
}

fn run_full_sim(replay_name: &str) {
    let units_dat = bw_data("units.dat").expect("units.dat not found in /tmp/bw-data/");
    let flingy_dat = bw_data("flingy.dat").expect("flingy.dat not found");
    let weapons_dat = bw_data("weapons.dat").expect("weapons.dat not found");

    let rep_data = fixture(replay_name);
    let replay = replay_core::parse(&rep_data).unwrap();

    println!("=== {replay_name} with REAL DATA ===");
    println!("  Map: {} ({}x{})", replay.header.map_name, replay.header.map_width, replay.header.map_height);
    for p in &replay.header.players {
        println!("  Player {}: {} ({})", p.player_id, p.name, p.race.code());
    }

    let data = GameData::from_dat_full(&units_dat, &flingy_dat, &weapons_dat).unwrap();

    // Validate known unit types.
    let marine = data.unit_type(0).unwrap();
    assert_eq!(marine.hitpoints >> 8, 40, "Marine should have 40 HP");
    let zergling = data.unit_type(37).unwrap();
    assert_eq!(zergling.hitpoints >> 8, 35, "Zergling should have 35 HP");

    let marine_flingy_raw = data.flingy_types.get(marine.flingy_id as usize).unwrap();
    let marine_flingy = data.flingy_for_unit(0).unwrap();
    println!("  Marine flingy raw: speed={}, resolved: speed={} ({:.2} px/f), accel={}, turn={}",
        marine_flingy_raw.top_speed, marine_flingy.top_speed,
        marine_flingy.top_speed as f64 / 256.0,
        marine_flingy.acceleration, marine_flingy.turn_rate);

    let sections = chk::parse_sections(&replay.map_data).unwrap();
    let terrain = chk::extract_terrain(&sections).unwrap();
    let tileset = bw_engine::Tileset::from_index(terrain.tileset_index).unwrap();
    println!("  Tileset: {} ({})", tileset.name(), terrain.tileset_index);

    let (cv5, vf4) = tileset_files(terrain.tileset_index).expect("tileset not found");
    let map = Map::from_chk(&replay.map_data, &cv5, &vf4).unwrap();

    let walkable = (0..map.height()).flat_map(|y| (0..map.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| map.is_tile_passable(x, y))
        .count();
    let total = map.width() as usize * map.height() as usize;
    println!("  Walkable: {}/{} ({:.0}%)", walkable, total, walkable as f64 / total as f64 * 100.0);

    let mut game = Game::new(map, data);
    let chk_units = chk_units::parse_chk_units(&sections).unwrap();
    game.load_initial_units(&chk_units).unwrap();

    let is_melee = matches!(
        replay.header.game_type,
        replay_core::header::GameType::Melee | replay_core::header::GameType::OneOnOne
    );
    if is_melee {
        let start_locs = chk_units::parse_start_locations(&sections);
        let races: Vec<(u8, u8)> = replay.header.players.iter().map(|p| {
            let r = match p.race {
                replay_core::header::Race::Zerg => 0,
                replay_core::header::Race::Terran => 1,
                replay_core::header::Race::Protoss => 2,
                _ => 1,
            };
            (p.player_id, r)
        }).collect();
        let locs: Vec<(u8, i32, i32)> = start_locs.iter().map(|&(o, x, y)| (o, x as i32, y as i32)).collect();
        game.create_melee_starting_units(&locs, &races);
    }

    let initial = game.unit_count();
    println!("  Units after setup: {}", initial);

    let total_frames = replay.header.frame_count;
    let mut cmd_idx = 0;
    let mut translated = 0;
    let mut peak = initial;
    let mut deaths = 0;

    for frame in 1..=total_frames {
        while cmd_idx < replay.commands.len() && replay.commands[cmd_idx].frame <= frame {
            let gc = &replay.commands[cmd_idx];
            if gc.frame == frame {
                if let Some(cmd) = translate(&gc.command) {
                    game.apply_command(gc.player_id, &cmd);
                    translated += 1;
                }
            }
            cmd_idx += 1;
        }

        let before = game.unit_count();
        game.step();
        let after = game.unit_count();

        if after < before { deaths += before - after; }
        if after > peak { peak = after; }

        if frame == total_frames / 4 || frame == total_frames / 2 || frame == total_frames * 3 / 4 {
            println!("  Frame {}/{}: {} units", frame, total_frames, after);
        }
    }

    println!("  --- RESULTS ---");
    println!("  Commands: {} / {} ({:.0}%)", translated, replay.commands.len(),
        translated as f64 / replay.commands.len().max(1) as f64 * 100.0);
    println!("  Units: {} initial -> {} final (peak {}, {} deaths)", initial, game.unit_count(), peak, deaths);
    println!("  PASSED");
}

#[test]
#[ignore]
fn test_real_1v1_melee() { run_full_sim("1v1melee.rep"); }

#[test]
#[ignore]
fn test_real_larva_vs_mini() { run_full_sim("larva_vs_mini.rep"); }

#[test]
#[ignore]
fn test_real_polypoid() { run_full_sim("polypoid.rep"); }

#[test]
#[ignore]
fn test_real_centauro() { run_full_sim("centauro_vs_djscan.rep"); }

#[test]
#[ignore]
fn test_real_franky() { run_full_sim("franky_vs_djscan.rep"); }
