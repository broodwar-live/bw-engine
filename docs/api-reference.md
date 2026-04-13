# API Reference

## JavaScript API (via WASM)

### `parseReplay(data: Uint8Array) -> Object`

Parse a `.rep` file. Returns:

```js
{
  header: {
    engine: "BroodWar",
    frame_count: 40856,
    map_name: "Fighting Spirit",
    map_width: 128,
    map_height: 128,
    game_speed: "Fastest",
    game_type: "Melee",
    players: [
      { name: "Flash", race: "Terran", player_id: 0, color: 111, ... },
      { name: "Jaedong", race: "Zerg", player_id: 1, color: 117, ... }
    ]
  },
  commands: [ { frame: 100, player_id: 0, command: { Select: { unit_tags: [0, 1] } } }, ... ],
  build_order: [ { frame: 500, player_id: 0, action: { Train: 7 }, real_seconds: 21.0 }, ... ],
  player_apm: [ { player_id: 0, apm: 312.5, eapm: 280.1 }, ... ],
  timeline: [ { frame: 0, players: [...] }, ... ],
  map_data: Uint8Array  // raw CHK bytes
}
```

### `GameMap`

```js
const map = new GameMap(chkBytes, cv5Bytes, vf4Bytes);

// Properties
map.width         // u16 — tile width (e.g., 128)
map.height        // u16 — tile height
map.widthPx       // u32 — pixel width (e.g., 4096)
map.heightPx      // u32 — pixel height
map.tileset       // string — "Badlands", "Jungle", etc.

// Queries
map.isWalkable(mx, my)     // bool — mini-tile (8px grid)
map.isWalkablePx(px, py)   // bool — pixel coords
map.groundHeight(mx, my)   // u8 — 0=Low, 1=Mid, 2=High, 3=VeryHigh

// Bulk data for rendering
map.walkabilityGrid()  // Uint8Array — 1=walkable, 0=blocked, (width*4 x height*4)
map.heightGrid()       // Uint8Array — 0-3 per mini-tile
```

### `GameSim`

```js
const sim = new GameSim(chkBytes, cv5, vf4, unitsDat, flingyDat, weaponsDat, repBytes);
// weaponsDat can be empty Uint8Array (combat disabled)

// Stepping
sim.step()                // advance 1 frame, apply commands at this frame
sim.stepTo(targetFrame)   // skip ahead

// Properties
sim.currentFrame   // u32
sim.unitCount      // usize

// Unit data — flat Int32Array, 6 values per unit
sim.unitData()
// [x, y, unitType, owner, hp, maxHp, x, y, unitType, owner, hp, maxHp, ...]

// Player resources — flat Int32Array, 4 values per player x 8 players
sim.playerData()
// [minerals, gas, supplyUsed, supplyMax, ...]  (player 0, then player 1, ...)

// Fog of war — Uint8Array, one byte per tile
sim.visibilityGrid(player)
// 0=fog (unexplored), 1=explored (not visible), 2=visible
// Dimensions: map.width x map.height tiles
```

## Rust API

### replay-core

```rust
use replay_core::{parse, Replay};

let replay: Replay = parse(&bytes)?;

// Header
replay.header.map_name           // String
replay.header.frame_count        // u32
replay.header.duration_secs()    // f64
replay.header.players            // Vec<Player>

// Commands
replay.commands                  // Vec<GameCommand>
replay.commands[0].frame         // u32
replay.commands[0].player_id     // u8
replay.commands[0].command       // Command enum

// Analytics
replay.build_order               // Vec<BuildOrderEntry>
replay.player_apm                // Vec<PlayerApm>
replay.apm_over_time(60.0, 10.0) // Vec<ApmSample>

// Game metadata (auto-detected)
replay.metadata.matchup          // Option<Matchup> — e.g., { code: "TvZ", mirror: false }
replay.metadata.map_name         // String — normalized ("Fighting Spirit")
replay.metadata.result           // GameResult::Winner { player_id, player_name } or Unknown
replay.metadata.is_1v1           // bool
replay.metadata.duration_secs    // f64

// Map data
replay.map_data                  // Vec<u8> — raw CHK bytes
```

