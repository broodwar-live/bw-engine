use crate::command::{Command, GameCommand};

/// Frames per second at "Fastest" game speed.
const FPS_FASTEST: f64 = 23.81;

/// A build order entry: a production/tech action by a player at a game frame.
#[derive(Debug, Clone)]
pub struct BuildOrderEntry {
    pub frame: u32,
    pub real_seconds: f64,
    pub player_id: u8,
    pub action: BuildAction,
}

/// The type of build order action.
#[derive(Debug, Clone)]
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

/// Per-player APM (actions per minute) computed over the game duration.
#[derive(Debug, Clone)]
pub struct PlayerApm {
    pub player_id: u8,
    pub apm: f64,
    pub eapm: f64,
}

/// APM sampled over time for graphing.
#[derive(Debug, Clone)]
pub struct ApmSample {
    pub frame: u32,
    pub real_seconds: f64,
    pub player_id: u8,
    pub apm: f64,
    pub eapm: f64,
}

/// Extract the build order from the command stream.
pub fn extract_build_order(commands: &[GameCommand]) -> Vec<BuildOrderEntry> {
    commands
        .iter()
        .filter(|c| c.command.is_build_order_action())
        .map(|c| {
            let action = match &c.command {
                Command::Build { unit_type, .. } => BuildAction::Build(*unit_type),
                Command::Train { unit_type } => BuildAction::Train(*unit_type),
                Command::UnitMorph { unit_type } => BuildAction::UnitMorph(*unit_type),
                Command::BuildingMorph { unit_type } => BuildAction::BuildingMorph(*unit_type),
                Command::Research { tech_type } => BuildAction::Research(*tech_type),
                Command::Upgrade { upgrade_type } => BuildAction::Upgrade(*upgrade_type),
                Command::TrainFighter => BuildAction::TrainFighter,
                _ => unreachable!("is_build_order_action was true"),
            };
            BuildOrderEntry {
                frame: c.frame,
                real_seconds: frame_to_seconds(c.frame),
                player_id: c.player_id,
                action,
            }
        })
        .collect()
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
    use crate::command::{Command, GameCommand, HotkeyAction};

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
