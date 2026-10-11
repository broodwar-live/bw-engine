use crate::command::{Command, GameCommand, HotkeyAction};
use crate::gamedata;

/// Frames per second at "Fastest" game speed.
const FPS_FASTEST: f64 = 23.81;

/// A build order entry: a production/tech action by a player at a game frame.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildOrderEntry {
    pub frame: u32,
    pub real_seconds: f64,
    pub player_id: u8,
    pub action: BuildAction,
}

/// The type of build order action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum BuildAction {
    /// Build a structure (unit_type).
    Build(u16),
    /// Train a unit (unit_type).
    Train(u16),
    /// Morph a unit — e.g. Zergling→Lurker (unit_type of the result).
    UnitMorph(u16),
    /// Morph a building — e.g. Lair, Hive, Greater Spire (unit_type of result).
    BuildingMorph(u16),
    /// Research a technology (tech_type).
    Research(u8),
    /// Start an upgrade (upgrade_type).
    Upgrade(u8),
    /// Train interceptor or scarab.
    TrainFighter,
}

impl BuildAction {
    /// Human-readable name of the action.
    pub fn name(&self) -> &'static str {
        match self {
            BuildAction::Build(id)
            | BuildAction::Train(id)
            | BuildAction::UnitMorph(id)
            | BuildAction::BuildingMorph(id) => gamedata::unit_name(*id),
            BuildAction::Research(id) => gamedata::tech_name(*id),
            BuildAction::Upgrade(id) => gamedata::upgrade_name(*id),
            BuildAction::TrainFighter => "Interceptor/Scarab",
        }
    }
}

impl std::fmt::Display for BuildAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildAction::Build(_) => write!(f, "Build {}", self.name()),
            BuildAction::Train(_) => write!(f, "Train {}", self.name()),
            BuildAction::UnitMorph(_) => write!(f, "Morph {}", self.name()),
            BuildAction::BuildingMorph(_) => write!(f, "Morph {}", self.name()),
            BuildAction::Research(_) => write!(f, "Research {}", self.name()),
            BuildAction::Upgrade(_) => write!(f, "Upgrade {}", self.name()),
            BuildAction::TrainFighter => write!(f, "Train {}", self.name()),
        }
    }
}

/// Per-player APM (actions per minute) computed over the game duration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerApm {
    pub player_id: u8,
    pub apm: f64,
    pub eapm: f64,
}

/// APM sampled over time for graphing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApmSample {
    pub frame: u32,
    pub real_seconds: f64,
    pub player_id: u8,
    pub apm: f64,
    pub eapm: f64,
}

/// Repeated `Build` orders for the same structure type by the same player
/// within this many frames (about one real second) are UI double-clicks or
/// placement retries; only the first is kept.
pub const BUILD_RETRY_FRAMES: u32 = 24;

/// Extract the build order from the command stream.
///
/// Duplicate `Build` orders (see [`BUILD_RETRY_FRAMES`]) are dropped; every
/// other production order is kept as issued. Whether an order could actually
/// be carried out (queue space, larvae, supply) is estimated separately by
/// [`crate::production`].
pub fn extract_build_order(commands: &[GameCommand]) -> Vec<BuildOrderEntry> {
    build_orders_with_selection(commands)
        .into_iter()
        .map(|(entry, _)| entry)
        .collect()
}

/// The build order together with how many units the issuing player had
/// selected for each order (at least 1). One Zerg morph order applies to every
/// selected larva, so this is needed to count multi-larva morphs.
pub(crate) fn build_orders_with_selection(
    commands: &[GameCommand],
) -> Vec<(BuildOrderEntry, Selection)> {
    let mut selections = SelectionTracker::default();
    // (player, building type) -> frame of the last kept Build order.
    let mut last_build: Vec<(u8, u16, u32)> = Vec::new();
    let mut out = Vec::new();

    for c in commands {
        selections.observe(c);
        let action = match &c.command {
            Command::Build { unit_type, .. } => BuildAction::Build(*unit_type),
            Command::Train { unit_type } => BuildAction::Train(*unit_type),
            Command::UnitMorph { unit_type } => BuildAction::UnitMorph(*unit_type),
            Command::BuildingMorph { unit_type } => BuildAction::BuildingMorph(*unit_type),
            Command::Research { tech_type } => BuildAction::Research(*tech_type),
            Command::Upgrade { upgrade_type } => BuildAction::Upgrade(*upgrade_type),
            Command::TrainFighter => BuildAction::TrainFighter,
            _ => continue,
        };

        if let BuildAction::Build(unit_type) = action {
            let key = last_build
                .iter_mut()
                .find(|(p, t, _)| *p == c.player_id && *t == unit_type);
            match key {
                Some((_, _, frame)) if c.frame.saturating_sub(*frame) <= BUILD_RETRY_FRAMES => {
                    continue;
                }
                Some((_, _, frame)) => *frame = c.frame,
                None => last_build.push((c.player_id, unit_type, c.frame)),
            }
        }

        out.push((
            BuildOrderEntry {
                frame: c.frame,
                real_seconds: frame_to_seconds(c.frame),
                player_id: c.player_id,
                action,
            },
            selections.selected(c.player_id),
        ));
    }
    out
}

