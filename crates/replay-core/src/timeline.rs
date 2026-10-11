use std::collections::BTreeMap;

use crate::analysis::BuildAction;
use crate::gamedata;
use crate::header::Race;
use crate::production::{ProductionEntry, ProductionModel, STARTING_WORKERS};

/// A snapshot of a player's state at a specific point in the game.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerState {
    pub player_id: u8,
    pub minerals_invested: u32,
    pub gas_invested: u32,
    /// Supply in use (starting workers included), from the production model.
    pub supply_used: u32,
    /// Supply provided by completed buildings/overlords (max 200).
    pub supply_max: u32,
    /// unit_type_id → count produced (cumulative, not accounting for losses),
    /// starting units included.
    pub units: BTreeMap<u16, u32>,
    /// building_type_id → count produced, starting main building included.
    pub buildings: BTreeMap<u16, u32>,
    /// Researched tech IDs.
    pub techs: Vec<u8>,
    /// upgrade_id → level.
    pub upgrades: BTreeMap<u8, u8>,
}

impl PlayerState {
    fn new(player_id: u8, race: Race) -> Self {
        let mut units = BTreeMap::new();
        let mut buildings = BTreeMap::new();
        let start = match race {
            Race::Terran => Some((7, 106)),
            Race::Protoss => Some((64, 154)),
            Race::Zerg => Some((41, 131)),
            Race::Unknown(_) => None,
        };
        if let Some((worker, main)) = start {
            units.insert(worker, STARTING_WORKERS);
            buildings.insert(main, 1);
        }
        if race == Race::Zerg {
            units.insert(42, 1); // Overlord
        }
        Self {
            player_id,
            minerals_invested: 0,
            gas_invested: 0,
            supply_used: if start.is_some() { STARTING_WORKERS } else { 0 },
            supply_max: crate::production::starting_capacity(race),
            units,
            buildings,
            techs: Vec::new(),
            upgrades: BTreeMap::new(),
        }
    }

    fn apply(&mut self, entry: &ProductionEntry) {
        if entry.dropped {
            return;
        }
        let n = entry.count;
        match &entry.action {
            BuildAction::Build(id) | BuildAction::BuildingMorph(id) => {
                let (min, gas, _) = gamedata::unit_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                *self.buildings.entry(*id).or_insert(0) += 1;
            }
            BuildAction::Train(id) | BuildAction::UnitMorph(id) => {
                // A Zergling/Scourge larva hatches a pair.
                let per_order = if matches!(id, 37 | 47) { 2 } else { 1 };
                let (min, gas, _) = gamedata::unit_cost(*id);
                self.minerals_invested += min * per_order * n;
                self.gas_invested += gas * per_order * n;
                *self.units.entry(*id).or_insert(0) += per_order * n;
            }
            BuildAction::Research(id) => {
                let (min, gas) = gamedata::tech_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                if !self.techs.contains(id) {
                    self.techs.push(*id);
                }
            }
            BuildAction::Upgrade(id) => {
                let (min, gas) = gamedata::upgrade_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                let level = self.upgrades.entry(*id).or_insert(0);
                *level += 1;
            }
            BuildAction::TrainFighter => {
                // Interceptor: 25 minerals, Scarab: 15 minerals
                // We don't know which, default to interceptor
                self.minerals_invested += 25;
            }
        }
    }
}

/// A snapshot of the full game state at a specific frame.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineSnapshot {
    pub frame: u32,
    pub real_seconds: f64,
    pub players: Vec<PlayerState>,
}