### Matchup Detection & Metadata

```rust
use replay_core::metadata::{extract_metadata, normalize_map_name, GameResult};

let meta = extract_metadata(&replay.header, &replay.commands);
meta.matchup.unwrap().code       // "TvZ", "PvT", "ZvP", "TvT", etc.
meta.map_name                    // "(4)Fighting Spirit 1.3" → "Fighting Spirit"
meta.result                      // GameResult::Winner { player_id: 0, .. }

normalize_map_name("(4)Circuit Breaker v2.1")  // "Circuit Breaker"
```

### Build Order Similarity

```rust
use replay_core::similarity::{BuildSequence, similarity, lcs_similarity, rank_by_similarity};

// Extract per-player build sequences
let p0 = BuildSequence::from_build_order(&replay.build_order, 0);
let p1 = BuildSequence::from_build_order(&replay.build_order, 1);

// Compare openings (first 20 actions)
let opening = BuildSequence::from_build_order_opening(&replay.build_order, 0, 20);

// Similarity metrics [0.0, 1.0]
similarity(&p0, &other_build)       // edit-distance-based
lcs_similarity(&p0, &other_build)   // longest common subsequence

// Rank candidates by similarity
let ranked = rank_by_similarity(&opening, &candidate_builds);
// ranked: Vec<(index, score)> sorted by descending similarity
```

### Game Phase Detection

```rust
use replay_core::phases::{detect_phases, phase_at_frame, Phase};

let analysis = detect_phases(&replay.build_order, replay.header.frame_count);

for phase in &analysis.phases {
    println!("{}: {:.0}s", phase.phase.name(), phase.start_seconds);
}
// "Opening: 0s", "Early Game: 34s", "Mid Game: 126s", "Late Game: 336s"

// Query phase at any frame
phase_at_frame(&analysis, 3000)  // Phase::MidGame

// Tech landmarks
analysis.landmarks.first_gas     // Option<u32> — frame of first gas building
analysis.landmarks.first_tier2   // Option<u32> — frame of first tier-2 tech
```

### Player Skill Estimation

```rust
use replay_core::skill::{estimate_skill, SkillTier};

let apm_samples = replay.apm_over_time(60.0, 10.0);
let profiles = estimate_skill(
    &replay.commands, &replay.player_apm, &apm_samples, replay.header.frame_count,
);

for p in &profiles {
    println!("{}: score={:.0} tier={} eapm={:.0} efficiency={:.0}%",
        p.player_id, p.skill_score, p.tier.name(), p.eapm, p.efficiency * 100.0);
}
// "0: score=62 tier=Expert eapm=285 efficiency=82%"
```

### Elixir NIF Bindings

```elixir
defmodule BroodwarNif.ReplayParser do
  use Rustler, otp_app: :broodwar, crate: "replay_nif"

  def parse_replay(_data), do: :erlang.nif_error(:not_loaded)
  def parse_header(_data), do: :erlang.nif_error(:not_loaded)
  def extract_build_order(_data), do: :erlang.nif_error(:not_loaded)
  def calculate_apm(_data), do: :erlang.nif_error(:not_loaded)
  def apm_over_time(_data, _window_secs, _step_secs), do: :erlang.nif_error(:not_loaded)
end

# Usage
{:ok, replay} = BroodwarNif.ReplayParser.parse_replay(File.read!("game.rep"))
replay.header.map_name          # "Fighting Spirit"
replay.header.players           # [%{name: "Flash", race: :terran, ...}, ...]
replay.build_order              # [%{frame: 500, name: "Barracks", action: {:build, 111}}, ...]
replay.player_apm               # [%{player_id: 0, apm: 312.5, eapm: 280.1}, ...]
```

### bw-engine

