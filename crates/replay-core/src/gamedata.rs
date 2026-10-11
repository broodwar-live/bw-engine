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

/// Cost of a unit or building: (minerals, gas, supply_cost).
/// Supply cost is 0 for buildings. Zerglings return 1 (pair costs 50/0/1 each).
pub fn unit_cost(id: u16) -> (u32, u32, u32) {
    match id {
        // Terran units
        0 => (50, 0, 1),     // Marine
        1 => (25, 75, 1),    // Ghost
        2 => (75, 0, 2),     // Vulture
        3 => (100, 50, 2),   // Goliath
        5 => (150, 100, 2),  // Siege Tank
        7 => (50, 0, 1),     // SCV
        8 => (150, 100, 2),  // Wraith
        9 => (100, 225, 2),  // Science Vessel
        11 => (100, 100, 2), // Dropship
        12 => (400, 300, 6), // Battlecruiser
        32 => (50, 25, 1),   // Firebat
        34 => (50, 25, 1),   // Medic
        58 => (250, 125, 3), // Valkyrie
        // Zerg units
        37 => (25, 0, 1),     // Zergling (half of pair)
        38 => (75, 25, 1),    // Hydralisk
        39 => (200, 200, 4),  // Ultralisk
        41 => (50, 0, 1),     // Drone
        42 => (100, 0, 0),    // Overlord (provides supply)
        43 => (100, 100, 2),  // Mutalisk
        44 => (150, 200, 2),  // Guardian (morph from muta)
        45 => (100, 100, 2),  // Queen
        46 => (50, 150, 2),   // Defiler
        47 => (13, 38, 1),    // Scourge (half of pair)
        62 => (250, 150, 2),  // Devourer (morph from muta)
        103 => (125, 125, 2), // Lurker (morph from hydra, additional cost)
        // Protoss units
        60 => (150, 100, 2), // Corsair
        61 => (125, 100, 2), // Dark Templar
        63 => (0, 0, 4),     // Dark Archon (merge)
        64 => (50, 0, 1),    // Probe
        65 => (100, 0, 2),   // Zealot
        66 => (125, 50, 2),  // Dragoon
        67 => (50, 150, 2),  // High Templar
        68 => (0, 0, 4),     // Archon (merge)
        69 => (200, 0, 2),   // Shuttle
        70 => (275, 125, 3), // Scout
        71 => (100, 350, 4), // Arbiter
        72 => (350, 250, 6), // Carrier
        73 => (25, 0, 0),    // Interceptor
        83 => (200, 100, 4), // Reaver
        84 => (25, 75, 1),   // Observer
        85 => (15, 0, 0),    // Scarab
        // Terran buildings
        106 => (400, 0, 0),   // Command Center
        107 => (50, 50, 0),   // Comsat Station
        108 => (100, 100, 0), // Nuclear Silo
        109 => (100, 0, 0),   // Supply Depot
        110 => (100, 0, 0),   // Refinery
        111 => (150, 0, 0),   // Barracks
        112 => (150, 0, 0),   // Academy
        113 => (200, 100, 0), // Factory
        114 => (150, 100, 0), // Starport
        115 => (50, 50, 0),   // Control Tower
        116 => (100, 150, 0), // Science Facility
        117 => (50, 50, 0),   // Covert Ops
        118 => (50, 50, 0),   // Physics Lab
        120 => (50, 50, 0),   // Machine Shop
        122 => (125, 0, 0),   // Engineering Bay
        123 => (100, 50, 0),  // Armory
        124 => (75, 0, 0),    // Missile Turret
        125 => (100, 0, 0),   // Bunker
        // Zerg buildings
        131 => (300, 0, 0),   // Hatchery
        132 => (150, 100, 0), // Lair (morph)
        133 => (200, 150, 0), // Hive (morph)
        134 => (160, 0, 0),   // Nydus Canal
        135 => (100, 50, 0),  // Hydralisk Den
        136 => (100, 100, 0), // Defiler Mound
        137 => (100, 150, 0), // Greater Spire (morph)
        138 => (150, 100, 0), // Queen's Nest
        139 => (75, 0, 0),    // Evolution Chamber
        140 => (150, 200, 0), // Ultralisk Cavern
        141 => (200, 150, 0), // Spire
        142 => (200, 0, 0),   // Spawning Pool
        143 => (75, 0, 0),    // Creep Colony
        144 => (50, 0, 0),    // Spore Colony (morph)
        146 => (50, 0, 0),    // Sunken Colony (morph)
        149 => (50, 0, 0),    // Extractor
        // Protoss buildings
        154 => (400, 0, 0),   // Nexus
        155 => (200, 200, 0), // Robotics Facility
        156 => (100, 0, 0),   // Pylon
        157 => (100, 0, 0),   // Assimilator
        159 => (50, 100, 0),  // Observatory
        160 => (150, 0, 0),   // Gateway
        162 => (150, 0, 0),   // Photon Cannon
        163 => (150, 100, 0), // Citadel of Adun
        164 => (200, 0, 0),   // Cybernetics Core
        165 => (150, 200, 0), // Templar Archives
        166 => (150, 0, 0),   // Forge
        167 => (150, 150, 0), // Stargate
        169 => (300, 200, 0), // Fleet Beacon
        170 => (200, 150, 0), // Arbiter Tribunal
        171 => (150, 100, 0), // Robotics Support Bay
        172 => (100, 0, 0),   // Shield Battery
        _ => (0, 0, 0),
    }
}

/// Supply provided by a unit/building.
pub fn supply_provided(id: u16) -> u32 {
    match id {
        106 => 10, // Command Center
        109 => 8,  // Supply Depot
        131 => 1,  // Hatchery
        132 => 1,  // Lair
        133 => 1,  // Hive
        42 => 8,   // Overlord
        154 => 9,  // Nexus
        156 => 8,  // Pylon
        _ => 0,
    }
}

