use replay_core::header::{Engine, Race, Speed};

fn fixture(name: &str) -> Vec<u8> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest_dir}/../../tests/fixtures/{name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

#[test]
fn test_parse_1v1_melee() {
    let data = fixture("1v1melee.rep");
    let replay = replay_core::parse(&data).expect("failed to parse 1v1melee.rep");

    assert_eq!(replay.header.engine, Engine::BroodWar);
    assert_eq!(replay.header.game_speed, Speed::Fastest);
    assert!(replay.header.frame_count > 0);
    assert!(replay.header.duration_secs() > 0.0);
    assert!(!replay.header.map_name.is_empty());
    assert!(replay.header.players.len() >= 2);

    for player in &replay.header.players {
        assert!(!player.name.is_empty());
        assert!(matches!(
            player.race,
            Race::Terran | Race::Protoss | Race::Zerg
        ));
    }

    println!("=== 1v1melee.rep ===");
    println!("Map: {}", replay.header.map_name);
    println!(
        "Duration: {:.0}s ({} frames)",
        replay.header.duration_secs(),
        replay.header.frame_count
    );
    println!("Commands: {}", replay.commands.len());
    println!("Build order entries: {}", replay.build_order.len());
    for apm in &replay.player_apm {
        println!(
            "  Player {}: APM={:.0} EAPM={:.0}",
            apm.player_id, apm.apm, apm.eapm
        );
    }
}

#[test]
fn test_parse_larva_vs_mini() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).expect("failed to parse larva_vs_mini.rep");

    assert_eq!(replay.header.engine, Engine::BroodWar);
    assert!(replay.header.players.len() >= 2);
    assert!(replay.header.frame_count > 0);

    // This is a real game — should have meaningful commands.
    assert!(
        replay.commands.len() > 100,
        "expected >100 commands for a real game, got {}",
        replay.commands.len()
    );
    assert!(
        !replay.build_order.is_empty(),
        "expected non-empty build order"
    );
    assert!(
        !replay.player_apm.is_empty(),
        "expected APM data for players"
    );

    // APM should be reasonable for a competitive game (>50 APM).
    for apm in &replay.player_apm {
        assert!(
            apm.apm > 10.0,
            "player {} APM={:.0} is suspiciously low",
            apm.player_id,
            apm.apm
        );
    }

    println!("=== larva_vs_mini.rep ===");
    println!("Map: {}", replay.header.map_name);
    println!(
        "Duration: {:.0}s ({} frames)",
        replay.header.duration_secs(),
        replay.header.frame_count
    );
    println!("Commands: {}", replay.commands.len());
    println!("Build order entries: {}", replay.build_order.len());
    for p in &replay.header.players {
        println!("  {} ({})", p.name, p.race.code());
    }
    for apm in &replay.player_apm {
        println!(
            "  Player {}: APM={:.0} EAPM={:.0}",
            apm.player_id, apm.apm, apm.eapm
        );
    }
    println!("First 10 build order entries:");
    for entry in replay.build_order.iter().take(10) {
        println!(
            "  {:.0}s player {} — {:?}",
            entry.real_seconds, entry.player_id, entry.action
        );
    }
}

#[test]
fn test_parse_polypoid() {
    let data = fixture("polypoid.rep");
    let replay = replay_core::parse(&data).expect("failed to parse polypoid.rep");

    assert_eq!(replay.header.engine, Engine::BroodWar);
    assert!(replay.header.players.len() >= 2);

    let map_lower = replay.header.map_name.to_lowercase();
    assert!(
        map_lower.contains("polypoid"),
        "expected map name to contain 'polypoid', got '{}'",
        replay.header.map_name
    );

    println!("=== polypoid.rep ===");
    println!("Map: {}", replay.header.map_name);
    println!(
        "Duration: {:.0}s ({} frames)",
        replay.header.duration_secs(),
        replay.header.frame_count
    );
    println!("Commands: {}", replay.commands.len());
}

#[test]
fn test_apm_over_time() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).expect("failed to parse");

    let samples = replay.apm_over_time(60.0, 30.0);
    assert!(!samples.is_empty(), "expected APM timeline samples");

    // Samples should cover the game duration.
    let last = samples.last().unwrap();
    assert!(
        last.real_seconds > replay.header.duration_secs() * 0.8,
        "APM timeline should cover most of the game"
    );
}
