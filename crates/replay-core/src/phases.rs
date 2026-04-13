//! Game phase detection from build order events.
//!
//! Detects early/mid/late game phases by identifying tech landmarks in the
//! build order. Phases are defined by concrete in-game events rather than
//! arbitrary time cutoffs, making them accurate across different game speeds
//! and play styles.
//!
//! ## Phase definitions
//!
//! - **Opening**: Game start until the first tech-enabling structure completes.
//! - **Early game**: First tech structure through first tier-2 tech or expansion.
//! - **Mid game**: Tier-2 tech/expansion through tier-3 tech.
//! - **Late game**: Tier-3 tech onwards.

use crate::analysis::{BuildAction, BuildOrderEntry};

/// Frames per second at Fastest speed.
const FPS: f64 = 23.81;

/// A detected game phase with start/end boundaries.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GamePhase {
    pub phase: Phase,
    pub start_frame: u32,
    pub start_seconds: f64,
    pub end_frame: Option<u32>,
    pub end_seconds: Option<f64>,
}

/// Phase of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Phase {
    Opening,
    EarlyGame,
    MidGame,
    LateGame,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Opening => "Opening",
            Phase::EarlyGame => "Early Game",
            Phase::MidGame => "Mid Game",
            Phase::LateGame => "Late Game",
        }
    }
}

/// Tech landmarks used for phase detection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TechLandmarks {
    /// Frame of first gas structure (Refinery, Extractor, Assimilator).
    pub first_gas: Option<u32>,
    /// Frame of first tech-enabling structure (Factory, Lair, Cyber Core, etc.).
    pub first_tech: Option<u32>,
    /// Frame of first tier-2 tech (Starport, Hive, Templar Archives, etc.).
    pub first_tier2: Option<u32>,
    /// Frame of first tier-3 tech (Science Facility, Greater Spire, Fleet Beacon, etc.).
    pub first_tier3: Option<u32>,
    /// Frame of first expansion (2nd Command Center, Hatchery, or Nexus).
    pub first_expansion: Option<u32>,
}

/// Full phase analysis result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhaseAnalysis {
    /// Detected phases in chronological order.
    pub phases: Vec<GamePhase>,
    /// Tech landmarks that triggered phase transitions.
    pub landmarks: TechLandmarks,
}

/// Detect game phases from the build order.
///
/// Analyzes both players' build orders together to find the earliest
/// tech landmarks and derive phase boundaries.
pub fn detect_phases(build_order: &[BuildOrderEntry], total_frames: u32) -> PhaseAnalysis {
    let landmarks = find_landmarks(build_order);

    let mut phases = Vec::new();
    let mut current_start = 0u32;

    // Opening → ends at first tech structure.
    let opening_end = landmarks
        .first_tech
        .or(landmarks.first_gas)
        .unwrap_or(total_frames);
    phases.push(GamePhase {
        phase: Phase::Opening,
        start_frame: current_start,
        start_seconds: frame_to_secs(current_start),
        end_frame: Some(opening_end),
        end_seconds: Some(frame_to_secs(opening_end)),
    });
    current_start = opening_end;

    if current_start < total_frames {
        // Early game → ends at first tier-2 or expansion.
        let early_end = [landmarks.first_tier2, landmarks.first_expansion]
            .iter()
            .filter_map(|&f| f)
            .min()
            .unwrap_or(total_frames);
        phases.push(GamePhase {
            phase: Phase::EarlyGame,
            start_frame: current_start,
            start_seconds: frame_to_secs(current_start),
            end_frame: Some(early_end),
            end_seconds: Some(frame_to_secs(early_end)),
        });
        current_start = early_end;
    }

    if current_start < total_frames {
        // Mid game → ends at first tier-3.
        let mid_end = landmarks.first_tier3.unwrap_or(total_frames);
        phases.push(GamePhase {
            phase: Phase::MidGame,
            start_frame: current_start,
            start_seconds: frame_to_secs(current_start),
            end_frame: Some(mid_end),
            end_seconds: Some(frame_to_secs(mid_end)),
        });
        current_start = mid_end;
    }

    if current_start < total_frames {
        // Late game → until end.
        phases.push(GamePhase {
            phase: Phase::LateGame,
            start_frame: current_start,
            start_seconds: frame_to_secs(current_start),
            end_frame: None,
            end_seconds: None,
        });
    }

    PhaseAnalysis { phases, landmarks }
}

