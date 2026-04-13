//! Player skill estimation from replay metrics.
//!
//! Estimates relative player skill from observable replay data. This produces
//! a skill *profile* — not an absolute Elo rating (that requires cross-replay
//! win/loss data).
//!
//! ## Metrics used
//!
//! - **APM**: Raw actions per minute — higher generally indicates more practice.
//! - **EAPM**: Effective APM (excludes spam) — better signal than raw APM.
//! - **Efficiency ratio**: EAPM / APM — higher means less spam, more purposeful input.
//! - **Hotkey usage**: Frequency of control group assign/recall — proxy for multitasking.
//! - **Action consistency**: Standard deviation of per-window APM — lower means steadier play.
//! - **Opening speed**: How quickly the first production actions happen — proxy for build order knowledge.

use crate::analysis::{ApmSample, PlayerApm};
use crate::command::{Command, GameCommand, HotkeyAction};

/// Frames per second at Fastest speed.
const FPS: f64 = 23.81;

/// A player's skill profile derived from replay data.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillProfile {
    pub player_id: u8,
    /// Raw APM.
    pub apm: f64,
    /// Effective APM (excludes spam).
    pub eapm: f64,
    /// EAPM / APM ratio (0.0–1.0). Higher = less spam.
    pub efficiency: f64,
    /// Hotkey assigns per minute.
    pub hotkey_assigns_per_min: f64,
    /// Hotkey recalls per minute.
    pub hotkey_recalls_per_min: f64,
    /// Standard deviation of APM across time windows.
    /// Lower = more consistent play.
    pub apm_consistency: f64,
    /// Frame of the first production action (lower = faster opener).
    pub first_action_frame: Option<u32>,
    /// Composite skill score (0–100). Relative, not calibrated to any rating system.
    pub skill_score: f64,
    /// Skill tier derived from score.
    pub tier: SkillTier,
}

/// Rough skill tier buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum SkillTier {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
    Professional,
}

impl SkillTier {
    pub fn name(self) -> &'static str {
        match self {
            SkillTier::Beginner => "Beginner",
            SkillTier::Intermediate => "Intermediate",
            SkillTier::Advanced => "Advanced",
            SkillTier::Expert => "Expert",
            SkillTier::Professional => "Professional",
        }
    }
}

/// Estimate skill profiles for all players in a replay.
pub fn estimate_skill(
    commands: &[GameCommand],
    player_apms: &[PlayerApm],
    apm_samples: &[ApmSample],
    total_frames: u32,
) -> Vec<SkillProfile> {
    let duration_minutes = total_frames as f64 / FPS / 60.0;
    if duration_minutes < 0.5 {
        // Game too short for meaningful analysis.
        return player_apms
            .iter()
            .map(|pa| SkillProfile {
                player_id: pa.player_id,
                apm: pa.apm,
                eapm: pa.eapm,
                efficiency: if pa.apm > 0.0 { pa.eapm / pa.apm } else { 0.0 },
                hotkey_assigns_per_min: 0.0,
                hotkey_recalls_per_min: 0.0,
                apm_consistency: 0.0,
                first_action_frame: None,
                skill_score: 0.0,
                tier: SkillTier::Beginner,
            })
            .collect();
    }

    player_apms
        .iter()
        .map(|pa| {
            let pid = pa.player_id;

            // Hotkey usage.
            let (assigns, recalls) = count_hotkeys(commands, pid);
            let hotkey_assigns_per_min = assigns as f64 / duration_minutes;
            let hotkey_recalls_per_min = recalls as f64 / duration_minutes;

            // APM consistency (std dev of per-window APM).
            let player_samples: Vec<f64> = apm_samples
                .iter()
                .filter(|s| s.player_id == pid)
                .map(|s| s.apm)
                .collect();
            let apm_consistency = std_dev(&player_samples);

            // First meaningful action.
            let first_action_frame = commands
                .iter()
                .filter(|c| c.player_id == pid && c.command.is_build_order_action())
                .map(|c| c.frame)
                .next();

            let efficiency = if pa.apm > 0.0 { pa.eapm / pa.apm } else { 0.0 };

            // Composite skill score.
            let score = compute_skill_score(
                pa.eapm,
                efficiency,
                hotkey_assigns_per_min + hotkey_recalls_per_min,
                apm_consistency,
                first_action_frame,
            );

            let tier = score_to_tier(score);

            SkillProfile {
                player_id: pid,
                apm: pa.apm,
                eapm: pa.eapm,
                efficiency,
                hotkey_assigns_per_min,
                hotkey_recalls_per_min,
                apm_consistency,
                first_action_frame,
                skill_score: score,
                tier,
            }
        })
        .collect()
}