```rust
use bw_engine::*;

// Map
let map = Map::from_chk(&chk, &cv5, &vf4)?;
map.is_walkable(mx, my)         // bool — mini-tile
map.ground_height(mx, my)       // Option<GroundHeight>

// Game data (basic)
let data = GameData::from_dat_full(&units_dat, &flingy_dat, &weapons_dat)?;

// Game data (all .dat files)
let data = GameData::from_dat_all(
    &units_dat, &flingy_dat, &weapons_dat,
    &techdata_dat, &upgrades_dat, &orders_dat,
)?;
data.tech_type(0)                // Option<&TechType> — Stim Packs
data.upgrade_type(27)            // Option<&UpgradeType> — Metabolic Boost
data.upgrade_type(0).unwrap().cost_at_level(2)  // (minerals, gas) at level 2
data.order_type(0x0A)            // Option<&OrderType> — AttackUnit

// Simulation
let mut game = Game::new(map, data);
game.load_initial_units(&chk_units)?;
game.create_melee_starting_units(&start_locs, &races);
game.set_player_resources(0, 50, 0);

game.apply_command(0, &EngineCommand::Select(vec![0, 1]));
game.apply_command(0, &EngineCommand::Move { x: 200, y: 100 });
game.step();

// Queries
for unit in game.units() {
    println!("{}: ({}, {})", unit.unit_type, unit.pixel_x, unit.pixel_y);
}
game.player_state(0)             // Option<&PlayerState>
game.visibility_grid(0)          // Vec<u8>
```

### MPQ Archives

```rust
use bw_engine::MpqArchive;

let archive = MpqArchive::from_bytes(std::fs::read("StarDat.mpq")?)?;

// Read a file by path
let units_dat = archive.read_file("arr\\units.dat")?;
let flingy_dat = archive.read_file("arr\\flingy.dat")?;

// Check if a file exists
archive.contains("arr\\weapons.dat")  // bool

// List files (if archive has a listfile)
if let Some(files) = archive.list_files() {
    for f in files { println!("{f}"); }
}
```

### SCX/SCM Map Files

```rust
use bw_engine::ScxMap;

let scx = ScxMap::from_bytes(std::fs::read("map.scx")?)?;

scx.dimensions()         // (width, height) in tiles
scx.tileset_index()      // u16 (0-7)
scx.tileset()?           // Tileset enum
scx.units                // Vec<ChkUnit> — unit placements
scx.chk_data             // Vec<u8> — raw CHK for Map::from_chk

// Build a Map with tileset files
let map = scx.to_map(&cv5_data, &vf4_data)?;
```

### String Tables (TBL)

```rust
use bw_engine::StringTable;

let tbl = StringTable::from_bytes(&stat_txt_tbl_data)?;

tbl.get(0)     // Option<&str> — first string
tbl.len()      // usize — number of strings
for s in tbl.iter() { println!("{s}"); }
```

### GRP Sprites

```rust
use bw_engine::Grp;

let grp = Grp::from_bytes(&grp_data)?;

grp.width              // u16 — max frame width
grp.height             // u16 — max frame height
grp.frame_count()      // usize

let frame = &grp.frames[0];
frame.x_offset         // u8 — left padding
frame.y_offset         // u8 — top padding
frame.width            // u8 — drawn width
frame.height           // u8 — drawn height
frame.pixels           // Vec<u8> — palette indices (0 = transparent)
```

### Tileset Rendering (VX4, VR4, WPE)

```rust
use bw_engine::{Vx4Data, Vr4Data, Palette};

// VX4: megatile → mini-tile graphic references
let vx4 = Vx4Data::from_bytes(&vx4_data)?;
let entry = vx4.get(megatile_idx).unwrap();
entry.vr4_index(0)     // u16 — VR4 image index for mini-tile 0
entry.is_flipped(0)    // bool — horizontal flip

// VR4: 8x8 mini-tile pixel data
let vr4 = Vr4Data::from_bytes(&vr4_data)?;
let tile = vr4.get(vr4_index).unwrap();
tile.pixel(3, 2)       // u8 — palette index at (x=3, y=2)
tile.row(0)            // &[u8] — 8 pixels

// WPE: 256-color palette
let palette = Palette::from_bytes(&wpe_data)?;
palette.color(42)      // PaletteColor { r, g, b }
palette.to_rgba(42)    // u32 — 0xRRGGBBAA
```
