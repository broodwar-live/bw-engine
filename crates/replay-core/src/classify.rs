//! Build order classification — label openings by name.
//!
//! Matches a player's opening against known competitive BW opening patterns
//! per race. Works on the production model output
//! ([`crate::production::ProductionEntry`]), so orders that could not have
//! been carried out are ignored, workers are recognised for every race (Zerg
//! drones are larva morphs), and pool timing uses the modelled supply count.
//!
//! ## Known openings
//!
//! - **Terran**: CC First, Bunker Rush, 1-1-1, 2 Rax (Academy), Bio,
//!   1 Rax FE, Factory Expand, Mech, Rax Factory, Barracks First
//! - **Zerg**: 4/5 Pool, 9 Pool, Overpool, 12 Pool, 12 Hatch,
//!   3 Hatch Before Pool, each optionally followed by its tech ("→ Muta",
//!   "→ Hydra", "→ Lair")
//! - **Protoss**: Nexus First, Forge FE (+ Cannon), DT Rush, 2 Gate (Core),
//!   1 Gate Stargate/Robo/Citadel/FE/Core, Gateway First

use crate::analysis::BuildAction;
use crate::header::Race;
use crate::production::{ProductionEntry, is_worker_order};

/// Only orders in the first four real minutes define the opening.
pub const OPENING_SECONDS: f64 = 240.0;
/// At most this many non-worker orders are considered.
pub const OPENING_ACTIONS: usize = 20;

/// Result of classifying a player's opening build order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpeningClassification {
    /// Player the classification is for.
    pub player_id: u8,
    /// Human-readable opening name (e.g., "9 Pool", "12 Pool → Lair").
    pub name: String,
    /// Short tag for database indexing (e.g., "9pool", "1gate_core").
    pub tag: String,
    /// Tech the opening heads into, if part of the name (Zerg: "muta",
    /// "hydra", "lair").
    pub follow_up: Option<String>,
    /// Confidence score (0.0–1.0). Higher = more certain match.
    pub confidence: f64,
    /// Player's race.
    pub race: String,
    /// Number of build actions considered.
    pub actions_analyzed: usize,
}

/// Classify a player's opening from the production model output.
///
/// Considers the first [`OPENING_ACTIONS`] non-worker orders of the first
/// [`OPENING_SECONDS`] that the model judged as carried out.
#[must_use]
pub fn classify_opening(
    production: &[ProductionEntry],
    player_id: u8,
    race: &Race,
) -> OpeningClassification {
    let entries: Vec<&ProductionEntry> = production
        .iter()
        .filter(|e| {
            e.player_id == player_id
                && !e.dropped
                && !is_worker_order(&e.action)
                && e.real_seconds < OPENING_SECONDS
        })
        .take(OPENING_ACTIONS)
        .collect();

    let race_str = race.code();

    let (name, tag, follow_up, confidence) = match race {
        Race::Terran => no_follow_up(classify_terran(&entries)),
        Race::Zerg => classify_zerg(&entries),
        Race::Protoss => no_follow_up(classify_protoss(&entries)),
        Race::Unknown(_) => ("Unknown".to_string(), "unknown".to_string(), None, 0.0),
    };

    OpeningClassification {
        player_id,
        name,
        tag,
        follow_up,
        confidence,
        race: race_str.to_string(),
        actions_analyzed: entries.len(),
    }
}

/// Classify all players' openings.
#[must_use]
pub fn classify_all(
    production: &[ProductionEntry],
    players: &[(u8, Race)],
) -> Vec<OpeningClassification> {
    players
        .iter()
        .map(|(pid, race)| classify_opening(production, *pid, race))
        .collect()
}

type Label = (String, String, f64);

fn label(name: &str, tag: &str, confidence: f64) -> Label {
    (name.to_string(), tag.to_string(), confidence)
}

