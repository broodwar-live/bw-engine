use crate::error::{ReplayError, Result};

/// Expected size of the header section (section 1).
pub const HEADER_SIZE: usize = 0x279; // 633 bytes

/// Offset where the 12-element player slot array begins.
const PLAYER_SLOTS_OFFSET: usize = 0xA1;
/// Size of each player slot entry.
const PLAYER_SLOT_SIZE: usize = 36;
/// Number of player slots.
const PLAYER_SLOT_COUNT: usize = 12;
/// Offset of the player color array (8 x u32).
const PLAYER_COLORS_OFFSET: usize = 0x251;

/// Expected size of the player names section (section 4).
pub const PLAYER_NAMES_SIZE: usize = 0x300; // 768 bytes
/// Size of each extended name entry.
const EXTENDED_NAME_SIZE: usize = 96;

/// Parsed replay header metadata.
#[derive(Debug, Clone)]
pub struct Header {
    pub engine: Engine,
    pub frame_count: u32,
    pub start_time: u32,
    pub game_title: String,
    pub map_width: u16,
    pub map_height: u16,
    pub game_speed: Speed,
    pub game_type: GameType,
    pub host_name: String,
    pub map_name: String,
    pub players: Vec<Player>,
}

impl Header {
    /// Duration in milliseconds (1 frame = 42ms).
    pub fn duration_ms(&self) -> u64 {
        self.frame_count as u64 * 42
    }

    /// Duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_ms() as f64 / 1000.0
    }
}

/// Game engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    StarCraft,
    BroodWar,
}

/// Game speed setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Slowest,
    Slower,
    Slow,
    Normal,
    Fast,
    Faster,
    Fastest,
    Unknown(u8),
}

/// Game type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameType {
    None,
    Custom,
    Melee,
    FreeForAll,
    OneOnOne,
    CaptureTheFlag,
    Greed,
    Slaughter,
    SuddenDeath,
    Ladder,
    UseMapSettings,
    TeamMelee,
    TeamFreeForAll,
    TeamCaptureTheFlag,
    TopVsBottom,
    IronManLadder,
    Unknown(u16),
}

/// A player's race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Race {
    Zerg,
    Terran,
    Protoss,
    Unknown(u8),
}

impl Race {
    /// Short code used in matchup notation ("T", "P", "Z").
    pub fn code(&self) -> &str {
        match self {
            Race::Terran => "T",
            Race::Protoss => "P",
            Race::Zerg => "Z",
            Race::Unknown(_) => "?",
        }
    }
}

/// Player type (human, computer, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerType {
    Inactive,
    Computer,
    Human,
    RescuePassive,
    ComputerControlled,
    Open,
    Neutral,
    Closed,
    Unknown(u8),
}

impl PlayerType {
    pub fn is_active(&self) -> bool {
        matches!(self, PlayerType::Human | PlayerType::Computer)
    }
}

/// A player parsed from the replay header.
#[derive(Debug, Clone)]
pub struct Player {
    pub slot_id: u16,
    pub player_id: u8,
    pub player_type: PlayerType,
    pub race: Race,
    pub team: u8,
    pub name: String,
    pub color: u32,
}

// -- Parsing --

fn read_u8(data: &[u8], offset: usize) -> u8 {
    data[offset]
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Decode a null-terminated byte string, trying UTF-8 first, then EUC-KR.
fn decode_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let raw = &bytes[..end];

    if raw.is_empty() {
        return String::new();
    }

    // Try UTF-8 first.
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_owned();
    }

    // Fall back to EUC-KR (CP949) for Korean names.
    let (decoded, _, _) = encoding_rs::EUC_KR.decode(raw);
    decoded.into_owned()
}

