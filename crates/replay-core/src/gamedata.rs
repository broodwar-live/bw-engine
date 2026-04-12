/// Look up the name of a unit type by its ID.
pub fn unit_name(id: u16) -> &'static str {
    match id {
        // Terran Units
        0 => "Marine",
        1 => "Ghost",
        2 => "Vulture",
        3 => "Goliath",
        5 => "Siege Tank",
        7 => "SCV",
        8 => "Wraith",
        9 => "Science Vessel",
        11 => "Dropship",
        12 => "Battlecruiser",
        13 => "Spider Mine",
        14 => "Nuclear Missile",
        15 => "Civilian",
        30 => "Siege Tank (Siege)",
        32 => "Firebat",
        34 => "Medic",
        58 => "Valkyrie",
        // Zerg Units
        35 => "Larva",
        36 => "Egg",
        37 => "Zergling",
        38 => "Hydralisk",
        39 => "Ultralisk",
        40 => "Broodling",
        41 => "Drone",
        42 => "Overlord",
        43 => "Mutalisk",
        44 => "Guardian",
        45 => "Queen",
        46 => "Defiler",
        47 => "Scourge",
        50 => "Infested Terran",
        59 => "Cocoon",
        62 => "Devourer",
        97 => "Lurker Egg",
        103 => "Lurker",
        // Protoss Units
        60 => "Corsair",
        61 => "Dark Templar",
        63 => "Dark Archon",
        64 => "Probe",
        65 => "Zealot",
        66 => "Dragoon",
        67 => "High Templar",
        68 => "Archon",
        69 => "Shuttle",
        70 => "Scout",
        71 => "Arbiter",
        72 => "Carrier",
        73 => "Interceptor",
        83 => "Reaver",
        84 => "Observer",
        85 => "Scarab",
        // Terran Buildings
        106 => "Command Center",
        107 => "Comsat Station",
        108 => "Nuclear Silo",
        109 => "Supply Depot",
        110 => "Refinery",
        111 => "Barracks",
        112 => "Academy",
        113 => "Factory",
        114 => "Starport",
        115 => "Control Tower",
        116 => "Science Facility",
        117 => "Covert Ops",
        118 => "Physics Lab",
        120 => "Machine Shop",
        122 => "Engineering Bay",
        123 => "Armory",
        124 => "Missile Turret",
        125 => "Bunker",
        // Zerg Buildings
        130 => "Infested Command Center",
        131 => "Hatchery",
        132 => "Lair",
        133 => "Hive",
        134 => "Nydus Canal",
        135 => "Hydralisk Den",
        136 => "Defiler Mound",
        137 => "Greater Spire",
        138 => "Queen's Nest",
        139 => "Evolution Chamber",
        140 => "Ultralisk Cavern",
        141 => "Spire",
        142 => "Spawning Pool",
        143 => "Creep Colony",
        144 => "Spore Colony",
        146 => "Sunken Colony",
        149 => "Extractor",
        // Protoss Buildings
        154 => "Nexus",
        155 => "Robotics Facility",
        156 => "Pylon",
        157 => "Assimilator",
        159 => "Observatory",
        160 => "Gateway",
        162 => "Photon Cannon",
        163 => "Citadel of Adun",
        164 => "Cybernetics Core",
        165 => "Templar Archives",
        166 => "Forge",
        167 => "Stargate",
        169 => "Fleet Beacon",
        170 => "Arbiter Tribunal",
        171 => "Robotics Support Bay",
        172 => "Shield Battery",
        // Heroes (abbreviated — these rarely appear in competitive replays)
        10 => "Gui Montag",
        16 => "Sarah Kerrigan",
        19 => "Jim Raynor (Vulture)",
        20 => "Jim Raynor (Marine)",
        21 => "Tom Kazansky",
        22 => "Magellan",
        23 => "Edmund Duke (Tank)",
        25 => "Edmund Duke (Siege)",
        27 => "Arcturus Mengsk",
        28 => "Hyperion",
        29 => "Norad II",
        48 => "Torrasque",
        49 => "Matriarch",
        51 => "Infested Kerrigan",
        52 => "Unclean One",
        53 => "Hunter Killer",
        54 => "Devouring One",
        55 => "Kukulza (Mutalisk)",
        56 => "Kukulza (Guardian)",
        57 => "Yggdrasill",
        74 => "Dark Templar (Hero)",
        75 => "Zeratul",
        76 => "Tassadar/Zeratul",
        77 => "Fenix (Zealot)",
        78 => "Fenix (Dragoon)",
        79 => "Tassadar",
        80 => "Mojo",
        81 => "Warbringer",
        82 => "Gantrithor",
        86 => "Danimoth",
        87 => "Aldaris",
        88 => "Artanis",
        98 => "Raszagal",
        99 => "Samir Duran",
        100 => "Alexei Stukov",
        102 => "Gerard DuGalle",
        104 => "Infested Duran",
        _ => "Unknown Unit",
    }
}

