use std::collections::BTreeMap;

use crate::analysis::{BuildAction, BuildOrderEntry};
use crate::gamedata;

/// A snapshot of a player's state at a specific point in the game.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerState {
    pub player_id: u8,
    pub minerals_invested: u32,
    pub gas_invested: u32,
    pub supply_used: u32,
    pub supply_max: u32,
    /// unit_type_id → count produced (cumulative, not accounting for losses).
    pub units: BTreeMap<u16, u32>,
    /// building_type_id → count produced.
    pub buildings: BTreeMap<u16, u32>,
    /// Researched tech IDs.
    pub techs: Vec<u8>,
    /// upgrade_id → level.
    pub upgrades: BTreeMap<u8, u8>,
}

impl PlayerState {
    fn new(player_id: u8) -> Self {
        Self {
            player_id,
            minerals_invested: 0,
            gas_invested: 0,
            supply_used: 0,
            supply_max: 0,
            units: BTreeMap::new(),
            buildings: BTreeMap::new(),
            techs: Vec::new(),
            upgrades: BTreeMap::new(),
        }
    }

    fn apply(&mut self, action: &BuildAction) {
        match action {
            BuildAction::Build(id) => {
                let (min, gas, _) = gamedata::unit_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                *self.buildings.entry(*id).or_insert(0) += 1;
                self.supply_max += gamedata::supply_provided(*id);
            }
            BuildAction::Train(id) | BuildAction::UnitMorph(id) => {
                let (min, gas, supply) = gamedata::unit_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                self.supply_used += supply;
                *self.units.entry(*id).or_insert(0) += 1;
                // Overlords/depots/pylons provide supply
                self.supply_max += gamedata::supply_provided(*id);
            }
            BuildAction::BuildingMorph(id) => {
                let (min, gas, _) = gamedata::unit_cost(*id);
                self.minerals_invested += min;
                self.gas_invested += gas;
                *self.buildings.entry(*id).or_insert(0) += 1;
                self.supply_max += gamedata::supply_provided(*id);
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

/// Build a timeline of game state snapshots from the build order.
///
/// Creates a snapshot after each build order event, so the timeline
/// can be scrubbed to any build action.
pub fn build_timeline(build_order: &[BuildOrderEntry], player_ids: &[u8]) -> Vec<TimelineSnapshot> {
    let mut states: BTreeMap<u8, PlayerState> = player_ids
        .iter()
        .map(|&pid| (pid, PlayerState::new(pid)))
        .collect();

    // Initial snapshot at frame 0
    let mut snapshots = vec![TimelineSnapshot {
        frame: 0,
        real_seconds: 0.0,
        players: states.values().cloned().collect(),
    }];

    for entry in build_order {
        if let Some(state) = states.get_mut(&entry.player_id) {
            state.apply(&entry.action);
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
    use crate::analysis::BuildAction;

    fn entry(frame: u32, player_id: u8, action: BuildAction) -> BuildOrderEntry {
        BuildOrderEntry {
            frame,
            real_seconds: frame as f64 / 23.81,
            player_id,
            action,
        }
    }

    #[test]
    fn test_timeline_tracks_investment() {
        let bo = vec![
            entry(100, 0, BuildAction::Train(0)),   // Marine: 50 min
            entry(200, 0, BuildAction::Train(0)),   // Marine: 50 min
            entry(300, 0, BuildAction::Build(111)), // Barracks: 150 min
        ];

        let timeline = build_timeline(&bo, &[0]);

        // After 2 marines + barracks
        let last = timeline.last().unwrap();
        let p0 = &last.players[0];
        assert_eq!(p0.minerals_invested, 250); // 50+50+150
        assert_eq!(p0.gas_invested, 0);
        assert_eq!(p0.supply_used, 2); // 2 marines
        assert_eq!(p0.units[&0], 2); // 2 marines
        assert_eq!(p0.buildings[&111], 1); // 1 barracks
    }

    #[test]
    fn test_timeline_tracks_tech() {
        let bo = vec![
            entry(500, 0, BuildAction::Research(0)), // Stim: 100/100
            entry(600, 0, BuildAction::Upgrade(27)), // Metabolic Boost: 100/100
        ];

        let timeline = build_timeline(&bo, &[0]);
        let last = timeline.last().unwrap();
        let p0 = &last.players[0];
        assert_eq!(p0.techs, vec![0]); // Stim researched
        assert_eq!(p0.upgrades[&27], 1); // Metabolic Boost level 1
        assert_eq!(p0.minerals_invested, 200);
        assert_eq!(p0.gas_invested, 200);
    }

    #[test]
    fn test_timeline_two_players() {
        let bo = vec![
            entry(100, 0, BuildAction::Train(7)),  // SCV
            entry(100, 1, BuildAction::Train(64)), // Probe
        ];

        let timeline = build_timeline(&bo, &[0, 1]);
        let last = timeline.last().unwrap();
        assert_eq!(last.players.len(), 2);
    }

    #[test]
    fn test_supply_tracking() {
        let bo = vec![
            entry(100, 0, BuildAction::Build(109)), // Supply Depot: +8 supply
            entry(200, 0, BuildAction::Train(0)),   // Marine: 1 supply
            entry(300, 0, BuildAction::Train(0)),   // Marine: 1 supply
        ];

        let timeline = build_timeline(&bo, &[0]);
        let last = timeline.last().unwrap();
        let p0 = &last.players[0];
        assert_eq!(p0.supply_max, 8);
        assert_eq!(p0.supply_used, 2);
    }
}
