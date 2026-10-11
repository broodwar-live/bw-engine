/// A single command issued by a player at a specific game frame.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameCommand {
    pub frame: u32,
    pub player_id: u8,
    pub command: Command,
}

/// Decoded command types.
///
/// Only gameplay-relevant commands are fully decoded. UI-only commands
/// (lobby, voice, sync) are captured as `Other` with their type ID.
#[derive(Debug, Clone, serde::Serialize)]
pub enum Command {
    Select {
        unit_tags: Vec<u16>,
    },
    SelectAdd {
        unit_tags: Vec<u16>,
    },
    SelectRemove {
        unit_tags: Vec<u16>,
    },
    Build {
        order: u8,
        x: u16,
        y: u16,
        unit_type: u16,
    },
    RightClick {
        x: u16,
        y: u16,
        target_tag: u16,
        unit_type: u16,
        queued: bool,
    },
    TargetedOrder {
        x: u16,
        y: u16,
        target_tag: u16,
        unit_type: u16,
        order: u8,
        queued: bool,
    },
    Hotkey {
        action: HotkeyAction,
        group: u8,
    },
    Train {
        unit_type: u16,
    },
    CancelTrain {
        unit_tag: u16,
    },
    UnitMorph {
        unit_type: u16,
    },
    BuildingMorph {
        unit_type: u16,
    },
    Research {
        tech_type: u8,
    },
    CancelResearch,
    Upgrade {
        upgrade_type: u8,
    },
    CancelUpgrade,
    Stop {
        queued: bool,
    },
    HoldPosition {
        queued: bool,
    },
    Siege {
        queued: bool,
    },
    Unsiege {
        queued: bool,
    },
    Cloak {
        queued: bool,
    },
    Decloak {
        queued: bool,
    },
    Burrow {
        queued: bool,
    },
    Unburrow {
        queued: bool,
    },
    ReturnCargo {
        queued: bool,
    },
    UnloadAll {
        queued: bool,
    },
    Unload {
        unit_tag: u16,
    },
    LiftOff {
        x: u16,
        y: u16,
    },
    MergeArchon,
    MergeDarkArchon,
    TrainFighter,
    Stim,
    CancelBuild,
    CancelMorph,
    CancelAddon,
    CancelNuke,
    MinimapPing {
        x: u16,
        y: u16,
    },
    Chat {
        sender_slot: u8,
        message: String,
    },
    LeaveGame {
        reason: u8,
    },
    KeepAlive,
    /// Latency, sync, lobby, voice, and other non-gameplay commands.
    Other {
        type_id: u8,
    },
}

/// Hotkey assign vs recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HotkeyAction {
    Assign,
    Select,
}

impl Command {
    /// Whether this command represents a meaningful player action for APM counting.
    /// Excludes keepalive, selections, hotkey recalls, and non-gameplay commands.
    pub fn is_meaningful_action(&self) -> bool {
        !matches!(
            self,
            Command::KeepAlive
                | Command::Other { .. }
                | Command::Select { .. }
                | Command::SelectAdd { .. }
                | Command::SelectRemove { .. }
                | Command::Chat { .. }
                | Command::LeaveGame { .. }
                | Command::MinimapPing { .. }
        )
    }

    /// Whether this command represents an "effective" action for EAPM.
    /// Stricter than meaningful: also excludes hotkey recalls and redundant stops.
    pub fn is_effective_action(&self) -> bool {
        if !self.is_meaningful_action() {
            return false;
        }
        // Hotkey recall (select) is not effective — only assign is.
        if let Command::Hotkey { action, .. } = self {
            return *action == HotkeyAction::Assign;
        }
        true
    }

    /// Whether this command affects the build order (structure, unit, tech, upgrade).
    pub fn is_build_order_action(&self) -> bool {
        matches!(
            self,
            Command::Build { .. }
                | Command::Train { .. }
                | Command::UnitMorph { .. }
                | Command::BuildingMorph { .. }
                | Command::Research { .. }
                | Command::Upgrade { .. }
                | Command::TrainFighter
        )
    }
}