/// Look up the name of a tech type by its ID.
pub fn tech_name(id: u8) -> &'static str {
    match id {
        0 => "Stim Packs",
        1 => "Lockdown",
        2 => "EMP Shockwave",
        3 => "Spider Mines",
        4 => "Scanner Sweep",
        5 => "Tank Siege Mode",
        6 => "Defensive Matrix",
        7 => "Irradiate",
        8 => "Yamato Gun",
        9 => "Cloaking Field",
        10 => "Personnel Cloaking",
        11 => "Burrowing",
        12 => "Infestation",
        13 => "Spawn Broodlings",
        14 => "Dark Swarm",
        15 => "Plague",
        16 => "Consume",
        17 => "Ensnare",
        18 => "Parasite",
        19 => "Psionic Storm",
        20 => "Hallucination",
        21 => "Recall",
        22 => "Stasis Field",
        23 => "Archon Warp",
        24 => "Restoration",
        25 => "Disruption Web",
        27 => "Mind Control",
        28 => "Dark Archon Meld",
        29 => "Feedback",
        30 => "Optical Flare",
        31 => "Maelstrom",
        32 => "Lurker Aspect",
        34 => "Healing",
        45 => "Nuclear Strike",
        _ => "Unknown Tech",
    }
}

/// Look up the name of an upgrade type by its ID.
pub fn upgrade_name(id: u8) -> &'static str {
    match id {
        0 => "Infantry Armor",
        1 => "Vehicle Plating",
        2 => "Ship Plating",
        3 => "Zerg Carapace",
        4 => "Zerg Flyer Carapace",
        5 => "Protoss Ground Armor",
        6 => "Protoss Air Armor",
        7 => "Infantry Weapons",
        8 => "Vehicle Weapons",
        9 => "Ship Weapons",
        10 => "Zerg Melee Attacks",
        11 => "Zerg Missile Attacks",
        12 => "Zerg Flyer Attacks",
        13 => "Protoss Ground Weapons",
        14 => "Protoss Air Weapons",
        15 => "Protoss Plasma Shields",
        16 => "U-238 Shells",
        17 => "Ion Thrusters",
        19 => "Titan Reactor",
        20 => "Ocular Implants",
        21 => "Moebius Reactor",
        22 => "Apollo Reactor",
        23 => "Colossus Reactor",
        24 => "Ventral Sacs",
        25 => "Antennae",
        26 => "Pneumatized Carapace",
        27 => "Metabolic Boost",
        28 => "Adrenal Glands",
        29 => "Muscular Augments",
        30 => "Grooved Spines",
        31 => "Gamete Meiosis",
        32 => "Metasynaptic Node",
        33 => "Singularity Charge",
        34 => "Leg Enhancements",
        35 => "Scarab Damage",
        36 => "Reaver Capacity",
        37 => "Gravitic Drive",
        38 => "Sensor Array",
        39 => "Gravitic Boosters",
        40 => "Khaydarin Amulet",
        41 => "Apial Sensors",
        42 => "Gravitic Thrusters",
        43 => "Carrier Capacity",
        44 => "Khaydarin Core",
        47 => "Argus Jewel",
        49 => "Argus Talisman",
        51 => "Caduceus Reactor",
        52 => "Chitinous Plating",
        53 => "Anabolic Synthesis",
        54 => "Charon Boosters",
        _ => "Unknown Upgrade",
    }
}

/// Determine which race a unit type belongs to.
pub fn unit_race(id: u16) -> Option<&'static str> {
    match id {
        0..=15 | 30..=34 | 58 | 106..=125 => Some("Terran"),
        35..=57 | 59 | 62 | 97 | 103 | 130..=146 | 149 => Some("Zerg"),
        60..=61 | 63..=73 | 83..=85 | 154..=172 => Some("Protoss"),
        _ => None,
    }
}

/// Whether a unit type is a building.
pub fn is_building(id: u16) -> bool {
    matches!(id, 106..=125 | 130..=146 | 149 | 154..=172)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_unit_names() {
        assert_eq!(unit_name(0), "Marine");
        assert_eq!(unit_name(37), "Zergling");
        assert_eq!(unit_name(41), "Drone");
        assert_eq!(unit_name(64), "Probe");
        assert_eq!(unit_name(65), "Zealot");
        assert_eq!(unit_name(66), "Dragoon");
        assert_eq!(unit_name(103), "Lurker");
        assert_eq!(unit_name(131), "Hatchery");
        assert_eq!(unit_name(160), "Gateway");
        assert_eq!(unit_name(111), "Barracks");
    }

    #[test]
    fn test_common_tech_names() {
        assert_eq!(tech_name(0), "Stim Packs");
        assert_eq!(tech_name(5), "Tank Siege Mode");
        assert_eq!(tech_name(19), "Psionic Storm");
        assert_eq!(tech_name(32), "Lurker Aspect");
    }

    #[test]
    fn test_common_upgrade_names() {
        assert_eq!(upgrade_name(27), "Metabolic Boost");
        assert_eq!(upgrade_name(34), "Leg Enhancements");
        assert_eq!(upgrade_name(33), "Singularity Charge");
        assert_eq!(upgrade_name(16), "U-238 Shells");
    }

    #[test]
    fn test_unit_race() {
        assert_eq!(unit_race(0), Some("Terran"));
        assert_eq!(unit_race(37), Some("Zerg"));
        assert_eq!(unit_race(65), Some("Protoss"));
        assert_eq!(unit_race(131), Some("Zerg"));
        assert_eq!(unit_race(160), Some("Protoss"));
        assert_eq!(unit_race(255), None);
    }

    #[test]
    fn test_is_building() {
        assert!(is_building(111)); // Barracks
        assert!(is_building(131)); // Hatchery
        assert!(is_building(160)); // Gateway
        assert!(!is_building(0)); // Marine
        assert!(!is_building(37)); // Zergling
    }

    #[test]
    fn test_unknown_ids() {
        assert_eq!(unit_name(999), "Unknown Unit");
        assert_eq!(tech_name(255), "Unknown Tech");
        assert_eq!(upgrade_name(255), "Unknown Upgrade");
    }
}
