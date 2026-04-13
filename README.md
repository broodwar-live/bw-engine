🌏 [한국어](README.ko.md)

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
    <img alt="WASM" src="https://img.shields.io/badge/WebAssembly-322KB-654ff0?logo=webassembly&logoColor=white&style=for-the-badge">
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
| **Unit Simulation** | + units.dat, flingy.dat | Movement physics, acceleration, turning, waypoint following |
| **Pathfinding** | (included) | Tile-level A* with region fallback, diagonal corner prevention |
| **Combat** | + weapons.dat | Weapon damage, cooldowns, range checks, unit death |
| **Production** | (included) | Build queues, train timers, unit spawning |
| **Fog of War** | (included) | Per-player visibility and exploration grids |

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
| [`replay-core`](crates/replay-core/) | Parse `.rep` files into structured Rust types. 40+ command variants, build order extraction, APM analysis, timeline generation. |
| [`bw-engine`](crates/bw-engine/) | Selective BW engine reimplementation. Map terrain, unit simulation with fp8 physics, tile-level A* pathfinding, combat, production, fog of war. 15 modules. |
| [`replay-wasm`](crates/replay-wasm/) | WASM bindings via wasm-bindgen. `parseReplay()`, `GameMap`, `GameSim` with frame stepping and bulk data queries. |
| [`replay-nif`](crates/replay-nif/) | Elixir NIF bindings via Rustler (stub). |

## Engine Architecture

The simulation engine reimplements BW subsystems from the [OpenBW](https://github.com/broodwar-live/openbw) reference:

```
.rep file ──> replay-core ──> commands + map CHK data
                                  │
              units.dat ──────────┤
              flingy.dat ─────────┤
              weapons.dat ────────┤
              CV5 + VF4 ──────────┤
                                  ▼
                            bw-engine::Game
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
                Movement    Combat/Death    Production
              (fp8 physics,  (weapons,     (build queue,
               pathfinding,   damage,       train timer,
               waypoints)     cooldowns)    unit spawn)
                    │             │              │
                    └─────────────┼──────────────┘
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
              Unit Positions   Fog of War   Player State
              (x, y, type,    (visible,     (minerals,
               owner, HP)      explored)     gas, supply)
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
cargo test --workspace    # 158 tests
cargo test -p bw-engine   # 106 unit + 6 integration (real replay fixtures)
```

Integration tests run the full simulation pipeline against 5 real replay fixtures (modern + legacy formats, up to 53K frames) and validate crash-free execution with 89-95% command translation coverage.

## License

MIT
