use replay_core::header::{Engine, PlayerType, Race, Speed};

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
    println!("First 15 build order entries:");
    for entry in replay.build_order.iter().take(15) {
        println!(
            "  {:>5.0}s  P{}  {}",
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

// -- Legacy (pre-1.18, PKWare DCL) replays --

#[test]
fn test_parse_legacy_centauro_vs_djscan() {
    let data = fixture("centauro_vs_djscan.rep");
    let replay = replay_core::parse(&data).expect("failed to parse centauro_vs_djscan.rep");

    assert_eq!(replay.header.engine, Engine::BroodWar);
    assert!(replay.header.frame_count > 0);
    assert!(!replay.header.map_name.is_empty());

    let humans: Vec<_> = replay
        .header
        .players
        .iter()
        .filter(|p| p.player_type == PlayerType::Human)
        .collect();
    assert!(
        humans.len() >= 2,
        "expected at least 2 human players, got {}",
        humans.len()
    );

    // This is a real ICCup ladder game — should have commands.
    assert!(
        replay.commands.len() > 100,
        "expected >100 commands, got {}",
        replay.commands.len()
    );
    assert!(!replay.build_order.is_empty());

    println!("=== centauro_vs_djscan.rep (LEGACY) ===");
    println!("Map: {}", replay.header.map_name);
    println!(
        "Duration: {:.0}s ({} frames)",
        replay.header.duration_secs(),
        replay.header.frame_count
    );
    println!("Commands: {}", replay.commands.len());
    println!("Build order entries: {}", replay.build_order.len());
    for p in &replay.header.players {
        println!("  {} ({}) — {:?}", p.name, p.race.code(), p.player_type);
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
            "  {:>5.0}s  P{}  {}",
            entry.real_seconds, entry.player_id, entry.action
        );
    }
}

#[test]
fn test_parse_legacy_franky_vs_djscan() {
    let data = fixture("franky_vs_djscan.rep");
    let replay = replay_core::parse(&data).expect("failed to parse franky_vs_djscan.rep");

    assert_eq!(replay.header.engine, Engine::BroodWar);
    assert!(replay.header.frame_count > 0);
    assert!(replay.commands.len() > 50);

    println!("=== franky_vs_djscan.rep (LEGACY) ===");
    println!("Map: {}", replay.header.map_name);
    println!(
        "Duration: {:.0}s ({} frames)",
        replay.header.duration_secs(),
        replay.header.frame_count
    );
    println!("Commands: {}", replay.commands.len());
    for p in &replay.header.players {
        println!("  {} ({}) — {:?}", p.name, p.race.code(), p.player_type);
    }
}

// ---------------------------------------------------------------------------
// Analysis features — metadata, classification, phases, skill
// ---------------------------------------------------------------------------

#[test]
fn test_metadata_on_real_replay() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).unwrap();

    let meta = &replay.metadata;
    assert!(meta.is_1v1, "larva_vs_mini should be a 1v1");
    assert_eq!(meta.player_count, 2);
    assert!(
        !meta.map_name.is_empty(),
        "normalized map name should not be empty"
    );
    assert!(meta.duration_secs > 0.0);

    // Should detect a matchup.
    assert!(meta.matchup.is_some(), "should detect matchup for 1v1");
    let mu = meta.matchup.as_ref().unwrap();
    assert!(
        mu.code.len() == 3,
        "matchup code should be 3 chars: {}",
        mu.code
    );
    assert!(mu.code.contains('v'), "matchup code should contain 'v'");

    println!("Matchup: {} (mirror={})", mu.code, mu.mirror);
    println!("Map: {} (raw: {})", meta.map_name, meta.map_name_raw);
    println!("Result: {:?}", meta.result);
}

#[test]
fn test_classification_on_real_replay() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).unwrap();

    let players: Vec<(u8, replay_core::header::Race)> = replay
        .header
        .players
        .iter()
        .map(|p| (p.player_id, p.race))
        .collect();

    let classifications = replay_core::classify::classify_all(&replay.production, &players);
    assert_eq!(classifications.len(), 2, "should classify both players");

    for c in &classifications {
        assert!(!c.name.is_empty());
        assert!(!c.tag.is_empty());
        assert!(c.confidence >= 0.0 && c.confidence <= 1.0);
        assert!(c.actions_analyzed > 0);
        println!(
            "  {} ({}) — {} (confidence {:.0}%)",
            c.race,
            c.tag,
            c.name,
            c.confidence * 100.0
        );
    }
}

