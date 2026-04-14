🇰🇷 [한국어](README.ko.md)

<div align="center">

<h1 align="center">
  bw-engine
</h1>

<p align="center">
  <em>StarCraft: Brood War replay parser and game engine for the web.</em><br>
  <em>Parse replays. Simulate games. Render in the browser.</em>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/">
    <img alt="Rust" src="https://img.shields.io/badge/Rust-1.85+-000000?logo=rust&logoColor=white&style=for-the-badge">
  </a>
  <a href="https://webassembly.org/">
    <img alt="WASM" src="https://img.shields.io/badge/WebAssembly-367KB-654ff0?logo=webassembly&logoColor=white&style=for-the-badge">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/badge/License-MIT-c6a0f6?style=for-the-badge">
  </a>
</p>

<p align="center">
  <a href="docs/getting-started.md">Getting Started</a> &bull;
  <a href="docs/api-reference.md">API Reference</a> &bull;
  <a href="docs/game-data-files.md">Game Data Files</a>
</p>

</div>

---

A Rust workspace that parses StarCraft: Brood War `.rep` files and selectively reimplements the BW game engine for WebAssembly. Built for [broodwar.live](https://broodwar.live), based on the [OpenBW](https://github.com/broodwar-live/openbw) C++ engine.

**No game data files needed for parsing.** The parser extracts player info, commands, build orders, and APM from any replay. The simulation engine adds movement physics, pathfinding, combat, and fog of war when provided with BW's `.dat` files.

## Quick Start

### Rust

```rust
let replay = replay_core::parse(&std::fs::read("game.rep")?)?;

println!("{} on {}", 
    replay.header.players.iter().map(|p| &p.name).collect::<Vec<_>>().join(" vs "),
    replay.header.map_name
);
println!("{:.0}s, {} commands", replay.header.duration_secs(), replay.commands.len());

for entry in replay.build_order.iter().take(5) {
    println!("  {:.0}s P{} {}", entry.real_seconds, entry.player_id, entry.action);
}
```

### Browser

```sh
wasm-pack build crates/replay-wasm --target web --out-dir ../../pkg
python3 -m http.server 8000
# Open http://localhost:8000/demo/index.html
```

The [demo page](demo/index.html) accepts a `.rep` file plus optional game data files for simulation and map rendering.

## Features

| Feature | Required Files | Description |
|---------|---------------|-------------|
| **Replay Parsing** | `.rep` only | Header, players, commands, build orders, APM, timeline |
| **Map Terrain** | + CV5, VF4 | Walkability grid, height map, tileset identification |
| **Map Rendering** | + VX4, VR4, WPE | Mini-tile pixel data, palette colors, tile graphic references |
| **Unit Simulation** | + units.dat, flingy.dat | Movement physics, acceleration, turning, waypoint following |
| **Pathfinding** | (included) | Tile-level A* with region fallback, diagonal corner prevention |
| **Combat** | + weapons.dat | Ground + air weapons, damage types (concussive/explosive/normal vs unit sizes), Protoss shields |
| **Tech & Upgrades** | + techdata.dat, upgrades.dat | Research costs/times, upgrade level scaling, upgrade bonuses applied to combat |
| **Production** | (included) | Build queues, train timers, resource deduction, data-driven supply costs |
| **Build & Morph Timers** | (included) | Building construction time, unit/building morph timers |
| **Worker Mining** | (included) | Workers generate mineral/gas income per trip cycle |
| **Energy** | (included) | Caster energy tracking with regeneration (HT, Defiler, Vessel, etc.) |
| **Unit Collision** | (included) | Ground units push apart on overlap, buildings block movement |
| **Fog of War** | (included) | Per-player visibility and exploration grids |
| **Matchup Detection** | (included) | Auto-detect TvZ/PvT/etc., normalize map names, detect winner |
| **Build Order Search** | (included) | Edit distance + LCS similarity metrics, rank by similarity |
| **Game Phase Detection** | (included) | Opening/Early/Mid/Late phases from tech landmarks |
| **Skill Estimation** | (included) | Composite skill score from EAPM, hotkeys, consistency, efficiency |
| **Build Order Classification** | (included) | Auto-label openings ("9 Pool", "1-1-1", "Forge FE") with confidence scores |
| **Player Identity** | (included) | Normalize names, strip clan tags, resolve across replays |
| **Collection Stats** | (included) | Matchup winrates, map popularity, race distribution across replay sets |
| **MPQ Archives** | `.mpq` files | Read game data archives and `.scx`/`.scm` map files |
| **String Tables** | `stat_txt.tbl` | Data-driven unit/tech/upgrade names (replaces hardcoded lookups) |
| **Sprites** | `.grp` files | RLE-decoded frame pixel data for units and buildings |

### Replay Format Support

| Format | Version | Compression |
|--------|---------|-------------|
| Legacy | Pre-1.18 | PKWare DCL Implode |
| Modern | 1.18 -- 1.20 | zlib |
| Remastered | 1.21+ | zlib + extended sections |

## Crates

Each crate has its own [`docs/architecture.md`](crates/replay-core/docs/architecture.md) with detailed module maps and design notes.

| Crate | Description |
|-------|-------------|
| [`replay-core`](crates/replay-core/) | Parse `.rep` files into structured Rust types. 40+ command variants, build order extraction, APM analysis, timeline, matchup detection, build order similarity/classification, game phases, skill estimation, player identity, and collection stats. 16 modules. |
| [`bw-engine`](crates/bw-engine/) | Selective BW engine reimplementation. Map terrain, unit simulation with fp8 physics, tile-level A* pathfinding, air + ground combat with damage types and shields, production with resource tracking, fog of war. Also includes parsers for BW native file formats: MPQ archives, SCX/SCM maps, TBL string tables, GRP sprites, and full .dat game data. 21 modules. |
| [`replay-wasm`](crates/replay-wasm/) | WASM bindings via wasm-bindgen. `parseReplay()`, `GameMap`, `GameSim`, plus `MpqFile`, `ScxMapFile`, `TblFile`, `GrpFile`, `TilesetPalette`, `TilesetVx4`, `TilesetVr4`. |
| [`replay-nif`](crates/replay-nif/) | Elixir NIF bindings via Rustler. 10 NIF functions for parsing, analysis, phases, skill, classification, similarity, and identity. |

## Engine Architecture

The simulation engine reimplements BW subsystems from the [OpenBW](https://github.com/broodwar-live/openbw) reference:

```
.rep file ──> replay-core ──> commands + map CHK data
                                  │
              units.dat ──────────┤
              flingy.dat ─────────┤
              weapons.dat ────────┤
              techdata.dat ───────┤
              upgrades.dat ───────┤
              orders.dat ─────────┤
              CV5 + VF4 ──────────┤
                                  ▼
                            bw-engine::Game
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
                Movement    Combat/Death    Production
              (fp8 physics,  (air+ground,   (build queue,
               pathfinding,   dmg types,     resource cost,
               waypoints)     shields)       supply check)
                    │             │              │
                    └─────────────┼──────────────┘
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
              Unit Positions   Fog of War   Player State
              (x, y, type,    (visible,     (minerals, gas,
               owner, HP,      explored)     supply, upgrades,
               shields)                       techs)

File format support:
  .mpq ──> MpqArchive ──> extract any file by path
  .scx ──> ScxMap ────────> CHK terrain + unit placements
  .tbl ──> StringTable ───> indexed game text strings
  .grp ──> Grp ───────────> RLE-decoded sprite frames
  VX4/VR4/WPE ────────────> tile graphics + palette
```

### Key Design Decisions

- **No filesystem access** — all inputs are `&[u8]`, WASM-safe by design
- **Fixed-point math** — 24.8 `Fp8` type matches BW's deterministic physics
- **Tag-compatible** — unit slot allocation matches BW's order (turret subunits, melee starting units)
- **Two-tier pathfinding** — tile-level A* (2048 node budget) with region graph fallback
- **Selective reimplementation** — only subsystems needed for replay viewing, not the full engine

## Demo

The [`demo/`](demo/) directory contains a single-page replay viewer that runs entirely in the browser:

- Load a `.rep` file to see replay metadata
- Add `units.dat` + `flingy.dat` to enable simulation with play/pause/speed controls
- Add `weapons.dat` for combat
- Add tileset CV5/VF4 files to render the walkability grid
- Unit positions rendered as colored dots with per-player colors

Game data files are in your StarCraft installation's `arr/` and `tileset/` directories. See [Game Data Files](docs/game-data-files.md) for details.

## Tests

```sh
cargo test --workspace    # 282 tests
cargo test -p bw-engine   # 146 unit + 6 integration (real replay fixtures)
cargo test -p replay-core # 111 unit + 12 integration + 7 proptest
cargo bench               # criterion benchmarks for parsing + simulation
```

Integration tests run the full simulation pipeline against 5 real replay fixtures (modern + legacy formats, up to 53K frames) and validate crash-free execution with 89-95% command translation coverage.

## License

MIT