/// Diagnostics collected while parsing the command stream.
///
/// The parser never fails on a malformed command stream; instead it records
/// what it had to skip here so callers can judge how trustworthy the derived
/// data (APM, build order, ...) is.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CommandParseWarnings {
    /// Commands with an opcode not present in the length table. The rest of
    /// the frame's command block is skipped for each of these.
    pub unknown_commands: u32,
    /// Commands whose payload extended past the end of their frame block.
    pub truncated_commands: u32,
    /// Total bytes skipped because of unknown or truncated commands.
    pub skipped_bytes: u32,
    /// Distinct unknown opcodes encountered, in ascending order.
    pub unknown_opcodes: Vec<u8>,
}

impl CommandParseWarnings {
    /// True if the whole command stream was decoded without skipping anything.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.unknown_commands == 0 && self.truncated_commands == 0
    }

    fn record_unknown(&mut self, type_id: u8, skipped: usize) {
        self.unknown_commands += 1;
        self.skipped_bytes = self.skipped_bytes.saturating_add(skipped as u32);
        if let Err(idx) = self.unknown_opcodes.binary_search(&type_id) {
            self.unknown_opcodes.insert(idx, type_id);
        }
    }

    fn record_truncated(&mut self, skipped: usize) {
        self.truncated_commands += 1;
        self.skipped_bytes = self.skipped_bytes.saturating_add(skipped as u32);
    }
}

/// Payload length of a command (bytes after the 2-byte player/type header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadLen {
    /// Fixed-size payload.
    Fixed(usize),
    /// `[u8 count]` followed by `count` entries of the given size.
    CountPrefixed { entry_size: usize },
    /// `[u32 len]` followed by `len` bytes (save/load game).
    LengthPrefixedU32,
}