/// Determine which phase a given frame falls in.
pub fn phase_at_frame(analysis: &PhaseAnalysis, frame: u32) -> Phase {
    for phase in analysis.phases.iter().rev() {
        if frame >= phase.start_frame {
            return phase.phase;
        }
    }
    Phase::Opening
}

fn find_landmarks(build_order: &[BuildOrderEntry]) -> TechLandmarks {
    let mut first_gas = None;
    let mut first_tech = None;
    let mut first_tier2 = None;
    let mut first_tier3 = None;
    let mut first_expansion = None;

    // Track base counts per player to detect expansions.
    let mut base_count: [u8; 8] = [0; 8];

    for entry in build_order {
        let frame = entry.frame;
        let pid = entry.player_id as usize;

        match &entry.action {
            BuildAction::Build(id) | BuildAction::BuildingMorph(id) => {
                let id = *id;

                // Gas structures.
                if is_gas_building(id) && first_gas.is_none() {
                    first_gas = Some(frame);
                }

                // Bases (expansion detection).
                if is_base(id) && pid < 8 {
                    base_count[pid] += 1;
                    if base_count[pid] >= 2 && first_expansion.is_none() {
                        first_expansion = Some(frame);
                    }
                }

                // Tech tiers.
                if is_tier1_tech(id) && first_tech.is_none() {
                    first_tech = Some(frame);
                }
                if is_tier2_tech(id) && first_tier2.is_none() {
                    first_tier2 = Some(frame);
                }
                if is_tier3_tech(id) && first_tier3.is_none() {
                    first_tier3 = Some(frame);
                }
            }
            _ => {}
        }
    }

    TechLandmarks {
        first_gas,
        first_tech,
        first_tier2,
        first_tier3,
        first_expansion,
    }
}

fn frame_to_secs(frame: u32) -> f64 {
    frame as f64 / FPS
}

// ---------------------------------------------------------------------------
// Building classification
// ---------------------------------------------------------------------------

fn is_gas_building(id: u16) -> bool {
    matches!(id, 110 | 149 | 157) // Refinery, Extractor, Assimilator
}

fn is_base(id: u16) -> bool {
    matches!(id, 106 | 131 | 132 | 133 | 154) // CC, Hatch, Lair, Hive, Nexus
}

/// Tier 1 tech: first production/tech structure beyond the base + supply.
fn is_tier1_tech(id: u16) -> bool {
    matches!(
        id,
        // Terran
        111 | 112 | 122 | 125 // Barracks, Academy, Eng Bay, Bunker
        // Zerg
        | 142 | 135 | 139 | 143 // Spawning Pool, Hydra Den, Evo Chamber, Creep Colony
        // Protoss
        | 160 | 166 | 164 // Gateway, Forge, Cyber Core
    )
}

/// Tier 2 tech: second-tier structures that unlock advanced units.
fn is_tier2_tech(id: u16) -> bool {
    matches!(
        id,
        // Terran
        113 | 114 | 120 | 123 // Factory, Starport, Machine Shop, Armory
        // Zerg
        | 132 | 141 | 138 | 140 // Lair, Spire, Queen's Nest, Ultra Cavern
        // Protoss
        | 155 | 167 | 163 | 165 | 170 // Robo, Stargate, Citadel, Archives, Tribunal
    )
}