/// Count hotkey assign and recall commands for a player.
fn count_hotkeys(commands: &[GameCommand], player_id: u8) -> (u32, u32) {
    let mut assigns = 0u32;
    let mut recalls = 0u32;
    for c in commands {
        if c.player_id != player_id {
            continue;
        }
        if let Command::Hotkey { action, .. } = &c.command {
            match action {
                HotkeyAction::Assign => assigns += 1,
                HotkeyAction::Select => recalls += 1,
            }
        }
    }
    (assigns, recalls)
}

/// Compute a composite skill score (0–100) from individual metrics.
///
/// Weights are calibrated against known BW skill distributions:
/// - Casual players: ~50-100 EAPM, low hotkey use
/// - Intermediate: ~100-200 EAPM, moderate hotkey use
/// - Advanced: ~200-300 EAPM, high hotkey use, consistent APM
/// - Pro: ~300+ EAPM, very high hotkey use, very consistent
fn compute_skill_score(
    eapm: f64,
    efficiency: f64,
    hotkey_per_min: f64,
    apm_consistency: f64,
    first_action_frame: Option<u32>,
) -> f64 {
    // EAPM component (0–40 points): logarithmic scaling.
    // 50 EAPM ≈ 10pts, 150 ≈ 25pts, 300 ≈ 35pts, 400+ ≈ 40pts.
    let eapm_score = (eapm.max(1.0).ln() / (400.0_f64).ln() * 40.0).min(40.0);

    // Efficiency component (0–15 points): linear from 0.5 to 0.95.
    let eff_score = ((efficiency - 0.5).max(0.0) / 0.45 * 15.0).min(15.0);

    // Hotkey component (0–20 points): logarithmic.
    // 10/min ≈ 5pts, 50/min ≈ 12pts, 100+/min ≈ 20pts.
    let hk_score = if hotkey_per_min > 0.0 {
        (hotkey_per_min.max(1.0).ln() / (100.0_f64).ln() * 20.0).min(20.0)
    } else {
        0.0
    };

    // Consistency component (0–15 points): lower std dev = better.
    // StdDev 200+ = 0pts, 50 = 10pts, <20 = 15pts.
    let consistency_score = if apm_consistency < 20.0 {
        15.0
    } else {
        (15.0 - (apm_consistency - 20.0) / 180.0 * 15.0).max(0.0)
    };

    // Opening speed (0–10 points): how fast the first build action happens.
    // <200 frames (8.4s) = 10pts, >1000 frames (42s) = 0pts.
    let opening_score = match first_action_frame {
        Some(f) if f < 200 => 10.0,
        Some(f) if f < 1000 => 10.0 - (f - 200) as f64 / 800.0 * 10.0,
        _ => 0.0,
    };

    (eapm_score + eff_score + hk_score + consistency_score + opening_score).min(100.0)
}

fn score_to_tier(score: f64) -> SkillTier {
    if score >= 75.0 {
        SkillTier::Professional
    } else if score >= 55.0 {
        SkillTier::Expert
    } else if score >= 35.0 {
        SkillTier::Advanced
    } else if score >= 18.0 {
        SkillTier::Intermediate
    } else {
        SkillTier::Beginner
    }
}

fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::PlayerApm;

    fn make_cmd(frame: u32, pid: u8, cmd: Command) -> GameCommand {
        GameCommand {
            frame,
            player_id: pid,
            command: cmd,
        }
    }

    #[test]
    fn test_estimate_skill_basic() {
        let commands = vec![
            make_cmd(100, 0, Command::Train { unit_type: 0 }),
            make_cmd(
                200,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Assign,
                    group: 1,
                },
            ),
            make_cmd(
                300,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Select,
                    group: 1,
                },
            ),
        ];

        let apms = vec![PlayerApm {
            player_id: 0,
            apm: 200.0,
            eapm: 150.0,
        }];

        let profiles = estimate_skill(&commands, &apms, &[], 10000);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].player_id, 0);
        assert!((profiles[0].efficiency - 0.75).abs() < 0.01);
        assert!(profiles[0].hotkey_assigns_per_min > 0.0);
        assert!(profiles[0].hotkey_recalls_per_min > 0.0);
        assert!(profiles[0].skill_score > 0.0);
    }

    #[test]
    fn test_skill_tiers() {
        assert_eq!(score_to_tier(10.0), SkillTier::Beginner);
        assert_eq!(score_to_tier(25.0), SkillTier::Intermediate);
        assert_eq!(score_to_tier(45.0), SkillTier::Advanced);
        assert_eq!(score_to_tier(65.0), SkillTier::Expert);
        assert_eq!(score_to_tier(80.0), SkillTier::Professional);
    }

    #[test]
    fn test_higher_eapm_higher_score() {
        let s1 = compute_skill_score(50.0, 0.8, 20.0, 50.0, Some(300));
        let s2 = compute_skill_score(200.0, 0.8, 20.0, 50.0, Some(300));
        let s3 = compute_skill_score(350.0, 0.8, 20.0, 50.0, Some(300));
        assert!(s2 > s1);
        assert!(s3 > s2);
    }

    #[test]
    fn test_hotkey_usage_increases_score() {
        let s1 = compute_skill_score(150.0, 0.8, 0.0, 50.0, Some(300));
        let s2 = compute_skill_score(150.0, 0.8, 80.0, 50.0, Some(300));
        assert!(s2 > s1);
    }

    #[test]
    fn test_consistency_increases_score() {
        let s1 = compute_skill_score(150.0, 0.8, 30.0, 150.0, Some(300));
        let s2 = compute_skill_score(150.0, 0.8, 30.0, 30.0, Some(300));
        assert!(s2 > s1);
    }

    #[test]
    fn test_short_game_returns_profiles() {
        let apms = vec![PlayerApm {
            player_id: 0,
            apm: 100.0,
            eapm: 80.0,
        }];
        // 500 frames ≈ 21s ≈ 0.35 min — too short.
        let profiles = estimate_skill(&[], &apms, &[], 500);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].tier, SkillTier::Beginner);
    }

    #[test]
    fn test_count_hotkeys() {
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
            make_cmd(
                300,
                0,
                Command::Hotkey {
                    action: HotkeyAction::Select,
                    group: 2,
                },
            ),
            make_cmd(
                400,
                1,
                Command::Hotkey {
                    action: HotkeyAction::Assign,
                    group: 1,
                },
            ),
        ];

        let (a, r) = count_hotkeys(&commands, 0);
        assert_eq!(a, 1);
        assert_eq!(r, 2);

        let (a, r) = count_hotkeys(&commands, 1);
        assert_eq!(a, 1);
        assert_eq!(r, 0);
    }

    #[test]
    fn test_std_dev() {
        assert!((std_dev(&[10.0, 10.0, 10.0]) - 0.0).abs() < 0.001);
        assert!(std_dev(&[0.0, 100.0]) > 40.0);
        assert!((std_dev(&[]) - 0.0).abs() < 0.001);
    }
}