/// Parse the header from decompressed section 1 data.
pub fn parse_header(data: &[u8]) -> Result<Header> {
    if data.len() < HEADER_SIZE {
        return Err(ReplayError::InvalidHeader(format!(
            "header too short: {} bytes (expected {})",
            data.len(),
            HEADER_SIZE
        )));
    }

    let engine = match read_u8(data, 0x00) {
        0x00 => Engine::StarCraft,
        0x01 => Engine::BroodWar,
        v => {
            return Err(ReplayError::InvalidHeader(format!(
                "unknown engine byte: 0x{v:02X}"
            )));
        }
    };

    let frame_count = read_u32(data, 0x01);
    let start_time = read_u32(data, 0x08);
    let game_title = decode_string(&data[0x18..0x18 + 28]);
    let map_width = read_u16(data, 0x34);
    let map_height = read_u16(data, 0x36);
    let game_speed = parse_speed(read_u8(data, 0x3A));
    let game_type = parse_game_type(read_u16(data, 0x3C));
    let host_name = decode_string(&data[0x48..0x48 + 24]);
    let map_name = decode_string(&data[0x61..0x61 + 26]);

    // Parse player slots.
    let mut players = Vec::new();
    for i in 0..PLAYER_SLOT_COUNT {
        let base = PLAYER_SLOTS_OFFSET + i * PLAYER_SLOT_SIZE;
        let player_type = parse_player_type(read_u8(data, base + 8));
        if !player_type.is_active() {
            continue;
        }

        let slot_id = read_u16(data, base);
        let player_id = read_u8(data, base + 4);
        let race = parse_race(read_u8(data, base + 9));
        let team = read_u8(data, base + 10);
        let name = decode_string(&data[base + 11..base + 11 + 25]);

        let color = if i < 8 {
            read_u32(data, PLAYER_COLORS_OFFSET + i * 4)
        } else {
            0
        };

        players.push(Player {
            slot_id,
            player_id,
            player_type,
            race,
            team,
            name,
            color,
        });
    }

    Ok(Header {
        engine,
        frame_count,
        start_time,
        game_title,
        map_width,
        map_height,
        game_speed,
        game_type,
        host_name,
        map_name,
        players,
    })
}

/// Override truncated player names (25 bytes) with extended names (96 bytes)
/// from section 4.
pub fn apply_extended_names(header: &mut Header, section4: &[u8]) {
    if section4.len() < PLAYER_NAMES_SIZE {
        return;
    }

    for player in &mut header.players {
        let slot = player.slot_id as usize;
        if slot >= PLAYER_SLOT_COUNT {
            continue;
        }
        let base = slot * EXTENDED_NAME_SIZE;
        let extended = decode_string(&section4[base..base + EXTENDED_NAME_SIZE]);
        if !extended.is_empty() {
            player.name = extended;
        }
    }
}

fn parse_speed(v: u8) -> Speed {
    match v {
        0 => Speed::Slowest,
        1 => Speed::Slower,
        2 => Speed::Slow,
        3 => Speed::Normal,
        4 => Speed::Fast,
        5 => Speed::Faster,
        6 => Speed::Fastest,
        _ => Speed::Unknown(v),
    }
}

fn parse_game_type(v: u16) -> GameType {
    match v {
        0x00 => GameType::None,
        0x01 => GameType::Custom,
        0x02 => GameType::Melee,
        0x03 => GameType::FreeForAll,
        0x04 => GameType::OneOnOne,
        0x05 => GameType::CaptureTheFlag,
        0x06 => GameType::Greed,
        0x07 => GameType::Slaughter,
        0x08 => GameType::SuddenDeath,
        0x09 => GameType::Ladder,
        0x0A => GameType::UseMapSettings,
        0x0B => GameType::TeamMelee,
        0x0C => GameType::TeamFreeForAll,
        0x0D => GameType::TeamCaptureTheFlag,
        0x0F => GameType::TopVsBottom,
        0x10 => GameType::IronManLadder,
        _ => GameType::Unknown(v),
    }
}

fn parse_race(v: u8) -> Race {
    match v {
        0x00 => Race::Zerg,
        0x01 => Race::Terran,
        0x02 => Race::Protoss,
        _ => Race::Unknown(v),
    }
}

