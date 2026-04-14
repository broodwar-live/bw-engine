//! Replay collection statistics.
//!
//! Aggregates stats across multiple replays for the broodwar.live stats
//! features: matchup winrates, map popularity, build order frequencies,
//! and duration distributions.
//!
//! ## Usage
//!
//! ```ignore
//! let mut collector = StatsCollector::new();
//! for replay in replays {
//!     collector.add(&replay);
//! }
//! let report = collector.report();
//! ```

use std::collections::HashMap;

use crate::Replay;
use crate::metadata::{GameMetadata, GameResult};

/// Collects statistics across multiple replays.
#[derive(Debug, Default)]
pub struct StatsCollector {
    total_replays: u32,
    total_1v1: u32,
    /// Matchup → (wins_for_first_race, total_games).
    /// "TvZ" → (terran_wins, total). First letter = first race alphabetically.
    matchup_stats: HashMap<String, MatchupRecord>,
    /// Normalized map name → play count.
    map_counts: HashMap<String, u32>,
    /// Matchup → total duration in seconds (for averaging).
    matchup_duration: HashMap<String, (f64, u32)>,
    /// Race → games played.
    race_popularity: HashMap<String, u32>,
}

/// Win/loss record for a matchup.
#[derive(Debug, Clone, Default)]
struct MatchupRecord {
    /// Wins for the alphabetically-first race (P in PvT, T in TvZ, etc.).
    first_race_wins: u32,
    /// Wins for the alphabetically-second race.
    second_race_wins: u32,
    /// Games with unknown result.
    draws: u32,
}

impl MatchupRecord {
    fn total(&self) -> u32 {
        self.first_race_wins + self.second_race_wins + self.draws
    }
}

/// Aggregated stats report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StatsReport {
    pub total_replays: u32,
    pub total_1v1: u32,
    pub matchup_winrates: Vec<MatchupWinrate>,
    pub map_popularity: Vec<MapStat>,
    pub race_popularity: Vec<RaceStat>,
    pub matchup_durations: Vec<MatchupDuration>,
}

/// Winrate for a specific matchup.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchupWinrate {
    /// Canonical matchup code (e.g., "TvZ").
    pub matchup: String,
    /// Total games in this matchup.
    pub games: u32,
    /// Winrate for the first race (0.0–1.0). In "TvZ", this is Terran's winrate.
    pub first_race_winrate: f64,
    /// Winrate for the second race.
    pub second_race_winrate: f64,
}

/// Map play count.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MapStat {
    pub map_name: String,
    pub games: u32,
    pub percentage: f64,
}

/// Race popularity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RaceStat {
    pub race: String,
    pub games: u32,
    pub percentage: f64,
}

/// Average game duration by matchup.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchupDuration {
    pub matchup: String,
    pub avg_duration_secs: f64,
    pub games: u32,
}