fn no_follow_up((name, tag, c): Label) -> (String, String, Option<String>, f64) {
    (name, tag, None, c)
}

/// Structures (and, for Zerg, building morphs) in order.
fn structures(entries: &[&ProductionEntry], with_morphs: bool) -> Vec<u16> {
    entries
        .iter()
        .filter_map(|e| match e.action {
            BuildAction::Build(id) => Some(id),
            BuildAction::BuildingMorph(id) if with_morphs => Some(id),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Building ID constants
// ---------------------------------------------------------------------------

const BARRACKS: u16 = 111;
const FACTORY: u16 = 113;
const STARPORT: u16 = 114;
const ACADEMY: u16 = 112;
const COMMAND_CENTER: u16 = 106;
const SUPPLY_DEPOT: u16 = 109;
const REFINERY: u16 = 110;
const BUNKER: u16 = 125;

const SPAWNING_POOL: u16 = 142;
const HATCHERY: u16 = 131;
const LAIR: u16 = 132;
const HYDRA_DEN: u16 = 135;
const SPIRE: u16 = 141;
const OVERLORD: u16 = 42;

const GATEWAY: u16 = 160;
const FORGE: u16 = 166;
const NEXUS: u16 = 154;
const CYBERNETICS: u16 = 164;
const ASSIMILATOR: u16 = 157;
const PYLON: u16 = 156;
const CITADEL: u16 = 163;
const TEMPLAR_ARCHIVES: u16 = 165;
const STARGATE: u16 = 167;
const ROBOTICS: u16 = 155;
const CANNON: u16 = 162;

// ---------------------------------------------------------------------------
// Terran classification
// ---------------------------------------------------------------------------

fn classify_terran(entries: &[&ProductionEntry]) -> Label {
    let buildings = structures(entries, false);
    if buildings.is_empty() {
        return label("Unknown", "unknown", 0.0);
    }

    let pos = |id: u16| buildings.iter().position(|&b| b == id);
    let rax_count = buildings.iter().filter(|&&b| b == BARRACKS).count();
    let has_factory = buildings.contains(&FACTORY);
    let has_starport = buildings.contains(&STARPORT);
    let has_academy = buildings.contains(&ACADEMY);
    let factory_idx = pos(FACTORY);
    let cc_idx = pos(COMMAND_CENTER);

    // First production-relevant structure (supply and gas excluded). The
    // main Command Center is not in the build order, so a Command Center here
    // is an expansion.
    let first = buildings
        .iter()
        .copied()
        .find(|&b| b != SUPPLY_DEPOT && b != REFINERY);

    // CC first (fast expand before any Barracks).
    if first == Some(COMMAND_CENTER) {
        return label("CC First", "cc_first", 0.8);
    }

    // Bunker rush: early Bunker, before any expansion or Factory.
    if let Some(bi) = pos(BUNKER)
        && bi < 4
        && cc_idx.is_none_or(|c| bi < c)
        && factory_idx.is_none_or(|f| bi < f)
    {
        return label("Bunker Rush", "bunker_rush", 0.7);
    }

    // 1-1-1: Barracks, Factory, Starport.
    if rax_count >= 1 && has_factory && has_starport && !has_academy {
        return label("1-1-1", "111", 0.85);
    }

    // 2 Rax: two Barracks before any Factory.
    if rax_count >= 2 && !has_factory {
        if has_academy {
            return label("2 Rax Academy", "2rax_academy", 0.8);
        }
        return label("2 Rax", "2rax", 0.8);
    }

    // 1 Rax FE: Command Center after one Barracks, before any Factory.
    if rax_count == 1
        && let Some(c) = cc_idx
        && factory_idx.is_none_or(|f| c < f)
    {
        return label("1 Rax FE", "1rax_fe", 0.75);
    }

    // Bio: Barracks + Academy, no Factory early.
    if rax_count >= 1 && has_academy && !has_factory {
        return label("Bio", "bio", 0.7);
    }

    // Factory builds with a single Barracks.
    if has_factory && rax_count == 1 {
        if let (Some(f), Some(c)) = (factory_idx, cc_idx)
            && c > f
        {
            return label("Factory Expand", "fe_factory", 0.75);
        }
        if has_starport {
            return label("Mech", "mech", 0.7);
        }
        return label("Rax Factory", "rax_factory", 0.6);
    }

    // Default: Barracks first.
    if first == Some(BARRACKS) {
        return label("Barracks First", "rax_first", 0.5);
    }

    label("Unknown Terran", "unknown_t", 0.3)
}

// ---------------------------------------------------------------------------
// Zerg classification
// ---------------------------------------------------------------------------

fn classify_zerg(entries: &[&ProductionEntry]) -> (String, String, Option<String>, f64) {
    let buildings = structures(entries, true);
    if buildings.is_empty() {
        return ("Unknown".into(), "unknown".into(), None, 0.0);
    }

    let pool_idx = buildings.iter().position(|&b| b == SPAWNING_POOL);
    let hatch_idx = buildings.iter().position(|&b| b == HATCHERY);

    let pool = entries
        .iter()
        .find(|e| matches!(e.action, BuildAction::Build(SPAWNING_POOL)));
    let overlord_before_pool = pool.is_some_and(|p| {
        entries
            .iter()
            .any(|e| e.frame <= p.frame && matches!(e.action, BuildAction::UnitMorph(OVERLORD)))
    });

    let (name, tag, confidence) = match (pool_idx, hatch_idx) {
        // Hatchery first (before pool, or no pool yet in the opening).
        (pi, Some(hi)) if pi.is_none_or(|pi| hi < pi) => {
            let before_pool = pi.map_or(&buildings[..], |pi| &buildings[..pi]);
            if before_pool.iter().filter(|&&b| b == HATCHERY).count() >= 2 {
                ("3 Hatch Before Pool", "3hatch_pool", 0.85)
            } else {
                ("12 Hatch", "12hatch", 0.8)
            }
        }
        (Some(_), _) => {
            let supply = pool.map_or(0, |p| p.supply);
            match supply {
                0..=5 => ("4/5 Pool", "4pool", 0.85),
                6..=10 if overlord_before_pool && supply >= 9 => ("Overpool", "overpool", 0.75),
                6..=10 => ("9 Pool", "9pool", 0.8),
                _ => ("12 Pool", "12pool", 0.75),
            }
        }
        _ => return ("Unknown Zerg".into(), "unknown_z".into(), None, 0.3),
    };

    let follow_up = if buildings.contains(&SPIRE) && !buildings.contains(&HYDRA_DEN) {
        Some(("Muta", "muta"))
    } else if buildings.contains(&HYDRA_DEN) {
        Some(("Hydra", "hydra"))
    } else if buildings.contains(&LAIR) {
        Some(("Lair", "lair"))
    } else {
        None
    };

    match follow_up {
        Some((label, key)) => (
            format!("{name} → {label}"),
            tag.to_string(),
            Some(key.to_string()),
            confidence,
        ),
        None => (name.to_string(), tag.to_string(), None, confidence),
    }
}

// ---------------------------------------------------------------------------
// Protoss classification
// ---------------------------------------------------------------------------

fn classify_protoss(entries: &[&ProductionEntry]) -> Label {
    let buildings = structures(entries, false);
    if buildings.is_empty() {
        return label("Unknown", "unknown", 0.0);
    }

    let gate_count = buildings.iter().filter(|&&b| b == GATEWAY).count();
    let has_core = buildings.contains(&CYBERNETICS);
    let has_forge = buildings.contains(&FORGE);
    // The main Nexus is not in the build order: any Nexus is an expansion.
    let has_nexus = buildings.contains(&NEXUS);
    let has_citadel = buildings.contains(&CITADEL);
    let has_archives = buildings.contains(&TEMPLAR_ARCHIVES);
    let has_stargate = buildings.contains(&STARGATE);
    let has_robo = buildings.contains(&ROBOTICS);
    let has_cannon = buildings.contains(&CANNON);

    let first = buildings
        .iter()
        .copied()
        .find(|&b| b != PYLON && b != ASSIMILATOR);

    // Nexus first.
    if first == Some(NEXUS) {
        return label("Nexus First", "nexus_first", 0.8);
    }

    // Forge FE: Forge before Gateway.
    let forge_idx = buildings.iter().position(|&b| b == FORGE);
    let gate_idx = buildings.iter().position(|&b| b == GATEWAY);
    if let (Some(fi), Some(gi)) = (forge_idx, gate_idx)
        && fi < gi
    {
        if has_cannon {
            return label("Forge FE + Cannon", "forge_fe_cannon", 0.85);
        }
        return label("Forge FE", "forge_fe", 0.85);
    }
    // Forge without gateway yet.
    if has_forge && gate_count == 0 {
        return label("Forge FE", "forge_fe", 0.75);
    }

    // DT rush: Gateway → Core → Citadel → Archives.
    if has_archives && gate_count >= 1 {
        return label("DT Rush", "dt_rush", 0.8);
    }

    // 2 Gate.
    if gate_count >= 2 {
        if has_core {
            return label("2 Gate Core", "2gate_core", 0.8);
        }
        return label("2 Gate", "2gate", 0.8);
    }

    // 1 Gate variants.
    if gate_count == 1 && has_core {
        if has_stargate {
            return label("1 Gate Stargate", "1gate_stargate", 0.8);
        }
        if has_robo {
            return label("1 Gate Robo", "1gate_robo", 0.8);
        }
        if has_citadel {
            return label("1 Gate Citadel", "1gate_citadel", 0.75);
        }
        if has_nexus {
            return label("1 Gate FE", "1gate_fe", 0.8);
        }
        return label("1 Gate Core", "1gate_core", 0.7);
    }

    if gate_count == 1 {
        return label("Gateway First", "gate_first", 0.5);
    }

    label("Unknown Protoss", "unknown_p", 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE_SHOP: u16 = 120;

    fn entry(secs: u32, action: BuildAction, supply: u32) -> ProductionEntry {
        let frame = (f64::from(secs) * 23.81) as u32;
        ProductionEntry {
            frame,
            real_seconds: f64::from(frame) / 23.81,
            player_id: 0,
            action,
            supply,
            supply_max: 0,
            count: 1,
            start_frame: Some(frame),
            dropped: false,
            drop_reason: None,
        }
    }

    fn b(secs: u32, id: u16) -> ProductionEntry {
        entry(secs, BuildAction::Build(id), 10)
    }

    fn tag(entries: &[ProductionEntry], race: Race) -> String {
        classify_opening(entries, 0, &race).tag
    }

    // -- Terran: one test per branch --

    #[test]
    fn test_classify_terran_every_branch() {
        let cases: &[(&[u16], &str)] = &[
            (&[SUPPLY_DEPOT, COMMAND_CENTER, BARRACKS], "cc_first"),
            (&[SUPPLY_DEPOT, BARRACKS, BUNKER], "bunker_rush"),
            (
                &[SUPPLY_DEPOT, BARRACKS, REFINERY, FACTORY, STARPORT],
                "111",
            ),
            (&[SUPPLY_DEPOT, BARRACKS, BARRACKS], "2rax"),
            (&[SUPPLY_DEPOT, BARRACKS, BARRACKS, ACADEMY], "2rax_academy"),
            (&[SUPPLY_DEPOT, BARRACKS, COMMAND_CENTER, BUNKER], "1rax_fe"),
            (&[SUPPLY_DEPOT, BARRACKS, REFINERY, ACADEMY], "bio"),
            (
                &[SUPPLY_DEPOT, BARRACKS, REFINERY, FACTORY, COMMAND_CENTER],
                "fe_factory",
            ),
            (
                &[SUPPLY_DEPOT, BARRACKS, REFINERY, FACTORY, ACADEMY, STARPORT],
                "mech",
            ),
            (
                &[SUPPLY_DEPOT, BARRACKS, REFINERY, FACTORY, MACHINE_SHOP],
                "rax_factory",
            ),
            (&[SUPPLY_DEPOT, BARRACKS, REFINERY], "rax_first"),
            (&[SUPPLY_DEPOT, 122], "unknown_t"), // Engineering Bay first
            (&[], "unknown"),
        ];
        for (buildings, want) in cases {
            let bo: Vec<ProductionEntry> = buildings
                .iter()
                .enumerate()
                .map(|(i, &id)| b(30 + i as u32 * 20, id))
                .collect();
            assert_eq!(&tag(&bo, Race::Terran), want, "{buildings:?}");
        }
    }

    // -- Zerg --

    fn zerg(seq: &[(BuildAction, u32)]) -> OpeningClassification {
        let bo: Vec<ProductionEntry> = seq
            .iter()
            .enumerate()
            .map(|(i, (a, supply))| entry(20 + i as u32 * 10, a.clone(), *supply))
            .collect();
        classify_opening(&bo, 0, &Race::Zerg)
    }

    #[test]
    fn test_classify_zerg_pool_timings_use_modelled_supply() {
        use BuildAction::{Build, UnitMorph};
        let drones = |n: u32| -> Vec<(BuildAction, u32)> {
            (0..n).map(|i| (UnitMorph(41), 4 + i)).collect()
        };
        // Drones are larva morphs; they used to be counted as Train(41) only,
        // which made every pool a 4/5 pool.
        let mut s = drones(5);
        s.push((Build(SPAWNING_POOL), 9));
        assert_eq!(zerg(&s).tag, "9pool");

        assert_eq!(zerg(&[(Build(SPAWNING_POOL), 4)]).tag, "4pool");

        let mut s = drones(5);
        s.push((UnitMorph(OVERLORD), 9));
        s.push((Build(SPAWNING_POOL), 9));
        let c = zerg(&s);
        assert_eq!((c.tag.as_str(), c.name.as_str()), ("overpool", "Overpool"));

        let mut s = drones(8);
        s.push((UnitMorph(OVERLORD), 9));
        s.push((Build(SPAWNING_POOL), 12));
        assert_eq!(zerg(&s).tag, "12pool");
    }

    #[test]
    fn test_classify_zerg_hatch_first_and_follow_ups() {
        use BuildAction::{Build, BuildingMorph};
        let c = zerg(&[(Build(HATCHERY), 12), (Build(SPAWNING_POOL), 11)]);
        assert_eq!(c.tag, "12hatch");
        let c = zerg(&[(Build(HATCHERY), 12)]);
        assert_eq!(c.tag, "12hatch", "hatch with pool after the opening window");
        let c = zerg(&[
            (Build(HATCHERY), 12),
            (Build(HATCHERY), 13),
            (Build(SPAWNING_POOL), 12),
        ]);
        assert_eq!(c.tag, "3hatch_pool");

        let c = zerg(&[
            (Build(SPAWNING_POOL), 12),
            (BuildingMorph(LAIR), 14),
            (Build(SPIRE), 16),
        ]);
        assert_eq!(c.name, "12 Pool → Muta");
        assert_eq!(c.tag, "12pool");
        assert_eq!(c.follow_up.as_deref(), Some("muta"));
        let c = zerg(&[(Build(SPAWNING_POOL), 9), (Build(HYDRA_DEN), 14)]);
        assert_eq!(c.name, "9 Pool → Hydra");
        let c = zerg(&[(Build(HATCHERY), 12), (BuildingMorph(LAIR), 14)]);
        assert_eq!(c.name, "12 Hatch → Lair");

        assert_eq!(zerg(&[(Build(149), 9)]).tag, "unknown_z");
        assert_eq!(zerg(&[]).tag, "unknown");
    }

    #[test]
    fn test_classify_ignores_dropped_and_late_orders() {
        let mut pool = b(30, SPAWNING_POOL);
        pool.supply = 4;
        pool.dropped = true;
        let mut real_pool = b(60, SPAWNING_POOL);
        real_pool.supply = 9;
        let late_hatch = b(300, HATCHERY);
        let c = classify_opening(&[pool, real_pool, late_hatch], 0, &Race::Zerg);
        assert_eq!(c.tag, "9pool");
        assert_eq!(c.actions_analyzed, 1);
    }

    // -- Protoss: one test per branch --

    #[test]
    fn test_classify_protoss_every_branch() {
        let cases: &[(&[u16], &str)] = &[
            (&[PYLON, NEXUS, FORGE, GATEWAY], "nexus_first"),
            (&[PYLON, FORGE, CANNON, NEXUS, GATEWAY], "forge_fe_cannon"),
            (&[PYLON, FORGE, NEXUS, GATEWAY], "forge_fe"),
            (&[PYLON, FORGE, CANNON], "forge_fe"),
            (
                &[PYLON, GATEWAY, CYBERNETICS, CITADEL, TEMPLAR_ARCHIVES],
                "dt_rush",
            ),
            (&[PYLON, GATEWAY, GATEWAY], "2gate"),
            (&[PYLON, GATEWAY, GATEWAY, CYBERNETICS], "2gate_core"),
            (&[PYLON, GATEWAY, CYBERNETICS, STARGATE], "1gate_stargate"),
            (&[PYLON, GATEWAY, CYBERNETICS, ROBOTICS], "1gate_robo"),
            (&[PYLON, GATEWAY, CYBERNETICS, CITADEL], "1gate_citadel"),
            (&[PYLON, GATEWAY, CYBERNETICS, NEXUS], "1gate_fe"),
            (&[PYLON, GATEWAY, ASSIMILATOR, CYBERNETICS], "1gate_core"),
            (&[PYLON, GATEWAY], "gate_first"),
            (&[PYLON, ASSIMILATOR], "unknown_p"),
            (&[], "unknown"),
        ];
        for (buildings, want) in cases {
            let bo: Vec<ProductionEntry> = buildings
                .iter()
                .enumerate()
                .map(|(i, &id)| b(30 + i as u32 * 20, id))
                .collect();
            assert_eq!(&tag(&bo, Race::Protoss), want, "{buildings:?}");
        }
    }

    // -- General --

    #[test]
    fn test_classify_all() {
        let mut z = b(40, SPAWNING_POOL);
        z.player_id = 1;
        let bo = vec![b(30, BARRACKS), z];
        let results = classify_all(&bo, &[(0, Race::Terran), (1, Race::Zerg)]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].race, "T");
        assert_eq!(results[1].race, "Z");
        assert_eq!(results[1].player_id, 1);
    }

    #[test]
    fn test_classify_workers_ignored_for_every_race() {
        let mut bo: Vec<ProductionEntry> = [BuildAction::Train(7), BuildAction::Train(64)]
            .into_iter()
            .chain(std::iter::repeat_n(BuildAction::UnitMorph(41), 30))
            .map(|a| entry(5, a, 5))
            .collect();
        bo.push(b(60, BARRACKS));
        let c = classify_opening(&bo, 0, &Race::Terran);
        assert_eq!(c.tag, "rax_first");
        assert_eq!(c.actions_analyzed, 1);
    }

    #[test]
    fn test_classify_unknown_race() {
        let c = classify_opening(&[b(30, BARRACKS)], 0, &Race::Unknown(9));
        assert_eq!(c.tag, "unknown");
    }
}