/// Build a timeline of game state snapshots from the production model.
///
/// Creates an initial snapshot at frame 0 (starting workers and main
/// building) and one after each build order entry, so the timeline can be
/// scrubbed to any build action. Orders the model judged as not carried out
/// leave the state unchanged. Supply comes from
/// [`ProductionModel::supply_at`].
pub fn build_timeline(model: &ProductionModel, players: &[(u8, Race)]) -> Vec<TimelineSnapshot> {
    let mut states: BTreeMap<u8, PlayerState> = players
        .iter()
        .map(|&(pid, race)| (pid, PlayerState::new(pid, race)))
        .collect();

    let mut snapshots = Vec::with_capacity(model.entries.len() + 1);
    snapshots.push(TimelineSnapshot {
        frame: 0,
        real_seconds: 0.0,
        players: states.values().cloned().collect(),
    });

    for entry in &model.entries {
        if let Some(state) = states.get_mut(&entry.player_id) {
            state.apply(entry);
            let (used, max) = model.supply_at(entry.player_id, entry.frame);
            state.supply_used = used;
            state.supply_max = max;
        }

        snapshots.push(TimelineSnapshot {
            frame: entry.frame,
            real_seconds: entry.real_seconds,
            players: states.values().cloned().collect(),
        });
    }

    snapshots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, GameCommand};
    use crate::production::model_production;

    fn cmd(frame: u32, command: Command) -> GameCommand {
        GameCommand {
            frame,
            player_id: 0,
            command,
        }
    }

    fn timeline(cmds: &[GameCommand], race: Race) -> Vec<TimelineSnapshot> {
        let players = [(0, race)];
        build_timeline(&model_production(cmds, &players), &players)
    }

    fn build(frame: u32, unit_type: u16) -> GameCommand {
        cmd(
            frame,
            Command::Build {
                order: 0x1E,
                x: 0,
                y: 0,
                unit_type,
            },
        )
    }

    #[test]
    fn test_timeline_initial_snapshot_has_starting_units() {
        let tl = timeline(&[], Race::Zerg);
        let p0 = &tl[0].players[0];
        assert_eq!(p0.units[&41], 4);
        assert_eq!(p0.units[&42], 1);
        assert_eq!(p0.buildings[&131], 1);
        assert_eq!((p0.supply_used, p0.supply_max), (4, 9));
    }

    #[test]
    fn test_timeline_tracks_investment() {
        let cmds = vec![
            build(100, 111),                            // Barracks: 150 min, ready 1300
            cmd(1400, Command::Train { unit_type: 0 }), // Marine: 50 min
            cmd(1500, Command::Train { unit_type: 0 }), // Marine: 50 min (queued)
        ];
        let tl = timeline(&cmds, Race::Terran);
        let last = tl.last().unwrap();
        let p0 = &last.players[0];
        assert_eq!(p0.minerals_invested, 250);
        assert_eq!(p0.gas_invested, 0);
        assert_eq!(p0.units[&0], 2);
        assert_eq!(p0.units[&7], 4);
        assert_eq!(p0.buildings[&111], 1);
        assert_eq!(p0.buildings[&106], 1);
        // Second marine waits for the first (360 frames), so at frame 1500
        // only one has started.
        assert_eq!(p0.supply_used, 5);
        assert_eq!(p0.supply_max, 10);
    }

    #[test]
    fn test_timeline_tracks_tech() {
        let cmds = vec![
            cmd(500, Command::Research { tech_type: 0 }),
            cmd(600, Command::Upgrade { upgrade_type: 27 }),
        ];
        let tl = timeline(&cmds, Race::Terran);
        let p0 = &tl.last().unwrap().players[0];
        assert_eq!(p0.techs, vec![0]);
        assert_eq!(p0.upgrades[&27], 1);
        assert_eq!(p0.minerals_invested, 200);
        assert_eq!(p0.gas_invested, 200);
    }

    #[test]
    fn test_timeline_two_players() {
        let cmds = vec![
            cmd(100, Command::Train { unit_type: 7 }),
            GameCommand {
                frame: 100,
                player_id: 1,
                command: Command::Train { unit_type: 64 },
            },
        ];
        let players = [(0, Race::Terran), (1, Race::Protoss)];
        let tl = build_timeline(&model_production(&cmds, &players), &players);
        let last = tl.last().unwrap();
        assert_eq!(last.players.len(), 2);
        assert_eq!(last.players[0].supply_used, 5);
        assert_eq!(last.players[1].supply_used, 5);
    }

    #[test]
    fn test_supply_max_counts_completed_depots_only() {
        let cmds = vec![
            build(100, 109), // Supply Depot, ready at 700
            cmd(200, Command::Train { unit_type: 7 }),
            cmd(800, Command::Train { unit_type: 7 }),
        ];
        let tl = timeline(&cmds, Race::Terran);
        assert_eq!(tl[1].players[0].supply_max, 10);
        let p0 = &tl.last().unwrap().players[0];
        assert_eq!(p0.supply_max, 18);
        assert_eq!(p0.supply_used, 6);
    }

    #[test]
    fn test_timeline_zergling_morph_hatches_pair() {
        let cmds = vec![
            cmd(
                2001,
                Command::Select {
                    unit_tags: vec![1, 2],
                },
            ),
            cmd(2010, Command::UnitMorph { unit_type: 37 }),
        ];
        let tl = timeline(&cmds, Race::Zerg);
        let p0 = &tl.last().unwrap().players[0];
        assert_eq!(p0.units[&37], 4);
        assert_eq!(p0.minerals_invested, 100);
        assert_eq!(p0.supply_used, 6);
    }
}