/// The issuing player's selection when an order was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selection {
    /// Number of selected units (at least 1).
    pub count: u16,
    /// Changes whenever the player's selection changes, so two orders with
    /// the same generation were given to the same units.
    pub generation: u32,
}

/// Follows each player's current selection through select and hotkey
/// commands. Only the number of selected units matters here.
#[derive(Default)]
struct SelectionTracker {
    players: Vec<(u8, PlayerSelection)>,
}

#[derive(Default)]
struct PlayerSelection {
    current: Vec<u16>,
    generation: u32,
    groups: [Vec<u16>; 10],
}

impl SelectionTracker {
    fn player(&mut self, id: u8) -> &mut PlayerSelection {
        let idx = match self.players.iter().position(|(p, _)| *p == id) {
            Some(i) => i,
            None => {
                self.players.push((id, PlayerSelection::default()));
                self.players.len() - 1
            }
        };
        &mut self.players[idx].1
    }

    fn observe(&mut self, c: &GameCommand) {
        let sel = self.player(c.player_id);
        if matches!(
            c.command,
            Command::Select { .. }
                | Command::SelectAdd { .. }
                | Command::SelectRemove { .. }
                | Command::Hotkey {
                    action: HotkeyAction::Select,
                    ..
                }
        ) {
            sel.generation += 1;
        }
        match &c.command {
            Command::Select { unit_tags } => sel.current = unit_tags.clone(),
            Command::SelectAdd { unit_tags } => {
                for t in unit_tags {
                    if !sel.current.contains(t) {
                        sel.current.push(*t);
                    }
                }
            }
            Command::SelectRemove { unit_tags } => sel.current.retain(|t| !unit_tags.contains(t)),
            Command::Hotkey { action, group } => {
                let Some(slot) = sel.groups.get_mut(*group as usize) else {
                    return;
                };
                match action {
                    HotkeyAction::Assign => *slot = sel.current.clone(),
                    HotkeyAction::Select => {
                        if !slot.is_empty() {
                            sel.current = slot.clone();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn selected(&mut self, id: u8) -> Selection {
        let sel = self.player(id);
        Selection {
            count: (sel.current.len() as u16).max(1),
            generation: sel.generation,
        }
    }
}

/// Calculate overall APM and EAPM for each player.
pub fn calculate_apm(commands: &[GameCommand], total_frames: u32) -> Vec<PlayerApm> {
    let duration_minutes = frame_to_seconds(total_frames) / 60.0;
    if duration_minutes < 0.01 {
        return vec![];
    }

    // Collect unique player IDs.
    let mut player_ids: Vec<u8> = commands.iter().map(|c| c.player_id).collect();
    player_ids.sort_unstable();
    player_ids.dedup();

    player_ids
        .into_iter()
        .map(|pid| {
            let meaningful = commands
                .iter()
                .filter(|c| c.player_id == pid && c.command.is_meaningful_action())
                .count();
            let effective = commands
                .iter()
                .filter(|c| c.player_id == pid && c.command.is_effective_action())
                .count();

            PlayerApm {
                player_id: pid,
                apm: meaningful as f64 / duration_minutes,
                eapm: effective as f64 / duration_minutes,
            }
        })
        .collect()
}

/// Calculate APM over time in sliding windows for graphing.
/// `window_frames` controls the window size (e.g., 60 seconds = ~1428 frames).
/// `step_frames` controls how often to sample (e.g., every 10 seconds = ~238 frames).
pub fn calculate_apm_over_time(
    commands: &[GameCommand],
    total_frames: u32,
    window_frames: u32,
    step_frames: u32,
) -> Vec<ApmSample> {
    if total_frames == 0 || step_frames == 0 || window_frames == 0 {
        return vec![];
    }

    let mut player_ids: Vec<u8> = commands.iter().map(|c| c.player_id).collect();
    player_ids.sort_unstable();
    player_ids.dedup();

    let window_minutes = frame_to_seconds(window_frames) / 60.0;
    let mut samples = Vec::new();

    let mut frame = 0;
    while frame <= total_frames {
        let window_start = frame.saturating_sub(window_frames);
        let window_end = frame;

        for &pid in &player_ids {
            let meaningful = commands
                .iter()
                .filter(|c| {
                    c.player_id == pid
                        && c.frame >= window_start
                        && c.frame < window_end
                        && c.command.is_meaningful_action()
                })
                .count();
            let effective = commands
                .iter()
                .filter(|c| {
                    c.player_id == pid
                        && c.frame >= window_start
                        && c.frame < window_end
                        && c.command.is_effective_action()
                })
                .count();

            samples.push(ApmSample {
                frame,
                real_seconds: frame_to_seconds(frame),
                player_id: pid,
                apm: meaningful as f64 / window_minutes,
                eapm: effective as f64 / window_minutes,
            });
        }

        frame += step_frames;
    }

    samples
}

/// Convert a game frame to real seconds at Fastest speed.
pub fn frame_to_seconds(frame: u32) -> f64 {
    frame as f64 / FPS_FASTEST
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::HotkeyAction;
    use crate::command::{Command, GameCommand};

    fn make_cmd(frame: u32, player_id: u8, command: Command) -> GameCommand {
        GameCommand {
            frame,
            player_id,
            command,
        }
    }

    #[test]
    fn test_extract_build_order() {
        let commands = vec![
            make_cmd(100, 0, Command::Select { unit_tags: vec![1] }),
            make_cmd(
                200,
                0,
                Command::Build {
                    order: 0,
                    x: 10,
                    y: 20,
                    unit_type: 0x7D,
                },
            ),
            make_cmd(300, 0, Command::Train { unit_type: 0x25 }),
            make_cmd(400, 1, Command::Research { tech_type: 5 }),
            make_cmd(500, 0, Command::Stop { queued: false }),
        ];

        let bo = extract_build_order(&commands);
        assert_eq!(bo.len(), 3);
        assert!(matches!(bo[0].action, BuildAction::Build(0x7D)));
        assert_eq!(bo[0].player_id, 0);
        assert!(matches!(bo[1].action, BuildAction::Train(0x25)));
        assert!(matches!(bo[2].action, BuildAction::Research(5)));
        assert_eq!(bo[2].player_id, 1);
    }

    fn build(frame: u32, player_id: u8, unit_type: u16, x: u16) -> GameCommand {
        make_cmd(
            frame,
            player_id,
            Command::Build {
                order: 0x1E,
                x,
                y: 10,
                unit_type,
            },
        )
    }

    #[test]
    fn test_extract_build_order_dedupes_build_retries_keeps_first() {
        let commands = vec![
            build(1000, 0, 156, 10), // Pylon
            build(1004, 0, 156, 10), // double click
            build(1020, 0, 156, 14), // placement retry elsewhere, < 1 s
            build(1010, 1, 156, 10), // other player: kept
            build(1030, 0, 160, 20), // Gateway: other type, kept
            build(1300, 0, 156, 30), // second Pylon much later: kept
        ];
        let bo = extract_build_order(&commands);
        let got: Vec<(u32, u8)> = bo.iter().map(|e| (e.frame, e.player_id)).collect();
        assert_eq!(got, vec![(1000, 0), (1010, 1), (1030, 0), (1300, 0)]);
    }

    #[test]
    fn test_extract_build_order_retry_window_is_about_one_second() {
        let commands = vec![
            build(1000, 0, 156, 10),
            build(1000 + BUILD_RETRY_FRAMES, 0, 156, 10),
            build(1000 + 2 * BUILD_RETRY_FRAMES + 1, 0, 156, 10),
        ];
        let bo = extract_build_order(&commands);
        assert_eq!(bo.len(), 2);
        assert_eq!(bo[1].frame, 1000 + 2 * BUILD_RETRY_FRAMES + 1);
    }

    #[test]
    fn test_build_orders_with_selection_tracks_select_and_hotkeys() {
        let commands = vec![
            make_cmd(
                1,
                0,
                Command::Select {
                    unit_tags: vec![1, 2, 3],
                },
            ),
            make_cmd(2, 0, Command::UnitMorph { unit_type: 41 }),
            make_cmd(
                3,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Assign,
                    group: 4,
                },
            ),
            make_cmd(4, 0, Command::SelectRemove { unit_tags: vec![2] }),
            make_cmd(5, 0, Command::UnitMorph { unit_type: 41 }),
            make_cmd(6, 0, Command::Select { unit_tags: vec![9] }),
            make_cmd(
                7,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Select,
                    group: 4,
                },
            ),
            make_cmd(
                8,
                0,
                Command::SelectAdd {
                    unit_tags: vec![7, 1],
                },
            ),
            make_cmd(9, 0, Command::UnitMorph { unit_type: 41 }),
            make_cmd(10, 1, Command::Train { unit_type: 64 }),
        ];
        let sel: Vec<(u16, u32)> = build_orders_with_selection(&commands)
            .iter()
            .map(|(_, s)| (s.count, s.generation))
            .collect();
        assert_eq!(sel, vec![(3, 1), (2, 2), (4, 5), (1, 0)]);
    }

    #[test]
    fn test_extract_build_order_dedupes_on_fixture() {
        // larva_vs_mini: Moo.Sapa clicked the Cybernetics Core three times
        // within a second (3:24-3:25); only the first order is kept.
        let path = format!(
            "{}/../../tests/fixtures/larva_vs_mini.rep",
            env!("CARGO_MANIFEST_DIR")
        );
        let replay = crate::parse(&std::fs::read(path).unwrap()).unwrap();
        let cores: Vec<u32> = replay
            .build_order
            .iter()
            .filter(|e| e.player_id == 1 && matches!(e.action, BuildAction::Build(164)))
            .map(|e| e.frame)
            .collect();
        assert!(!cores.is_empty());
        for w in cores.windows(2) {
            assert!(w[1] - w[0] > BUILD_RETRY_FRAMES, "{cores:?}");
        }
        assert!(cores[0] < 5000, "first core kept: {cores:?}");
    }

    #[test]
    fn test_calculate_apm() {
        // 1 minute of game = 23.81 * 60 = ~1428 frames
        let frames_per_min = (FPS_FASTEST * 60.0) as u32;

        let commands = vec![
            // 50 meaningful actions for player 0
            make_cmd(100, 0, Command::Train { unit_type: 1 }),
            make_cmd(
                200,
                0,
                Command::Build {
                    order: 0,
                    x: 0,
                    y: 0,
                    unit_type: 1,
                },
            ),
            // 1 non-meaningful action (selection)
            make_cmd(300, 0, Command::Select { unit_tags: vec![1] }),
            // 1 meaningful for player 1
            make_cmd(400, 1, Command::Train { unit_type: 1 }),
        ];

        let apms = calculate_apm(&commands, frames_per_min);
        assert_eq!(apms.len(), 2);

        // Player 0: 2 meaningful actions in 1 minute = 2 APM
        let p0 = apms.iter().find(|a| a.player_id == 0).unwrap();
        assert!((p0.apm - 2.0).abs() < 0.1);

        // Player 1: 1 meaningful action in 1 minute = 1 APM
        let p1 = apms.iter().find(|a| a.player_id == 1).unwrap();
        assert!((p1.apm - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_eapm_excludes_hotkey_recall() {
        let frames_per_min = (FPS_FASTEST * 60.0) as u32;

        let commands = vec![
            make_cmd(
                100,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Assign,
                    group: 1,
                },
            ),
            make_cmd(
                200,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Select,
                    group: 1,
                },
            ),
            make_cmd(300, 0, Command::Train { unit_type: 1 }),
        ];

        let apms = calculate_apm(&commands, frames_per_min);
        let p0 = &apms[0];
        // APM: 3 meaningful (assign + select-hotkey + train) — wait, select-hotkey is meaningful
        // Actually hotkey is meaningful. Let me check:
        // Hotkey assign: is_meaningful=true, is_effective=true
        // Hotkey select: is_meaningful=true, is_effective=false
        assert!((p0.apm - 3.0).abs() < 0.1);
        // EAPM: 2 effective (assign + train, not select)
        assert!((p0.eapm - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_apm_zero_duration() {
        let commands = vec![make_cmd(0, 0, Command::Train { unit_type: 1 })];
        let apms = calculate_apm(&commands, 0);
        assert!(apms.is_empty());
    }

    #[test]
    fn test_frame_to_seconds() {
        assert!((frame_to_seconds(0) - 0.0).abs() < 0.001);
        // 2381 frames at 23.81 fps = 100 seconds
        assert!((frame_to_seconds(2381) - 100.0).abs() < 0.1);
    }
}