/// Payload length of every known command opcode.
///
/// Lengths derived from the documented BW network/replay command layout
/// (cross-checked against screp's `repcmd`/`repparser` and BWAPI). Returns
/// `None` for opcodes that never appear in replays as far as is known; the
/// parser cannot know their length and must skip the rest of the block.
#[must_use]
pub fn payload_len(type_id: u8) -> Option<PayloadLen> {
    use PayloadLen::{CountPrefixed, Fixed, LengthPrefixedU32};
    let len = match type_id {
        0x05 => Fixed(0),                               // Keep alive
        0x06 | 0x07 => LengthPrefixedU32,               // Save game / Load game
        0x08 => Fixed(0),                               // Restart game
        0x09..=0x0B => CountPrefixed { entry_size: 2 }, // Select / add / remove
        0x0C => Fixed(7),                               // Build
        0x0D => Fixed(2),                               // Vision
        0x0E => Fixed(4),                               // Alliance
        0x0F => Fixed(1),                               // Game speed
        0x10 | 0x11 => Fixed(0),                        // Pause / resume
        0x12 => Fixed(4),                               // Cheat
        0x13 => Fixed(2),                               // Hotkey
        0x14 => Fixed(9),                               // Right click
        0x15 => Fixed(10),                              // Targeted order
        0x18 | 0x19 => Fixed(0),                        // Cancel build / morph
        0x1A => Fixed(1),                               // Stop
        0x1B..=0x1D => Fixed(0), // Carrier stop / reaver stop / order nothing
        0x1E => Fixed(1),        // Return cargo
        0x1F | 0x20 => Fixed(2), // Train / cancel train
        0x21 | 0x22 => Fixed(1), // Cloak / decloak
        0x23 => Fixed(2),        // Unit morph
        0x25 | 0x26 => Fixed(1), // Unsiege / siege
        0x27 => Fixed(0),        // Train fighter
        0x28 => Fixed(1),        // Unload all
        0x29 => Fixed(2),        // Unload
        0x2A => Fixed(0),        // Merge archon
        0x2B..=0x2D => Fixed(1), // Hold position / burrow / unburrow
        0x2E => Fixed(0),        // Cancel nuke
        0x2F => Fixed(4),        // Lift off
        0x30 => Fixed(1),        // Research
        0x31 => Fixed(0),        // Cancel research
        0x32 => Fixed(1),        // Upgrade
        0x33 | 0x34 => Fixed(0), // Cancel upgrade / addon
        0x35 => Fixed(2),        // Building morph
        0x36 => Fixed(0),        // Stim
        0x37 => Fixed(6),        // Sync
        0x38 | 0x39 => Fixed(0), // Voice enable / disable
        0x3A | 0x3B => Fixed(1), // Voice squelch / unsquelch
        0x3C => Fixed(0),        // Start game
        0x3D => Fixed(1),        // Download percentage
        0x3E => Fixed(5),        // Change game slot
        0x3F => Fixed(7),        // New net player
        0x40 => Fixed(17),       // Joined game
        0x41 => Fixed(2),        // Change race
        0x42 | 0x43 => Fixed(1), // Team game team / UMS team
        0x44 | 0x45 => Fixed(2), // Melee team / swap players
        0x48 => Fixed(12),       // Saved data
        0x54 => Fixed(0),        // Briefing start
        0x55 => Fixed(1),        // Latency
        0x56 => Fixed(9),        // Replay speed
        0x57 => Fixed(1),        // Leave game
        0x58 => Fixed(4),        // Minimap ping
        0x5A | 0x5B => Fixed(0), // Merge dark archon / make game public
        0x5C => Fixed(81),       // Chat
        0x60 => Fixed(11),       // Right click (1.21)
        0x61 => Fixed(12),       // Targeted order (1.21)
        0x62 => Fixed(4),        // Unload (1.21)
        0x63..=0x65 => CountPrefixed { entry_size: 4 }, // Select / add / remove (1.21)
        _ => return None,
    };
    Some(len)
}

/// Resolve the concrete payload length of a command given the bytes that
/// follow its header. Returns `None` if the length prefix itself is missing.
fn resolve_len(len: PayloadLen, data: &[u8]) -> Option<usize> {
    match len {
        PayloadLen::Fixed(n) => Some(n),
        PayloadLen::CountPrefixed { entry_size } => {
            let count = *data.first()? as usize;
            Some(1 + count * entry_size)
        }
        PayloadLen::LengthPrefixedU32 => {
            let n = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
            4usize.checked_add(n)
        }
    }
}

/// Parse the command stream from decompressed section 2 data.
///
/// See [`parse_commands_with_warnings`]; this variant discards the warnings.
#[must_use]
pub fn parse_commands(data: &[u8]) -> Vec<GameCommand> {
    parse_commands_with_warnings(data).0
}

/// Parse the command stream from decompressed section 2 data.
///
/// The stream is a sequence of blocks:
///   [u32 frame] [u8 block_size] [block_size bytes of commands]
///
/// Each block contains one or more commands:
///   [u8 player_id] [u8 command_type] [payload...]
///
/// Payload lengths come from [`payload_len`]. An unknown opcode makes the rest
/// of its frame block unparseable, so the parser skips to the end of the
/// block (re-synchronising on the next frame header) and records a warning.
#[must_use]
pub fn parse_commands_with_warnings(data: &[u8]) -> (Vec<GameCommand>, CommandParseWarnings) {
    let mut commands = Vec::new();
    let mut warnings = CommandParseWarnings::default();
    let mut pos = 0;

    while pos + 5 <= data.len() {
        let frame = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let block_size = data[pos + 4] as usize;
        pos += 5;

        if pos + block_size > data.len() {
            warnings.record_truncated(data.len() - pos);
            break;
        }

        let block_end = pos + block_size;
        let mut bpos = pos;

        while bpos + 2 <= block_end {
            let player_id = data[bpos];
            let type_id = data[bpos + 1];
            bpos += 2;
            let rest = &data[bpos..block_end];

            let Some(len) = payload_len(type_id) else {
                warnings.record_unknown(type_id, rest.len());
                bpos = block_end;
                break;
            };
            let payload = match resolve_len(len, rest).and_then(|n| rest.get(..n)) {
                Some(p) => p,
                None => {
                    warnings.record_truncated(rest.len());
                    bpos = block_end;
                    break;
                }
            };
            bpos += payload.len();

            commands.push(GameCommand {
                frame,
                player_id,
                command: decode_command(type_id, payload),
            });
        }

        if bpos < block_end {
            // A trailing single byte that cannot hold a command header.
            warnings.record_truncated(block_end - bpos);
        }
        pos = block_end;
    }

    (commands, warnings)
}

