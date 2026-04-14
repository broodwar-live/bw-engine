use criterion::{Criterion, black_box, criterion_group, criterion_main};

use bw_engine::dat::{FlingyType, GameData, UnitType, WeaponType};
use bw_engine::tile::MiniTile;
use bw_engine::tileset::{CV5_ENTRY_SIZE, VF4_ENTRY_SIZE};
use bw_engine::{Game, Map};

fn test_game_data() -> GameData {
    let marine_flingy = FlingyType {
        top_speed: 4 * 256,
        acceleration: 256,
        halt_distance: 0,
        turn_rate: 40,
        movement_type: 0,
    };
    let flingy_types = vec![marine_flingy; 209];

    let marine_ut = UnitType {
        flingy_id: 0,
        turret_unit_type: 228,
        hitpoints: 40 * 256,
        ground_weapon: 0,
        max_ground_hits: 1,
        air_weapon: 130,
        sight_range: 7,
        build_time: 30,
        mineral_cost: 50,
        unit_size: bw_engine::UnitSize::Small,
        ..UnitType::default()
    };
    let mut unit_types = vec![UnitType::default(); 228];
    unit_types[0] = marine_ut;

    let marine_weapon = WeaponType {
        damage_amount: 6,
        damage_bonus: 0,
        cooldown: 15,
        damage_factor: 1,
        damage_type: bw_engine::DamageType::Normal,
        damage_upgrade: 7,
        max_range: 128,
    };
    let weapon_types = vec![marine_weapon; 130];

    GameData {
        flingy_types,
        unit_types,
        weapon_types,
        tech_types: Vec::new(),
        upgrade_types: Vec::new(),
        order_types: Vec::new(),
        fallback_flingy: Vec::new(),
    }
}

fn test_map() -> Map {
    let walkable = [MiniTile::WALKABLE; 16];
    let mut vf4 = vec![0u8; VF4_ENTRY_SIZE];
    for (j, &f) in walkable.iter().enumerate() {
        vf4[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
    }
    let mut cv5 = vec![0u8; CV5_ENTRY_SIZE];
    cv5[20..22].copy_from_slice(&0u16.to_le_bytes());

    let mut chk = Vec::new();
    chk.extend_from_slice(b"DIM ");
    chk.extend_from_slice(&4u32.to_le_bytes());
    chk.extend_from_slice(&16u16.to_le_bytes()); // Larger map for benchmarks.
    chk.extend_from_slice(&16u16.to_le_bytes());
    chk.extend_from_slice(b"ERA ");
    chk.extend_from_slice(&2u32.to_le_bytes());
    chk.extend_from_slice(&0u16.to_le_bytes());
    let mtxm = vec![0u8; 16 * 16 * 2];
    chk.extend_from_slice(b"MTXM");
    chk.extend_from_slice(&(mtxm.len() as u32).to_le_bytes());
    chk.extend_from_slice(&mtxm);

    Map::from_chk(&chk, &cv5, &vf4).unwrap()
}

fn bench_simulation_step(c: &mut Criterion) {
    let mut game = Game::new(test_map(), test_game_data());
    // Create 20 units for a somewhat realistic scenario.
    let chk_units: Vec<bw_engine::ChkUnit> = (0..20)
        .map(|i| bw_engine::ChkUnit {
            instance_id: i,
            x: (50 + (i % 10) * 30) as u16,
            y: (50 + (i / 10) * 30) as u16,
            unit_type: 0, // Marine
            owner: (i % 2) as u8,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        })
        .collect();
    game.load_initial_units(&chk_units).unwrap();

    c.bench_function("simulation_step (20 units)", |b| {
        b.iter(|| {
            game.step();
        })
    });
}

fn bench_simulation_100_frames(c: &mut Criterion) {
    let mut game = Game::new(test_map(), test_game_data());
    let chk_units: Vec<bw_engine::ChkUnit> = (0..10)
        .map(|i| bw_engine::ChkUnit {
            instance_id: i,
            x: (50 + (i % 5) * 40) as u16,
            y: (50 + (i / 5) * 40) as u16,
            unit_type: 0,
            owner: (i % 2) as u8,
            hp_percent: 100,
            shield_percent: 0,
            energy_percent: 0,
            resources: 0,
        })
        .collect();
    game.load_initial_units(&chk_units).unwrap();

    c.bench_function("simulation_100_frames (10 units)", |b| {
        b.iter(|| {
            for _ in 0..100 {
                game.step();
            }
        })
    });
}

fn bench_map_parse(c: &mut Criterion) {
    let walkable = [MiniTile::WALKABLE; 16];
    let mut vf4 = vec![0u8; VF4_ENTRY_SIZE];
    for (j, &f) in walkable.iter().enumerate() {
        vf4[j * 2..j * 2 + 2].copy_from_slice(&f.to_le_bytes());
    }
    let mut cv5 = vec![0u8; CV5_ENTRY_SIZE];
    cv5[20..22].copy_from_slice(&0u16.to_le_bytes());

    let mut chk = Vec::new();
    chk.extend_from_slice(b"DIM ");
    chk.extend_from_slice(&4u32.to_le_bytes());
    chk.extend_from_slice(&128u16.to_le_bytes());
    chk.extend_from_slice(&128u16.to_le_bytes());
    chk.extend_from_slice(b"ERA ");
    chk.extend_from_slice(&2u32.to_le_bytes());
    chk.extend_from_slice(&0u16.to_le_bytes());
    let mtxm = vec![0u8; 128 * 128 * 2];
    chk.extend_from_slice(b"MTXM");
    chk.extend_from_slice(&(mtxm.len() as u32).to_le_bytes());
    chk.extend_from_slice(&mtxm);

    c.bench_function("map_parse (128x128)", |b| {
        b.iter(|| Map::from_chk(black_box(&chk), black_box(&cv5), black_box(&vf4)).unwrap())
    });
}

criterion_group!(
    benches,
    bench_simulation_step,
    bench_simulation_100_frames,
    bench_map_parse,
);
criterion_main!(benches);