#[test]
fn test_phases_on_real_replay() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).unwrap();

    let analysis =
        replay_core::phases::detect_phases(&replay.build_order, replay.header.frame_count);

    assert!(
        !analysis.phases.is_empty(),
        "should detect at least one phase"
    );
    assert_eq!(
        analysis.phases[0].phase,
        replay_core::phases::Phase::Opening,
        "first phase should be Opening"
    );

    // Phases should be in order.
    for i in 1..analysis.phases.len() {
        assert!(
            analysis.phases[i].start_frame >= analysis.phases[i - 1].start_frame,
            "phases should be in chronological order"
        );
    }

    println!("Phases:");
    for p in &analysis.phases {
        println!(
            "  {} at {:.0}s (frame {})",
            p.phase.name(),
            p.start_seconds,
            p.start_frame
        );
    }
    println!(
        "Landmarks: gas={:?} tech={:?} tier2={:?} tier3={:?} expand={:?}",
        analysis.landmarks.first_gas,
        analysis.landmarks.first_tech,
        analysis.landmarks.first_tier2,
        analysis.landmarks.first_tier3,
        analysis.landmarks.first_expansion,
    );
}

#[test]
fn test_skill_on_real_replay() {
    let data = fixture("larva_vs_mini.rep");
    let replay = replay_core::parse(&data).unwrap();

    let samples = replay.apm_over_time(60.0, 10.0);
    let profiles = replay_core::skill::estimate_skill(
        &replay.commands,
        &replay.player_apm,
        &samples,
        replay.header.frame_count,
    );

    assert_eq!(
        profiles.len(),
        2,
        "should have skill profiles for both players"
    );

    for p in &profiles {
        assert!(p.skill_score >= 0.0 && p.skill_score <= 100.0);
        assert!(p.apm > 0.0, "APM should be positive for real games");
        assert!(p.eapm > 0.0, "EAPM should be positive for real games");
        assert!(p.efficiency > 0.0 && p.efficiency <= 1.0);
        println!(
            "  P{}: score={:.0} tier={} apm={:.0} eapm={:.0} eff={:.0}%",
            p.player_id,
            p.skill_score,
            p.tier.name(),
            p.apm,
            p.eapm,
            p.efficiency * 100.0
        );
    }
}

#[test]
fn test_similarity_on_real_replays() {
    let data1 = fixture("polypoid.rep");
    let data2 = fixture("larva_vs_mini.rep");
    let r1 = replay_core::parse(&data1).unwrap();
    let r2 = replay_core::parse(&data2).unwrap();

    let p0_r1 = replay_core::similarity::BuildSequence::from_build_order(
        &r1.build_order,
        r1.header.players[0].player_id,
    );
    let p0_r2 = replay_core::similarity::BuildSequence::from_build_order(
        &r2.build_order,
        r2.header.players[0].player_id,
    );

    // Self-similarity should be 1.0.
    let self_sim = replay_core::similarity::similarity(&p0_r1, &p0_r1);
    assert!(
        (self_sim - 1.0).abs() < 0.001,
        "self-similarity should be 1.0"
    );

    // Cross-replay similarity should be between 0 and 1.
    let cross_sim = replay_core::similarity::similarity(&p0_r1, &p0_r2);
    assert!((0.0..=1.0).contains(&cross_sim));
    println!("Self-similarity: {:.3}", self_sim);
    println!("Cross-similarity: {:.3}", cross_sim);
}

#[test]
fn test_stats_collector_on_real_replays() {
    let fixtures = ["polypoid.rep", "larva_vs_mini.rep", "1v1melee.rep"];
    let mut collector = replay_core::stats::StatsCollector::new();

    for name in &fixtures {
        let data = fixture(name);
        if let Ok(replay) = replay_core::parse(&data) {
            collector.add(&replay);
        }
    }

    let report = collector.report();
    assert!(
        report.total_replays >= 2,
        "should have parsed at least 2 replays"
    );
    assert!(!report.map_popularity.is_empty(), "should have map data");

    println!(
        "Stats: {} replays, {} 1v1",
        report.total_replays, report.total_1v1
    );
    for m in &report.matchup_winrates {
        println!(
            "  {}: {} games, {:.0}% first race WR",
            m.matchup,
            m.games,
            m.first_race_winrate * 100.0
        );
    }
    for m in &report.map_popularity {
        println!(
            "  Map: {} ({} games, {:.1}%)",
            m.map_name, m.games, m.percentage
        );
    }
}

#[test]
fn test_command_stream_all_fixtures_parse_without_warnings() {
    for name in [
        "1v1melee.rep",
        "larva_vs_mini.rep",
        "polypoid.rep",
        "centauro_vs_djscan.rep",
        "franky_vs_djscan.rep",
    ] {
        let replay = replay_core::parse(&fixture(name)).expect(name);
        assert!(
            replay.command_warnings.is_clean(),
            "{name}: {:?}",
            replay.command_warnings
        );
        assert!(!replay.commands.is_empty(), "{name}: no commands");
    }
}