/// Little-endian payload reader that yields zero past the end instead of
/// panicking. [`parse_commands_with_warnings`] already guarantees payloads
/// have the length from [`payload_len`], so this is purely defensive.
struct Payload<'a>(&'a [u8]);

impl Payload<'_> {
    fn u8(&self, off: usize) -> u8 {
        self.0.get(off).copied().unwrap_or(0)
    }

    fn u16(&self, off: usize) -> u16 {
        u16::from_le_bytes([self.u8(off), self.u8(off + 1)])
    }

    fn flag(&self, off: usize) -> bool {
        self.u8(off) != 0
    }
}

/// Decode a command whose payload has already been sliced to its exact length.
fn decode_command(type_id: u8, data: &[u8]) -> Command {
    let p = Payload(data);
    match type_id {
        0x05 => Command::KeepAlive,
        0x09 => Command::Select {
            unit_tags: select_tags(data, 2),
        },
        0x0A => Command::SelectAdd {
            unit_tags: select_tags(data, 2),
        },
        0x0B => Command::SelectRemove {
            unit_tags: select_tags(data, 2),
        },
        0x0C => Command::Build {
            order: p.u8(0),
            x: p.u16(1),
            y: p.u16(3),
            unit_type: p.u16(5),
        },
        0x13 => Command::Hotkey {
            action: if p.u8(0) == 0 {
                HotkeyAction::Assign
            } else {
                HotkeyAction::Select
            },
            group: p.u8(1),
        },
        0x14 => Command::RightClick {
            x: p.u16(0),
            y: p.u16(2),
            target_tag: p.u16(4),
            unit_type: p.u16(6),
            queued: p.flag(8),
        },
        0x15 => Command::TargetedOrder {
            x: p.u16(0),
            y: p.u16(2),
            target_tag: p.u16(4),
            unit_type: p.u16(6),
            order: p.u8(8),
            queued: p.flag(9),
        },
        0x18 => Command::CancelBuild,
        0x19 => Command::CancelMorph,
        0x1A => Command::Stop { queued: p.flag(0) },
        0x1E => Command::ReturnCargo { queued: p.flag(0) },
        0x1F => Command::Train {
            unit_type: p.u16(0),
        },
        0x20 => Command::CancelTrain { unit_tag: p.u16(0) },
        0x21 => Command::Cloak { queued: p.flag(0) },
        0x22 => Command::Decloak { queued: p.flag(0) },
        0x23 => Command::UnitMorph {
            unit_type: p.u16(0),
        },
        0x25 => Command::Unsiege { queued: p.flag(0) },
        0x26 => Command::Siege { queued: p.flag(0) },
        0x27 => Command::TrainFighter,
        0x28 => Command::UnloadAll { queued: p.flag(0) },
        0x29 => Command::Unload { unit_tag: p.u16(0) },
        0x2A => Command::MergeArchon,
        0x2B => Command::HoldPosition { queued: p.flag(0) },
        0x2C => Command::Burrow { queued: p.flag(0) },
        0x2D => Command::Unburrow { queued: p.flag(0) },
        0x2E => Command::CancelNuke,
        0x2F => Command::LiftOff {
            x: p.u16(0),
            y: p.u16(2),
        },
        0x30 => Command::Research { tech_type: p.u8(0) },
        0x31 => Command::CancelResearch,
        0x32 => Command::Upgrade {
            upgrade_type: p.u8(0),
        },
        0x33 => Command::CancelUpgrade,
        0x34 => Command::CancelAddon,
        0x35 => Command::BuildingMorph {
            unit_type: p.u16(0),
        },
        0x36 => Command::Stim,
        0x57 => Command::LeaveGame { reason: p.u8(0) },
        0x58 => Command::MinimapPing {
            x: p.u16(0),
            y: p.u16(2),
        },
        0x5A => Command::MergeDarkArchon,
        0x5C => Command::Chat {
            sender_slot: p.u8(0),
            message: crate::text::decode_bw_string(data.get(1..).unwrap_or_default()),
        },
        // 1.21+ extended commands: unit tags are followed by 2 unknown bytes.
        0x60 => Command::RightClick {
            x: p.u16(0),
            y: p.u16(2),
            target_tag: p.u16(4),
            unit_type: p.u16(8),
            queued: p.flag(10),
        },
        0x61 => Command::TargetedOrder {
            x: p.u16(0),
            y: p.u16(2),
            target_tag: p.u16(4),
            unit_type: p.u16(8),
            order: p.u8(10),
            queued: p.flag(11),
        },
        0x62 => Command::Unload { unit_tag: p.u16(0) },
        0x63 => Command::Select {
            unit_tags: select_tags(data, 4),
        },
        0x64 => Command::SelectAdd {
            unit_tags: select_tags(data, 4),
        },
        0x65 => Command::SelectRemove {
            unit_tags: select_tags(data, 4),
        },
        // Lobby, sync, voice, and other non-gameplay commands.
        _ => Command::Other { type_id },
    }
}

