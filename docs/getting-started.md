# Getting Started

## What is this?

A Rust toolkit for working with StarCraft: Brood War replays. Parse `.rep` files, extract game data, and simulate replays in the browser using WebAssembly.

## Quick Start: Parse a Replay (Rust)

```rust
let bytes = std::fs::read("game.rep").unwrap();
let replay = replay_core::parse(&bytes).unwrap();

println!("Map: {}", replay.header.map_name);
println!("Duration: {:.0}s", replay.header.duration_secs());

for player in &replay.header.players {
    println!("  {} ({})", player.name, player.race.code());
}

for entry in replay.build_order.iter().take(10) {
    println!("  {:.0}s - P{} - {}", entry.real_seconds, entry.player_id, entry.action);
}
```

## Quick Start: Browser Replay Viewer

### 1. Build the WASM package

```sh
wasm-pack build crates/replay-wasm --target web --out-dir ../../pkg
```

### 2. Serve and open the demo

```sh
python3 -m http.server 8000
# Open http://localhost:8000/demo/index.html
```

### 3. Load files in the demo

The demo accepts these files via drag-and-drop or file inputs:

| File | Required | Source |
|------|----------|--------|
| `.rep` replay | Yes | Any BW replay file |
| `units.dat` | For simulation | `StarCraft/arr/units.dat` |
| `flingy.dat` | For simulation | `StarCraft/arr/flingy.dat` |
| `weapons.dat` | For combat | `StarCraft/arr/weapons.dat` |
| `techdata.dat` | For tech costs | `StarCraft/arr/techdata.dat` |
| `upgrades.dat` | For upgrade costs | `StarCraft/arr/upgrades.dat` |
| `orders.dat` | For order metadata | `StarCraft/arr/orders.dat` |
| `.cv5` tileset | For map terrain | `StarCraft/tileset/<name>.cv5` |
| `.vf4` tileset | For map terrain | `StarCraft/tileset/<name>.vf4` |
| `.vx4` tileset | For map rendering | `StarCraft/tileset/<name>.vx4` |
| `.vr4` tileset | For map rendering | `StarCraft/tileset/<name>.vr4` |
| `.wpe` tileset | For map rendering | `StarCraft/tileset/<name>.wpe` |

The `.dat` files are in your StarCraft installation's `arr/` directory. Tileset files are in `tileset/` (e.g., `badlands.cv5` for Badlands maps). You can also load `.scx`/`.scm` map files and game data directly from MPQ archives.

## What You Get

### From replay parsing alone (no game data files needed):
- Map name, dimensions, tileset
- Player names, races, teams, colors
- Game duration, speed, type
- Full command stream with frame timing
- Build order with human-readable names
- APM and EAPM per player
- APM over time for graphing
- Timeline of cumulative resource investment
- **Matchup detection** — auto-detect TvZ/PvT/etc., normalize map names, detect winner
- **Build order similarity** — compare builds with edit distance and LCS metrics
- **Game phase detection** — Opening/Early/Mid/Late game from tech landmarks
- **Skill estimation** — composite score from EAPM, hotkeys, consistency, efficiency
- **Build order classification** — auto-label openings ("9 Pool", "1-1-1", "Forge FE")
- **Player identity** — normalize names, strip clan tags, resolve across replays
- **Collection stats** — aggregate matchup winrates, map popularity across replay sets

### With game data files (units.dat, flingy.dat):
- Unit simulation with movement physics
- Pathfinding around terrain
- Initial unit placement from map data

### With weapons.dat additionally:
- Air + ground combat with correct weapon selection
- Damage type modifiers (concussive/explosive/normal vs unit sizes)
- Protoss shields (absorb damage first)
- Upgrade bonuses applied to weapon damage and armor
- Resource deduction and supply tracking for training/building

### With techdata.dat and upgrades.dat:
- Data-driven tech research costs and times
- Upgrade costs with per-level scaling

### With tileset files (CV5, VF4):
- Map walkability grid
- Terrain height map

### With tileset rendering files (VX4, VR4, WPE):
- Mini-tile pixel data for visual map rendering
- 256-color tileset palette

### With MPQ archives:
- Load `.scx`/`.scm` map files directly (extracts CHK automatically)
- Extract game data files from StarDat.mpq / BrooDat.mpq

### With TBL string tables (stat_txt.tbl):
- Data-driven unit, tech, and upgrade names

### With GRP sprite files:
- RLE-decoded frame pixel data for unit and building sprites

## Crate Overview

| Crate | Purpose |
|-------|---------|
| `replay-core` | Parse `.rep` files into structured Rust types |
| `bw-engine` | Game engine: map, units, pathfinding, combat, production, fog of war |
| `replay-wasm` | WASM bindings for browser use |
| `replay-nif` | Elixir NIF bindings (stub) |