#[test]
fn test_remastered_extra_sections_on_real_replays() {
    let poly = replay_core::parse(&fixture("polypoid.rep")).unwrap();
    assert_eq!(
        poly.extra_sections.tags,
        vec!["SKIN", "LMTS", "BFIX", "CCLR", "GCFG"]
    );
    assert!(!poly.extra_sections.truncated);
    // polypoid.rep uses custom colours; slot 0 is brown (0x703014 in SC:R).
    assert_eq!(poly.extra_sections.player_colors[0], Some(0x703014));
    assert!(poly.header.players.iter().any(|p| p.custom_color.is_some()));

    // Older 1.21 replay with only SKIN + LMTS, and no custom colours.
    let larva = replay_core::parse(&fixture("larva_vs_mini.rep")).unwrap();
    assert_eq!(larva.extra_sections.tags, vec!["SKIN", "LMTS"]);
    assert!(larva.extra_sections.player_colors.is_empty());

    // Legacy replays have none.
    let legacy = replay_core::parse(&fixture("centauro_vs_djscan.rep")).unwrap();
    assert!(legacy.extra_sections.tags.is_empty());
}

#[test]
fn test_polypoid_cclr_colors_override_header_colors() {
    // The header color array says purple/yellow/blue/white, but the game was
    // played with the colours stored in CCLR (brown/blue/lime/green).
    let replay = replay_core::parse(&fixture("polypoid.rep")).unwrap();
    let got: Vec<(String, u32, Option<u32>)> = replay
        .header
        .players
        .iter()
        .map(|p| (p.name.clone(), p.color, p.custom_color))
        .collect();
    assert_eq!(
        got,
        vec![
            ("Furinax Tribe".to_string(), 5, Some(0x703014)),
            ("Dada".to_string(), 1, Some(0x0C48CC)),
            ("Mar Sara".to_string(), 17, Some(0xD2F53C)),
            ("Shelak Tribe".to_string(), 8, Some(0x088008)),
        ]
    );
    for p in &replay.header.players {
        assert_eq!(p.color_rgb(), p.custom_color);
    }
}

#[test]
fn test_map_names_normalized_on_fixtures() {
    let cases = [
        ("franky_vs_djscan.rep", "| iCCup | Python 1.3", "Python"),
        ("centauro_vs_djscan.rep", "| iCCup | Python 1.3", "Python"),
        ("larva_vs_mini.rep", "Transistor1.2", "Transistor"),
        ("polypoid.rep", "Polypoid 1.65", "Polypoid"),
        ("1v1melee.rep", "Road War", "Road War"),
    ];
    for (file, raw, normalized) in cases {
        let replay = replay_core::parse(&fixture(file)).unwrap();
        assert_eq!(replay.metadata.map_name_raw, raw, "{file}");
        assert_eq!(replay.metadata.map_name, normalized, "{file}");
    }
}

/// Supply counts at the first orders of each structure, in "9 pool" notation.
fn supply_at_first(replay: &replay_core::Replay, player: &str, unit: u16, nth: usize) -> u32 {
    use replay_core::analysis::BuildAction;
    let pid = replay
        .header
        .players
        .iter()
        .find(|p| p.name == player)
        .unwrap_or_else(|| panic!("no player {player}"))
        .player_id;
    replay
        .production
        .iter()
        .filter(|e| e.player_id == pid && !e.dropped)
        .filter(|e| matches!(e.action, BuildAction::Build(id) if id == unit))
        .nth(nth)
        .unwrap_or_else(|| panic!("{player}: no build #{nth} of {unit}"))
        .supply
}