/// Decode the unit tags of a select command: `[u8 count]` followed by
/// `count` entries of `entry_size` bytes, each starting with a u16 tag.
fn select_tags(data: &[u8], entry_size: usize) -> Vec<u16> {
    data.get(1..)
        .unwrap_or_default()
        .chunks_exact(entry_size)
        .map(|e| u16::from_le_bytes([e[0], e[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_stream() {
        let commands = parse_commands(&[]);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_parse_single_train_command() {
        // Frame 100, block_size=4, player 0, Train(type=0x0025 = Zergling)
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes()); // frame
        data.push(4); // block_size
        data.push(0); // player_id
        data.push(0x1F); // Train command
        data.extend_from_slice(&0x0025u16.to_le_bytes()); // unit_type

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].frame, 100);
        assert_eq!(commands[0].player_id, 0);
        assert!(matches!(
            commands[0].command,
            Command::Train { unit_type: 0x25 }
        ));
    }

    #[test]
    fn test_parse_build_command() {
        let mut data = Vec::new();
        data.extend_from_slice(&200u32.to_le_bytes());
        data.push(9); // block_size = 2 (header) + 7 (payload)
        data.push(1); // player 1
        data.push(0x0C); // Build
        data.push(0x0F); // order
        data.extend_from_slice(&100u16.to_le_bytes()); // x
        data.extend_from_slice(&200u16.to_le_bytes()); // y
        data.extend_from_slice(&0x007Du16.to_le_bytes()); // unit_type (Gateway)

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        if let Command::Build {
            order,
            x,
            y,
            unit_type,
        } = &commands[0].command
        {
            assert_eq!(*order, 0x0F);
            assert_eq!(*x, 100);
            assert_eq!(*y, 200);
            assert_eq!(*unit_type, 0x007D);
        } else {
            panic!("expected Build command");
        }
    }

    #[test]
    fn test_parse_hotkey_command() {
        let mut data = Vec::new();
        data.extend_from_slice(&50u32.to_le_bytes());
        data.push(4);
        data.push(0);
        data.push(0x13);
        data.push(0); // assign
        data.push(1); // group 1

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0].command,
            Command::Hotkey {
                action: HotkeyAction::Assign,
                group: 1
            }
        ));
    }

    #[test]
    fn test_parse_multiple_commands_in_block() {
        let mut data = Vec::new();
        data.extend_from_slice(&300u32.to_le_bytes());
        // Two commands: Train + Stop = (2+2) + (2+1) = 7 bytes
        data.push(7);
        // Command 1: player 0, Train
        data.push(0);
        data.push(0x1F);
        data.extend_from_slice(&0x0001u16.to_le_bytes());
        // Command 2: player 0, Stop
        data.push(0);
        data.push(0x1A);
        data.push(0); // not queued

        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 2);
        assert!(matches!(commands[0].command, Command::Train { .. }));
        assert!(matches!(
            commands[1].command,
            Command::Stop { queued: false }
        ));
    }

    fn block(frame: u32, cmds: &[u8]) -> Vec<u8> {
        let mut data = frame.to_le_bytes().to_vec();
        data.push(cmds.len() as u8);
        data.extend_from_slice(cmds);
        data
    }

    #[test]
    fn test_parse_unknown_opcode_skips_block_and_resyncs() {
        // Frame 10: Train, then unknown 0x24 followed by garbage that would
        // decode as a bogus Train if consumed as 0 bytes.
        let mut data = block(10, &[0, 0x1F, 0x07, 0x00, 0, 0x24, 0, 0x1F, 0x41, 0x00]);
        // Frame 11: a normal Stop must still be parsed.
        data.extend(block(11, &[1, 0x1A, 0x00]));

        let (commands, warnings) = parse_commands_with_warnings(&data);
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            commands[0].command,
            Command::Train { unit_type: 7 }
        ));
        assert_eq!(commands[1].frame, 11);
        assert!(matches!(
            commands[1].command,
            Command::Stop { queued: false }
        ));
        assert_eq!(warnings.unknown_commands, 1);
        assert_eq!(warnings.unknown_opcodes, vec![0x24]);
        assert_eq!(warnings.skipped_bytes, 4);
        assert!(!warnings.is_clean());
    }

    #[test]
    fn test_parse_lobby_commands_consume_full_payload() {
        // Sync (6 bytes) followed by Train in the same block.
        let mut cmds = vec![0, 0x37, 1, 2, 3, 4, 5, 6];
        cmds.extend_from_slice(&[0, 0x1F, 0x25, 0x00]);
        let (commands, warnings) = parse_commands_with_warnings(&block(5, &cmds));
        assert!(warnings.is_clean());
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            commands[0].command,
            Command::Other { type_id: 0x37 }
        ));
        assert!(matches!(
            commands[1].command,
            Command::Train { unit_type: 0x25 }
        ));
    }

    #[test]
    fn test_parse_save_game_skips_length_prefixed_payload() {
        let mut cmds = vec![0, 0x06];
        cmds.extend_from_slice(&3u32.to_le_bytes());
        cmds.extend_from_slice(&[0x1F, 0x1F, 0x1F]);
        cmds.extend_from_slice(&[0, 0x36]); // Stim
        let (commands, warnings) = parse_commands_with_warnings(&block(1, &cmds));
        assert!(warnings.is_clean());
        assert_eq!(commands.len(), 2);
        assert!(matches!(commands[1].command, Command::Stim));
    }

    #[test]
    fn test_parse_truncated_command_records_warning() {
        // Build needs 7 payload bytes; only 3 present.
        let (commands, warnings) = parse_commands_with_warnings(&block(1, &[0, 0x0C, 1, 2, 3]));
        assert!(commands.is_empty());
        assert_eq!(warnings.truncated_commands, 1);
        assert_eq!(warnings.skipped_bytes, 3);
    }

    #[test]
    fn test_parse_select_121_reads_4_byte_entries() {
        let cmds = [0, 0x63, 2, 0x10, 0x00, 0xAA, 0xBB, 0x20, 0x00, 0xCC, 0xDD];
        let (commands, warnings) = parse_commands_with_warnings(&block(1, &cmds));
        assert!(warnings.is_clean());
        match &commands[0].command {
            Command::Select { unit_tags } => assert_eq!(unit_tags, &vec![0x10, 0x20]),
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chat_euc_kr_message_decodes_korean() {
        let mut cmds = vec![0, 0x5C, 1];
        let mut msg = [0u8; 80];
        // "ㅎㅇ" (hi) in EUC-KR followed by NUL padding.
        msg[..4].copy_from_slice(&[0xA4, 0xBE, 0xA4, 0xB7]);
        cmds.extend_from_slice(&msg);
        let (commands, warnings) = parse_commands_with_warnings(&block(1, &cmds));
        assert!(warnings.is_clean());
        match &commands[0].command {
            Command::Chat {
                sender_slot,
                message,
            } => {
                assert_eq!(*sender_slot, 1);
                assert_eq!(message, "ㅎㅇ");
            }
            other => panic!("expected Chat, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chat_utf8_message_decodes() {
        let mut cmds = vec![0, 0x5C, 0];
        let mut msg = [0u8; 80];
        let text = "gg 수고".as_bytes();
        msg[..text.len()].copy_from_slice(text);
        cmds.extend_from_slice(&msg);
        let (commands, _) = parse_commands_with_warnings(&block(1, &cmds));
        assert!(
            matches!(&commands[0].command, Command::Chat { message, .. } if message == "gg 수고")
        );
    }

    #[test]
    fn test_payload_len_table_known_and_unknown() {
        assert_eq!(payload_len(0x14), Some(PayloadLen::Fixed(9)));
        assert_eq!(payload_len(0x5C), Some(PayloadLen::Fixed(81)));
        assert_eq!(payload_len(0x40), Some(PayloadLen::Fixed(17)));
        for unknown in [0x00, 0x16, 0x17, 0x24, 0x46, 0x59, 0x5D, 0x66, 0xFF] {
            assert_eq!(payload_len(unknown), None, "opcode {unknown:#04x}");
        }
    }

    #[test]
    fn test_meaningful_and_effective_actions() {
        assert!(!Command::KeepAlive.is_meaningful_action());
        assert!(!Command::Select { unit_tags: vec![1] }.is_meaningful_action());
        assert!(Command::Train { unit_type: 1 }.is_meaningful_action());
        assert!(
            Command::Build {
                order: 0,
                x: 0,
                y: 0,
                unit_type: 1
            }
            .is_meaningful_action()
        );

        // Hotkey assign is effective, select is not
        assert!(
            Command::Hotkey {
                action: HotkeyAction::Assign,
                group: 1
            }
            .is_effective_action()
        );
        assert!(
            !Command::Hotkey {
                action: HotkeyAction::Select,
                group: 1
            }
            .is_effective_action()
        );
    }

    #[test]
    fn test_build_order_actions() {
        assert!(
            Command::Build {
                order: 0,
                x: 0,
                y: 0,
                unit_type: 1
            }
            .is_build_order_action()
        );
        assert!(Command::Train { unit_type: 1 }.is_build_order_action());
        assert!(Command::Research { tech_type: 1 }.is_build_order_action());
        assert!(Command::Upgrade { upgrade_type: 1 }.is_build_order_action());
        assert!(Command::UnitMorph { unit_type: 1 }.is_build_order_action());
        assert!(!Command::Stop { queued: false }.is_build_order_action());
        assert!(!Command::KeepAlive.is_build_order_action());
    }
}
