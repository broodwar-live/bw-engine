use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn load_fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR").replace("/crates/replay-core", "")
    );
    std::fs::read(&path).unwrap_or_else(|_| panic!("fixture not found: {path}"))
}

fn bench_parse_replay(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    c.bench_function("parse_replay (polypoid)", |b| {
        b.iter(|| replay_core::parse(black_box(&data)).unwrap())
    });
}

fn bench_parse_legacy(c: &mut Criterion) {
    let data = load_fixture("centauro_vs_djscan.rep");
    c.bench_function("parse_replay (legacy)", |b| {
        b.iter(|| replay_core::parse(black_box(&data)).unwrap())
    });
}

fn bench_build_order_extraction(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    c.bench_function("extract_build_order", |b| {
        b.iter(|| replay_core::analysis::extract_build_order(black_box(&replay.commands)))
    });
}

fn bench_apm_calculation(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    c.bench_function("calculate_apm", |b| {
        b.iter(|| {
            replay_core::analysis::calculate_apm(
                black_box(&replay.commands),
                replay.header.frame_count,
            )
        })
    });
}

fn bench_apm_over_time(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    c.bench_function("apm_over_time (60s/10s)", |b| {
        b.iter(|| replay.apm_over_time(black_box(60.0), black_box(10.0)))
    });
}

fn bench_similarity(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    let p0 = replay_core::similarity::BuildSequence::from_build_order(&replay.build_order, 0);
    let p1 = replay_core::similarity::BuildSequence::from_build_order(&replay.build_order, 1);
    c.bench_function("build_order_similarity", |b| {
        b.iter(|| replay_core::similarity::similarity(black_box(&p0), black_box(&p1)))
    });
}

fn bench_classify(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    let players: Vec<(u8, replay_core::header::Race)> = replay
        .header
        .players
        .iter()
        .map(|p| (p.player_id, p.race))
        .collect();
    c.bench_function("classify_all", |b| {
        b.iter(|| {
            replay_core::classify::classify_all(black_box(&replay.build_order), black_box(&players))
        })
    });
}

fn bench_phases(c: &mut Criterion) {
    let data = load_fixture("polypoid.rep");
    let replay = replay_core::parse(&data).unwrap();
    c.bench_function("detect_phases", |b| {
        b.iter(|| {
            replay_core::phases::detect_phases(
                black_box(&replay.build_order),
                replay.header.frame_count,
            )
        })
    });
}

criterion_group!(
    benches,
    bench_parse_replay,
    bench_parse_legacy,
    bench_build_order_extraction,
    bench_apm_calculation,
    bench_apm_over_time,
    bench_similarity,
    bench_classify,
    bench_phases,
);
criterion_main!(benches);