#[test]
fn test_production_supply_counts_on_fixtures() {
    const PYLON: u16 = 156;
    const GATEWAY: u16 = 160;
    const NEXUS: u16 = 154;
    const FORGE: u16 = 166;
    const BARRACKS: u16 = 111;
    const DEPOT: u16 = 109;
    const POOL: u16 = 142;
    const HATCH: u16 = 131;

    let cases: &[(&str, &str, u16, usize, u32)] = &[
        // centauro: 9 pylon, 10 gate.
        ("centauro_vs_djscan.rep", "centauro", PYLON, 0, 9),
        ("centauro_vs_djscan.rep", "centauro", GATEWAY, 0, 10),
        // djscan: 12 hatch, 11 pool (the hatch drone is gone).
        ("centauro_vs_djscan.rep", "djscan", HATCH, 0, 12),
        ("centauro_vs_djscan.rep", "djscan", POOL, 0, 11),
        // franky2000: 7 rax, 9 depot, 10 rax.
        ("franky_vs_djscan.rep", "franky2000", BARRACKS, 0, 7),
        ("franky_vs_djscan.rep", "franky2000", DEPOT, 0, 9),
        ("franky_vs_djscan.rep", "franky2000", BARRACKS, 1, 10),
        // djscan: 4 pool.
        ("franky_vs_djscan.rep", "djscan", POOL, 0, 4),
        // Moo.Sapa: 8 pylon, 12 nexus, 13 forge (probe spam capped by queue).
        ("larva_vs_mini.rep", "Moo.Sapa", PYLON, 0, 8),
        ("larva_vs_mini.rep", "Moo.Sapa", NEXUS, 0, 12),
        ("larva_vs_mini.rep", "Moo.Sapa", FORGE, 0, 13),
        // adultrabbit: 12 pool, 11 hatch (drones are larva morphs).
        ("larva_vs_mini.rep", "adultrabbit", POOL, 0, 12),
        ("larva_vs_mini.rep", "adultrabbit", HATCH, 0, 11),
        // Dada: 5 pool.
        ("1v1melee.rep", "Dada", POOL, 0, 5),
    ];
    for &(file, player, unit, nth, want) in cases {
        let replay = replay_core::parse(&fixture(file)).unwrap();
        assert_eq!(
            supply_at_first(&replay, player, unit, nth),
            want,
            "{file} {player} unit {unit} #{nth}"
        );
    }
}

#[test]
fn test_timeline_starts_with_workers_and_main_building() {
    let replay = replay_core::parse(&fixture("larva_vs_mini.rep")).unwrap();
    assert_eq!(replay.timeline.len(), replay.build_order.len() + 1);
    assert_eq!(replay.production.len(), replay.build_order.len());
    let start = &replay.timeline[0];
    let protoss = start.players.iter().find(|p| p.player_id == 1).unwrap();
    let zerg = start.players.iter().find(|p| p.player_id == 0).unwrap();
    assert_eq!((protoss.supply_used, protoss.supply_max), (4, 9));
    assert_eq!(protoss.units[&64], 4);
    assert_eq!(protoss.buildings[&154], 1);
    assert_eq!((zerg.supply_used, zerg.supply_max), (4, 9));

    // Probe spam must not inflate supply: at the first Pylon (0:48) the
    // Protoss player had 8 supply, not ~34 as when every order counted.
    let last = replay.timeline.last().unwrap();
    for p in &last.players {
        assert!(p.supply_used <= 200 && p.supply_max <= 200, "{p:?}");
    }
    // Deaths are not modelled, so late-game orders are never supply blocked.
    assert!(replay.production.iter().all(|e| {
        e.frame < replay_core::production::SUPPLY_GATE_FRAMES
            || e.drop_reason != Some(replay_core::production::DropReason::SupplyBlocked)
    }));
}

#[test]
fn test_openings_on_fixtures() {
    // Before the production model every Zerg pool read as "4/5 Pool"
    // (drones are larva morphs, not Train orders) and Nexus/CC first could
    // never match.
    /// (player, tag, name)
    type Expected<'a> = &'a [(&'a str, &'a str, &'a str)];
    let cases: &[(&str, Expected)] = &[
        (
            "centauro_vs_djscan.rep",
            &[
                ("centauro", "1gate_core", "1 Gate Core"),
                ("djscan", "12hatch", "12 Hatch → Lair"),
            ],
        ),
        (
            "franky_vs_djscan.rep",
            &[
                ("franky2000", "2rax", "2 Rax"),
                ("djscan", "4pool", "4/5 Pool"),
            ],
        ),
        (
            "larva_vs_mini.rep",
            &[
                ("Moo.Sapa", "nexus_first", "Nexus First"),
                ("adultrabbit", "12pool", "12 Pool → Lair"),
            ],
        ),
        ("1v1melee.rep", &[("Dada", "4pool", "4/5 Pool")]),
        ("polypoid.rep", &[("Dada", "unknown", "Unknown")]),
    ];
    for (file, want) in cases {
        let replay = replay_core::parse(&fixture(file)).unwrap();
        let openings = replay.openings();
        assert_eq!(openings.len(), want.len(), "{file}: computers are skipped");
        for (player, tag, name) in *want {
            let pid = replay
                .header
                .players
                .iter()
                .find(|p| p.name == *player)
                .unwrap()
                .player_id;
            let o = openings.iter().find(|o| o.player_id == pid).unwrap();
            assert_eq!(
                (o.tag.as_str(), o.name.as_str()),
                (*tag, *name),
                "{file} {player}"
            );
        }
    }
}