fn parse_player_type(v: u8) -> PlayerType {
    match v {
        0x00 => PlayerType::Inactive,
        0x01 => PlayerType::Computer,
        0x02 => PlayerType::Human,
        0x03 => PlayerType::RescuePassive,
        0x05 => PlayerType::ComputerControlled,
        0x06 => PlayerType::Open,
        0x07 => PlayerType::Neutral,
        0x08 => PlayerType::Closed,
        _ => PlayerType::Unknown(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_header() -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];

        // Engine: Brood War
        data[0x00] = 0x01;
        // Frame count: 10000
        data[0x01..0x05].copy_from_slice(&10000u32.to_le_bytes());
        // Start time
        data[0x08..0x0C].copy_from_slice(&1700000000u32.to_le_bytes());
        // Game title
        data[0x18..0x18 + 6].copy_from_slice(b"1v1 me");
        // Map dimensions
        data[0x34..0x36].copy_from_slice(&128u16.to_le_bytes());
        data[0x36..0x38].copy_from_slice(&128u16.to_le_bytes());
        // Speed: Fastest
        data[0x3A] = 0x06;
        // Game type: Melee
        data[0x3C..0x3E].copy_from_slice(&0x02u16.to_le_bytes());
        // Host name
        data[0x48..0x48 + 5].copy_from_slice(b"Flash");
        // Map name
        data[0x61..0x61 + 8].copy_from_slice(b"Polypoid");

        // Player 0: Human Terran "Flash"
        let p0 = PLAYER_SLOTS_OFFSET;
        data[p0..p0 + 2].copy_from_slice(&0u16.to_le_bytes()); // slot_id
        data[p0 + 4] = 0; // player_id
        data[p0 + 8] = 0x02; // Human
        data[p0 + 9] = 0x01; // Terran
        data[p0 + 10] = 0; // team
        data[p0 + 11..p0 + 11 + 5].copy_from_slice(b"Flash");

        // Player 1: Human Protoss "Rain"
        let p1 = PLAYER_SLOTS_OFFSET + PLAYER_SLOT_SIZE;
        data[p1..p1 + 2].copy_from_slice(&1u16.to_le_bytes());
        data[p1 + 4] = 1;
        data[p1 + 8] = 0x02; // Human
        data[p1 + 9] = 0x02; // Protoss
        data[p1 + 10] = 1;
        data[p1 + 11..p1 + 11 + 4].copy_from_slice(b"Rain");

        // Colors
        data[PLAYER_COLORS_OFFSET..PLAYER_COLORS_OFFSET + 4].copy_from_slice(&0u32.to_le_bytes()); // Red
        data[PLAYER_COLORS_OFFSET + 4..PLAYER_COLORS_OFFSET + 8]
            .copy_from_slice(&1u32.to_le_bytes()); // Blue

        data
    }

    #[test]
    fn test_parse_header_basic_fields() {
        let data = build_test_header();
        let header = parse_header(&data).unwrap();

        assert_eq!(header.engine, Engine::BroodWar);
        assert_eq!(header.frame_count, 10000);
        assert_eq!(header.start_time, 1700000000);
        assert_eq!(header.game_title, "1v1 me");
        assert_eq!(header.map_width, 128);
        assert_eq!(header.map_height, 128);
        assert_eq!(header.game_speed, Speed::Fastest);
        assert_eq!(header.game_type, GameType::Melee);
        assert_eq!(header.host_name, "Flash");
        assert_eq!(header.map_name, "Polypoid");
    }

    #[test]
    fn test_parse_header_players() {
        let data = build_test_header();
        let header = parse_header(&data).unwrap();

        assert_eq!(header.players.len(), 2);

        assert_eq!(header.players[0].name, "Flash");
        assert_eq!(header.players[0].race, Race::Terran);
        assert_eq!(header.players[0].player_type, PlayerType::Human);
        assert_eq!(header.players[0].race.code(), "T");

        assert_eq!(header.players[1].name, "Rain");
        assert_eq!(header.players[1].race, Race::Protoss);
        assert_eq!(header.players[1].race.code(), "P");
    }

    #[test]
    fn test_parse_header_duration() {
        let data = build_test_header();
        let header = parse_header(&data).unwrap();
        assert_eq!(header.duration_ms(), 420000);
        assert!((header.duration_secs() - 420.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_header_too_short() {
        let data = vec![0u8; 100];
        assert!(parse_header(&data).is_err());
    }

    #[test]
    fn test_apply_extended_names() {
        let data = build_test_header();
        let mut header = parse_header(&data).unwrap();

        let mut section4 = vec![0u8; PLAYER_NAMES_SIZE];
        // Slot 0: extended name "FlashKT"
        section4[0..7].copy_from_slice(b"FlashKT");
        // Slot 1: extended name "RainCJ"
        section4[EXTENDED_NAME_SIZE..EXTENDED_NAME_SIZE + 6].copy_from_slice(b"RainCJ");

        apply_extended_names(&mut header, &section4);
        assert_eq!(header.players[0].name, "FlashKT");
        assert_eq!(header.players[1].name, "RainCJ");
    }

    #[test]
    fn test_decode_string_euc_kr() {
        // "이영호" (Lee Young Ho / Flash) in EUC-KR
        let euc_kr_bytes: &[u8] = &[0xC0, 0xCC, 0xBF, 0xB5, 0xC8, 0xA3, 0x00];
        let result = decode_string(euc_kr_bytes);
        assert_eq!(result, "이영호");
    }

    #[test]
    fn test_decode_string_utf8() {
        let utf8_bytes = b"Flash\x00extra";
        let result = decode_string(utf8_bytes);
        assert_eq!(result, "Flash");
    }

    #[test]
    fn test_inactive_slots_filtered() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0x00] = 0x01; // BW

        // All 12 slots at default (type 0x00 = Inactive) → no players
        let header = parse_header(&data).unwrap();
        assert!(header.players.is_empty());
    }
}