/// Tier 3 tech: highest-tier structures.
fn is_tier3_tech(id: u16) -> bool {
    matches!(
        id,
        // Terran
        116 | 117 | 118 | 108 // Science Facility, Covert Ops, Physics Lab, Nuke Silo
        // Zerg
        | 133 | 137 | 136 // Hive, Greater Spire, Defiler Mound
        // Protoss
        | 169 | 159 | 171 // Fleet Beacon, Observatory, Robo Support Bay
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bo_entry(frame: u32, player: u8, action: BuildAction) -> BuildOrderEntry {
        BuildOrderEntry {
            frame,
            real_seconds: frame as f64 / FPS,
            player_id: player,
            action,
        }
    }

    #[test]
    fn test_detect_phases_full_game() {
        let bo = vec![
            // Opening: workers + supply
            bo_entry(100, 0, BuildAction::Train(7)),   // SCV
            bo_entry(200, 0, BuildAction::Build(109)), // Supply Depot
            // Early game trigger: Barracks (tier 1 tech)
            bo_entry(800, 0, BuildAction::Build(111)), // Barracks
            bo_entry(1000, 0, BuildAction::Train(0)),  // Marine
            // Mid game trigger: Factory (tier 2 tech)
            bo_entry(3000, 0, BuildAction::Build(113)), // Factory
            // Late game trigger: Science Facility (tier 3 tech)
            bo_entry(8000, 0, BuildAction::Build(116)), // Science Facility
        ];

        let analysis = detect_phases(&bo, 15000);
        assert_eq!(analysis.phases.len(), 4);
        assert_eq!(analysis.phases[0].phase, Phase::Opening);
        assert_eq!(analysis.phases[1].phase, Phase::EarlyGame);
        assert_eq!(analysis.phases[1].start_frame, 800);
        assert_eq!(analysis.phases[2].phase, Phase::MidGame);
        assert_eq!(analysis.phases[2].start_frame, 3000);
        assert_eq!(analysis.phases[3].phase, Phase::LateGame);
        assert_eq!(analysis.phases[3].start_frame, 8000);
    }

    #[test]
    fn test_detect_phases_short_game() {
        let bo = vec![
            bo_entry(100, 0, BuildAction::Train(41)),  // Drone
            bo_entry(500, 0, BuildAction::Build(142)), // Spawning Pool (tier 1)
            bo_entry(800, 0, BuildAction::Train(37)),  // Zergling
        ];

        let analysis = detect_phases(&bo, 2000);
        assert_eq!(analysis.phases.len(), 2); // Opening + Early
        assert_eq!(analysis.phases[0].phase, Phase::Opening);
        assert_eq!(analysis.phases[1].phase, Phase::EarlyGame);
    }

    #[test]
    fn test_expansion_triggers_mid() {
        let bo = vec![
            bo_entry(100, 0, BuildAction::Build(111)), // Barracks (tier 1)
            bo_entry(500, 0, BuildAction::Build(106)), // First CC (base #1)
            bo_entry(2000, 0, BuildAction::Build(106)), // Second CC (expansion)
        ];

        // Expansion should trigger mid game at frame 2000 (if no tier-2 earlier).
        let analysis = detect_phases(&bo, 5000);
        let mid = analysis.phases.iter().find(|p| p.phase == Phase::MidGame);
        assert!(mid.is_some());
        assert_eq!(mid.unwrap().start_frame, 2000);
    }

    #[test]
    fn test_landmarks() {
        let bo = vec![
            bo_entry(300, 0, BuildAction::Build(110)),  // Refinery
            bo_entry(600, 0, BuildAction::Build(111)),  // Barracks
            bo_entry(2000, 0, BuildAction::Build(113)), // Factory
            bo_entry(5000, 0, BuildAction::Build(116)), // Science Facility
        ];

        let analysis = detect_phases(&bo, 10000);
        assert_eq!(analysis.landmarks.first_gas, Some(300));
        assert_eq!(analysis.landmarks.first_tech, Some(600));
        assert_eq!(analysis.landmarks.first_tier2, Some(2000));
        assert_eq!(analysis.landmarks.first_tier3, Some(5000));
    }

    #[test]
    fn test_phase_at_frame() {
        let bo = vec![
            bo_entry(500, 0, BuildAction::Build(160)), // Gateway (tier 1)
            bo_entry(2000, 0, BuildAction::Build(167)), // Stargate (tier 2)
            bo_entry(6000, 0, BuildAction::Build(169)), // Fleet Beacon (tier 3)
        ];

        let analysis = detect_phases(&bo, 10000);
        assert_eq!(phase_at_frame(&analysis, 0), Phase::Opening);
        assert_eq!(phase_at_frame(&analysis, 400), Phase::Opening);
        assert_eq!(phase_at_frame(&analysis, 500), Phase::EarlyGame);
        assert_eq!(phase_at_frame(&analysis, 1500), Phase::EarlyGame);
        assert_eq!(phase_at_frame(&analysis, 2000), Phase::MidGame);
        assert_eq!(phase_at_frame(&analysis, 6000), Phase::LateGame);
        assert_eq!(phase_at_frame(&analysis, 9000), Phase::LateGame);
    }

    #[test]
    fn test_empty_build_order() {
        let analysis = detect_phases(&[], 5000);
        // Should have at least opening phase.
        assert!(!analysis.phases.is_empty());
        assert_eq!(analysis.phases[0].phase, Phase::Opening);
    }
}