impl StatsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a replay's metadata to the collection.
    pub fn add_metadata(&mut self, meta: &GameMetadata) {
        self.total_replays += 1;

        // Map popularity.
        *self.map_counts.entry(meta.map_name.clone()).or_insert(0) += 1;

        if !meta.is_1v1 {
            return;
        }
        self.total_1v1 += 1;

        // Matchup stats.
        if let Some(matchup) = &meta.matchup {
            let record = self.matchup_stats.entry(matchup.code.clone()).or_default();
            match &meta.result {
                GameResult::Winner { player_name, .. } => {
                    // Determine which race won. The first letter of the matchup code
                    // is the "first race" (alphabetically).
                    let first_race_code = &matchup.code[..1];
                    // We need the winner's race code. Since we only have the player name,
                    // we use a heuristic: if it's a mirror, count as first_race_win.
                    if matchup.mirror {
                        record.first_race_wins += 1;
                    } else {
                        // We store the winner determination from add_replay which has
                        // access to player data.
                        // For metadata-only, we track as draw since we can't determine race
                        // from name alone.
                        let _ = (first_race_code, player_name);
                        record.draws += 1;
                    }
                }
                GameResult::Unknown => {
                    record.draws += 1;
                }
            }

            // Duration tracking.
            let (total, count) = self
                .matchup_duration
                .entry(matchup.code.clone())
                .or_insert((0.0, 0));
            *total += meta.duration_secs;
            *count += 1;
        }
    }

    /// Add a full replay to the collection (has access to player data for race-aware winrates).
    pub fn add(&mut self, replay: &Replay) {
        self.total_replays += 1;

        let meta = &replay.metadata;
        *self.map_counts.entry(meta.map_name.clone()).or_insert(0) += 1;

        // Track race popularity.
        for player in &replay.header.players {
            let race_str = player.race.code().to_string();
            *self.race_popularity.entry(race_str).or_insert(0) += 1;
        }

        if !meta.is_1v1 {
            return;
        }
        self.total_1v1 += 1;

        if let Some(matchup) = &meta.matchup {
            let record = self.matchup_stats.entry(matchup.code.clone()).or_default();

            match &meta.result {
                GameResult::Winner { player_id, .. } => {
                    if matchup.mirror {
                        record.first_race_wins += 1;
                    } else {
                        // Find winner's race.
                        let winner_race = replay
                            .header
                            .players
                            .iter()
                            .find(|p| p.player_id == *player_id)
                            .map(|p| p.race.code());

                        let first_race = &matchup.code[..1];
                        if winner_race == Some(first_race) {
                            record.first_race_wins += 1;
                        } else {
                            record.second_race_wins += 1;
                        }
                    }
                }
                GameResult::Unknown => {
                    record.draws += 1;
                }
            }

            let (total, count) = self
                .matchup_duration
                .entry(matchup.code.clone())
                .or_insert((0.0, 0));
            *total += meta.duration_secs;
            *count += 1;
        }
    }

    /// Generate the aggregated stats report.
    pub fn report(&self) -> StatsReport {
        // Matchup winrates.
        let mut matchup_winrates: Vec<MatchupWinrate> = self
            .matchup_stats
            .iter()
            .map(|(matchup, record)| {
                let decided = record.first_race_wins + record.second_race_wins;
                let (wr1, wr2) = if decided > 0 {
                    (
                        record.first_race_wins as f64 / decided as f64,
                        record.second_race_wins as f64 / decided as f64,
                    )
                } else {
                    (0.0, 0.0)
                };
                MatchupWinrate {
                    matchup: matchup.clone(),
                    games: record.total(),
                    first_race_winrate: wr1,
                    second_race_winrate: wr2,
                }
            })
            .collect();
        matchup_winrates.sort_by(|a, b| b.games.cmp(&a.games));

        // Map popularity.
        let total = self.total_replays.max(1) as f64;
        let mut map_popularity: Vec<MapStat> = self
            .map_counts
            .iter()
            .map(|(name, &count)| MapStat {
                map_name: name.clone(),
                games: count,
                percentage: count as f64 / total * 100.0,
            })
            .collect();
        map_popularity.sort_by(|a, b| b.games.cmp(&a.games));

        // Race popularity.
        let total_player_slots: f64 = self.race_popularity.values().sum::<u32>() as f64;
        let total_player_slots = total_player_slots.max(1.0);
        let mut race_popularity: Vec<RaceStat> = self
            .race_popularity
            .iter()
            .map(|(race, &count)| RaceStat {
                race: race.clone(),
                games: count,
                percentage: count as f64 / total_player_slots * 100.0,
            })
            .collect();
        race_popularity.sort_by(|a, b| b.games.cmp(&a.games));

        // Matchup durations.
        let mut matchup_durations: Vec<MatchupDuration> = self
            .matchup_duration
            .iter()
            .map(|(matchup, (total_secs, count))| MatchupDuration {
                matchup: matchup.clone(),
                avg_duration_secs: if *count > 0 {
                    total_secs / *count as f64
                } else {
                    0.0
                },
                games: *count,
            })
            .collect();
        matchup_durations.sort_by(|a, b| b.games.cmp(&a.games));

        StatsReport {
            total_replays: self.total_replays,
            total_1v1: self.total_1v1,
            matchup_winrates,
            map_popularity,
            race_popularity,
            matchup_durations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::*;
    use crate::metadata::Matchup;

    fn make_meta(
        matchup_code: &str,
        map: &str,
        winner_id: Option<u8>,
        duration: f64,
    ) -> GameMetadata {
        let mirror = matchup_code.chars().next() == matchup_code.chars().nth(2);
        GameMetadata {
            matchup: Some(Matchup {
                code: matchup_code.to_string(),
                mirror,
            }),
            map_name: map.to_string(),
            map_name_raw: map.to_string(),
            result: match winner_id {
                Some(id) => GameResult::Winner {
                    player_id: id,
                    player_name: format!("Player{id}"),
                },
                None => GameResult::Unknown,
            },
            duration_secs: duration,
            is_1v1: true,
            player_count: 2,
        }
    }

    fn make_replay(race0: Race, race1: Race, winner_id: u8, map: &str) -> Replay {
        let players = vec![
            Player {
                slot_id: 0,
                player_id: 0,
                player_type: PlayerType::Human,
                race: race0,
                team: 0,
                name: "Player0".to_string(),
                color: 0,
            },
            Player {
                slot_id: 1,
                player_id: 1,
                player_type: PlayerType::Human,
                race: race1,
                team: 1,
                name: "Player1".to_string(),
                color: 1,
            },
        ];
        let header = Header {
            engine: Engine::BroodWar,
            frame_count: 10000,
            start_time: 0,
            game_title: String::new(),
            map_width: 128,
            map_height: 128,
            game_speed: Speed::Fastest,
            game_type: GameType::Melee,
            host_name: String::new(),
            map_name: map.to_string(),
            players,
        };
        let meta = crate::metadata::extract_metadata(
            &header,
            &[crate::command::GameCommand {
                frame: 5000,
                player_id: 1 - winner_id,
                command: crate::command::Command::LeaveGame { reason: 1 },
            }],
        );
        Replay {
            header,
            commands: vec![],
            build_order: vec![],
            player_apm: vec![],
            timeline: vec![],
            metadata: meta,
            map_data: vec![],
        }
    }

    #[test]
    fn test_basic_collection() {
        let mut c = StatsCollector::new();
        c.add_metadata(&make_meta("TvZ", "Fighting Spirit", Some(0), 600.0));
        c.add_metadata(&make_meta("TvZ", "Fighting Spirit", Some(1), 500.0));
        c.add_metadata(&make_meta("PvT", "Polypoid", None, 400.0));

        let report = c.report();
        assert_eq!(report.total_replays, 3);
        assert_eq!(report.total_1v1, 3);
        assert_eq!(report.matchup_winrates.len(), 2);
        assert_eq!(report.map_popularity.len(), 2);
    }

    #[test]
    fn test_race_aware_winrates() {
        let mut c = StatsCollector::new();
        // TvZ: Terran wins 2, Zerg wins 1.
        c.add(&make_replay(Race::Terran, Race::Zerg, 0, "FS")); // T wins
        c.add(&make_replay(Race::Terran, Race::Zerg, 0, "FS")); // T wins
        c.add(&make_replay(Race::Zerg, Race::Terran, 1, "FS")); // T wins (player 1 is T)

        let report = c.report();
        let tvz = report
            .matchup_winrates
            .iter()
            .find(|m| m.matchup == "TvZ")
            .unwrap();
        assert_eq!(tvz.games, 3);
        // All 3 games won by Terran → first_race_winrate = 1.0.
        assert!((tvz.first_race_winrate - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_map_popularity() {
        let mut c = StatsCollector::new();
        c.add_metadata(&make_meta("TvZ", "Fighting Spirit", Some(0), 600.0));
        c.add_metadata(&make_meta("TvZ", "Fighting Spirit", Some(0), 600.0));
        c.add_metadata(&make_meta("TvZ", "Polypoid", Some(0), 600.0));

        let report = c.report();
        assert_eq!(report.map_popularity[0].map_name, "Fighting Spirit");
        assert_eq!(report.map_popularity[0].games, 2);
        assert!((report.map_popularity[0].percentage - 66.67).abs() < 1.0);
    }

    #[test]
    fn test_matchup_duration() {
        let mut c = StatsCollector::new();
        c.add_metadata(&make_meta("TvZ", "FS", Some(0), 600.0));
        c.add_metadata(&make_meta("TvZ", "FS", Some(0), 400.0));

        let report = c.report();
        let tvz = report
            .matchup_durations
            .iter()
            .find(|m| m.matchup == "TvZ")
            .unwrap();
        assert!((tvz.avg_duration_secs - 500.0).abs() < 0.01);
    }

    #[test]
    fn test_race_popularity() {
        let mut c = StatsCollector::new();
        c.add(&make_replay(Race::Terran, Race::Zerg, 0, "FS"));
        c.add(&make_replay(Race::Terran, Race::Protoss, 0, "FS"));

        let report = c.report();
        let terran = report
            .race_popularity
            .iter()
            .find(|r| r.race == "T")
            .unwrap();
        assert_eq!(terran.games, 2); // Terran in both games.
    }

    #[test]
    fn test_empty_collector() {
        let c = StatsCollector::new();
        let report = c.report();
        assert_eq!(report.total_replays, 0);
        assert!(report.matchup_winrates.is_empty());
    }
}