/// Build/train/morph time in game frames (units.dat values; 15 frames are one
/// "game second", one frame is 42 ms on Fastest). 0 if unknown.
pub fn build_time(id: u16) -> u32 {
    match id {
        // Terran units
        0 | 32 => 360,          // Marine, Firebat
        1 | 5 | 11 | 58 => 750, // Ghost, Siege Tank, Dropship, Valkyrie
        2 | 34 => 450,          // Vulture, Medic
        3 => 600,               // Goliath
        7 | 41 | 64 => 300,     // SCV, Drone, Probe
        8 => 900,               // Wraith
        9 => 1200,              // Science Vessel
        12 => 2000,             // Battlecruiser
        // Zerg units
        37 | 38 => 420,                 // Zergling, Hydralisk
        39 => 900,                      // Ultralisk
        42 | 43 | 44 | 62 | 103 => 600, // Overlord, Mutalisk, Guardian, Devourer, Lurker
        45 | 46 => 750,                 // Queen, Defiler
        47 => 450,                      // Scourge
        // Protoss units
        60 | 65 | 84 => 600, // Corsair, Zealot, Observer
        61 | 66 | 67 => 750, // Dark Templar, Dragoon, High Templar
        69 => 900,           // Shuttle
        70 => 1200,          // Scout
        71 => 2400,          // Arbiter
        72 => 2100,          // Carrier
        83 => 1050,          // Reaver
        // Terran buildings
        106 | 131 | 154 => 1800,       // Command Center, Hatchery, Nexus
        109 | 110 | 149 | 157 => 600,  // Supply Depot, Refinery, Extractor, Assimilator
        111..=113 | 123 | 142 => 1200, // Barracks, Academy, Factory, Armory, Spawning Pool
        114 | 167 => 1050,             // Starport, Stargate
        116 | 122 | 160 | 163 | 164 => 900, // Science Facility, Engineering Bay, Gateway, Citadel, Core
        124 | 125 | 156 => 450,             // Missile Turret, Bunker, Pylon
        132 => 1500,                        // Lair
        133 | 141 => 1800,                  // Hive, Spire
        135 | 139 | 166 => 600,             // Hydralisk Den, Evolution Chamber, Forge
        143 => 300,                         // Creep Colony
        155 => 1200,                        // Robotics Facility
        162 | 165 => 750,                   // Photon Cannon, Templar Archives
        _ => 0,
    }
}

/// Cost of a tech research: (minerals, gas).
pub fn tech_cost(id: u8) -> (u32, u32) {
    match id {
        0 => (100, 100),  // Stim Packs
        1 => (200, 200),  // Lockdown
        2 => (200, 200),  // EMP Shockwave
        3 => (100, 100),  // Spider Mines
        5 => (150, 150),  // Tank Siege Mode
        7 => (200, 200),  // Irradiate
        8 => (100, 100),  // Yamato Gun
        9 => (150, 150),  // Cloaking Field
        10 => (100, 100), // Personnel Cloaking
        11 => (100, 100), // Burrowing
        13 => (100, 100), // Spawn Broodlings
        15 => (200, 100), // Plague
        16 => (100, 100), // Consume
        17 => (100, 100), // Ensnare
        19 => (200, 200), // Psionic Storm
        20 => (150, 150), // Hallucination
        21 => (150, 150), // Recall
        22 => (150, 100), // Stasis Field
        24 => (100, 100), // Restoration
        25 => (200, 200), // Disruption Web
        27 => (200, 200), // Mind Control
        30 => (100, 100), // Optical Flare
        31 => (100, 100), // Maelstrom
        32 => (200, 200), // Lurker Aspect
        _ => (0, 0),
    }
}

/// Cost of an upgrade at level 1: (minerals, gas).
/// Many upgrades have increasing costs per level; this returns base cost.
pub fn upgrade_cost(id: u8) -> (u32, u32) {
    match id {
        0 | 7 => (100, 100),           // Infantry Armor / Weapons
        1 | 2 | 8 | 9 => (100, 100),   // Vehicle/Ship Plating/Weapons
        3 | 10 | 11 => (100, 100),     // Zerg Carapace / Melee / Missile
        4 | 12 => (100, 100),          // Zerg Flyer Carapace / Attacks
        5 | 6 | 13 | 14 => (100, 100), // Protoss Armor / Weapons
        15 => (200, 200),              // Plasma Shields
        16 => (150, 150),              // U-238 Shells
        17 => (100, 100),              // Ion Thrusters
        24 => (200, 200),              // Ventral Sacs
        25 => (150, 150),              // Antennae
        26 => (150, 150),              // Pneumatized Carapace
        27 => (100, 100),              // Metabolic Boost
        28 => (200, 200),              // Adrenal Glands
        29 => (150, 150),              // Muscular Augments
        30 => (150, 150),              // Grooved Spines
        33 => (150, 50),               // Singularity Charge
        34 => (150, 150),              // Leg Enhancements
        35 => (100, 50),               // Scarab Damage
        36 => (200, 200),              // Reaver Capacity
        37 => (150, 150),              // Gravitic Drive
        38 => (100, 100),              // Sensor Array
        39 => (150, 150),              // Gravitic Boosters
        40 => (150, 150),              // Khaydarin Amulet
        43 => (100, 100),              // Carrier Capacity
        52 => (150, 150),              // Chitinous Plating
        53 => (200, 200),              // Anabolic Synthesis
        54 => (100, 100),              // Charon Boosters
        _ => (0, 0),
    }
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
